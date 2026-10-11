//! The new app (CONTRACT §2): `Lantern` owns the engine, the model, what's being typed and the
//! windows. Engine events update the model (`engine.rs`); what people do arrives as `UiAction`s
//! and becomes effects in exactly one place (`apply.rs`); the pill and panel windows live in
//! `windows.rs`. Runs with `--shell` until it replaces the legacy UI.
// The new UI path is built while the legacy UI is still the default (CONTRACT §10).
#![allow(dead_code)]

/// Event trace for hands-on checks: `LANTERN_DEBUG=1`.
macro_rules! trace {
    ($($arg:tt)*) => {
        if std::env::var_os("LANTERN_DEBUG").is_some() {
            eprintln!("lantern {:>8.2}s: {}", $crate::app::uptime(), format!($($arg)*));
        }
    };
}
pub(crate) use trace;

static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
pub(crate) fn uptime() -> f64 {
    STARTED.get_or_init(Instant::now).elapsed().as_secs_f64()
}

mod apply;
mod engine;
pub mod fixture;
mod windows;

use crate::model::{Inputs, Model, drafts::Ticket, inbox::Turns};
use crate::sidecar::Sidecar;
use crate::ui::{Assets, UiAction, UiState};
use anyhow::Result;
use eframe::egui;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A request the engine hasn't answered yet.
#[derive(Clone, Debug)]
enum Pending {
    /// The ticket says which draft revision was sent, so only that is cleared when it arrives.
    Reply(Ticket),
    Keys {
        agent_id: String,
    },
    Focus {
        agent_id: String,
    },
    History {
        agent_id: String,
    },
}

pub struct Lantern {
    engine: Sidecar,
    connected: bool,
    model: Model,
    inputs: Inputs,
    turns: Turns,
    view: UiState,
    assets: Option<Assets>,
    windows: windows::Windows,
    next_request: u64,
    pending: HashMap<String, Pending>,
    /// The history request whose answer may update the open chat; older answers are stale.
    history_request: Option<String>,
    history_at: Instant,
    toast_until: Option<Instant>,
    actions: Vec<UiAction>,
    started: Instant,
}

impl Lantern {
    fn new(engine: Sidecar) -> Self {
        let mut model = Model { caps: capabilities(), ..Default::default() };
        model.system.version = env!("CARGO_PKG_VERSION").into();
        Self {
            engine,
            connected: false,
            model,
            inputs: Inputs::default(),
            turns: Turns::default(),
            view: UiState::default(),
            assets: None,
            windows: windows::Windows::default(),
            next_request: 0,
            pending: HashMap::new(),
            history_request: None,
            history_at: Instant::now(),
            toast_until: None,
            actions: Vec::new(),
            started: Instant::now(),
        }
    }

    /// Sends a request to the engine with a fresh id; `None` when the engine isn't there.
    fn request(&mut self, mut request: Value) -> Option<String> {
        if !self.connected {
            return None;
        }
        self.next_request += 1;
        let id = format!("shell-{}", self.next_request);
        request["requestId"] = id.clone().into();
        match self.engine.send(&request) {
            Ok(()) => Some(id),
            Err(error) => {
                self.connected = false;
                self.show(format!("Lantern's engine isn't answering: {error:#}"));
                None
            }
        }
    }

    /// A short message at the bottom of the panel, for four seconds.
    fn show(&mut self, text: impl Into<String>) {
        let id = self.model.toast.as_ref().map_or(1, |toast| toast.id + 1);
        self.model.toast = Some(crate::model::Toast { id, text: text.into() });
        self.toast_until = Some(Instant::now() + Duration::from_secs(4));
    }

    fn now_ms() -> f64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }

    fn tick(&mut self) {
        if self.toast_until.is_some_and(|until| Instant::now() >= until) {
            self.toast_until = None;
            self.model.toast = None;
        }
        // While a conversation is open it refreshes every two seconds, so a long turn shows its
        // progress (snapshots only arrive when an agent's state changes).
        if self.model.chat.is_some() && self.history_at.elapsed() >= Duration::from_secs(2) {
            self.refresh_chat();
        }
    }
}

/// What this platform can do (CONTRACT §8). Only what has been proven is true.
fn capabilities() -> crate::model::Caps {
    use crate::model::{Caps, Platform};
    if cfg!(windows) {
        // Unverified on a real machine until tested (CONTRACT §8.2c); the polled halo is the plan.
        Caps { platform: Platform::Windows, edge_pill: true, hover_halo: true, dictation: true, global_hotkeys: true, ..Default::default() }
    } else {
        Caps {
            platform: Platform::Linux,
            edge_pill: crate::os::shell::x11_available(),
            dictation: true,
            global_hotkeys: true,
            ..Default::default()
        }
    }
}

impl eframe::App for Lantern {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(event) = self.engine.events.try_recv() {
            match event {
                Ok(event) => self.engine_event(event),
                Err(error) => {
                    self.connected = false;
                    self.model.engine = crate::model::EngineHealth::Failed(format!("{error:#}"));
                }
            }
        }
        for event in self.engine.take_latest() {
            self.engine_event(event);
        }
        self.tick();
        self.assets.get_or_insert_with(|| Assets::load(&ctx));
        self.frame(ui);
        for action in std::mem::take(&mut self.actions) {
            self.apply(action);
        }
        // TODO(apollo): wake on engine events instead of polling (the legacy event pump).
        ctx.request_repaint_after(Duration::from_millis(if self.windows.animating() { 16 } else { 250 }));
    }
}

/// `--shell`: the Mac-parity app (pill + panel). The legacy window stays the default until it's done.
pub fn run(terminal: &str) -> Result<()> {
    let engine = Sidecar::with_terminal(terminal)?;
    let options = windows::options();
    eframe::run_native(
        crate::os::shell::PILL_TITLE,
        options,
        Box::new(move |cc| {
            crate::ui::theme::install(&cc.egui_ctx);
            Ok(Box::new(Lantern::new(engine)))
        }),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
