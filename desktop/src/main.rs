//! Native Rust desktop. Discovery, targeting and agent delivery stay in the Rust sidecar.
// Release builds on Windows are windowed apps: no console window behind Lantern.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod account;
mod app;
mod legacy_ui;
mod model;
mod os;
mod settings;
mod shell;
mod sidecar;
mod state;
mod ui;
mod updates;

use eframe::{egui, icon_data::IconDataExt};
use egui::{Color32, RichText};
use os::Event;
use serde_json::{Value, json};
use settings::Settings;
use sidecar::Sidecar;
use state::{Draft, PendingReply};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{Receiver, Sender},
};
use std::time::{Duration, Instant};

const ACCENT: Color32 = Color32::from_rgb(242, 153, 80);
const MUTED: Color32 = Color32::from_rgb(139, 148, 162);
fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn short(text: &str, count: usize) -> String {
    let mut result: String = text.chars().take(count).collect();
    if text.chars().count() > count {
        result.push('…');
    }
    result
}
fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}
fn state_color(state: &str) -> Color32 {
    match state {
        "working" => Color32::from_rgb(104, 178, 247),
        "needs_input" => ACCENT,
        "waiting" => Color32::from_rgb(115, 204, 160),
        _ => MUTED,
    }
}
fn state_label(state: &str) -> &str {
    match state {
        "needs_input" => "Needs you",
        "waiting" => "Ready",
        "working" => "Working",
        "idle" => "Idle",
        _ => "Unknown",
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Sessions,
    Ask,
    Connections,
    Settings,
    Help,
}

struct Lantern {
    engine: Sidecar,
    connected: bool,
    agents: Vec<Value>,
    selected: Option<String>,
    messages: Vec<Value>,
    markdown: egui_commonmark::CommonMarkCache,
    history_pending: Option<String>,
    history_at: Instant,
    pending_reply: Option<PendingReply>,
    drafts: HashMap<String, Draft>,
    bridge_snapshot: Arc<std::sync::RwLock<String>>,
    status: String,
    installed: Vec<String>,
    kind: String,
    model: String,
    models: Vec<Value>,
    choosable: bool,
    cwd: String,
    new_draft: Draft,
    background: bool,
    launch_pending: Option<String>,
    next_request: u64,
    show_new: bool,
    page: Page,
    filter: String,
    needs_only: bool,
    settings: Settings,
    settings_error: Option<String>,
    events: Receiver<Event>,
    tx: Sender<Event>,
    tray: bool,
    can_hide: bool,
    compact: bool,
    window_mode_pending: bool,
    expanded_size: egui::Vec2,
    quitting: bool,
    shortcuts: bool,
    shortcuts_pending: bool,
    capture_pending: bool,
    capture_generation: u64,
    show_welcome: bool,
    uninstall_confirm: bool,
    textures: HashMap<PathBuf, egui::TextureHandle>,
    logo: egui::TextureHandle,
    recording: Option<(os::Recording, String, u64)>,
    speech_generation: u64,
    speech_busy: bool,
    october: Value,
    phone: Value,
    session: Option<account::Session>,
    auth_generation: Arc<AtomicU64>,
    auth_pending: bool,
    vault: Sender<account::VaultCommand>,
    remember: bool,
    email: String,
    password: String,
    ask_draft: Draft,
    ask_answer: String,
    ask_model: String,
    ask_generation: Arc<AtomicU64>,
    ask_history: Vec<Value>,
    ask_question: String,
    ask_agent: String,
    ask_pending: bool,
    update: Option<updates::Release>,
    update_pending: bool,
    update_checked: Instant,
    snapshot: Option<PathBuf>,
    snapshot_requested: bool,
    started: Instant,
}

impl Lantern {
    fn new(
        engine: Sidecar,
        settings: Settings,
        settings_error: Option<String>,
        snapshot: Option<PathBuf>,
        page: Page,
        ctx: &egui::Context,
        wake: impl Fn(bool) + Send + 'static,
    ) -> Self {
        let (tx, events) = os::event_pump(wake);
        if snapshot.is_none() {
            std::thread::spawn(os::prune_screenshots);
            os::tray(tx.clone());
        }
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../logo.png")).expect("bundled icon");
        let logo = ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba),
            Default::default(),
        );
        let auth_generation = Arc::new(AtomicU64::new(1));
        let bridge_snapshot = Arc::new(std::sync::RwLock::new("[]".to_string()));
        if snapshot.is_none() {
            os::bridge(tx.clone(), bridge_snapshot.clone());
        }
        let status = settings_error.clone().unwrap_or_else(|| "Connecting to your agents…".into());
        if snapshot.is_none() && settings.global_shortcuts {
            os::shortcuts(tx.clone());
        }
        let vault = account::vault(tx.clone());
        if snapshot.is_none() {
            engine.observe(tx.clone(), bridge_snapshot.clone(), settings.notifications, settings.notify_finished);
            account::Flow {
                generation: 1,
                current: auth_generation.clone(),
                engine: engine.sender(),
                vault: vault.clone(),
                remember: true,
                events: tx.clone(),
            }
            .start(account::restore);
        }
        let shortcuts_pending = settings.global_shortcuts && snapshot.is_none();
        let show_welcome = !settings.welcome_seen && snapshot.is_none();
        let app = Self {
            engine,
            connected: false,
            agents: Vec::new(),
            selected: None,
            messages: Vec::new(),
            markdown: Default::default(),
            history_pending: None,
            history_at: Instant::now(),
            pending_reply: None,
            drafts: HashMap::new(),
            bridge_snapshot,
            status,
            installed: Vec::new(),
            kind: String::new(),
            model: String::new(),
            models: Vec::new(),
            choosable: false,
            cwd: std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned(),
            new_draft: Default::default(),
            background: true,
            launch_pending: None,
            next_request: 0,
            show_new: false,
            page,
            filter: String::new(),
            needs_only: false,
            settings,
            settings_error,
            events,
            tx,
            tray: false,
            can_hide: true,
            compact: false,
            window_mode_pending: false,
            expanded_size: egui::vec2(1060.0, 760.0),
            quitting: false,
            shortcuts: false,
            shortcuts_pending,
            capture_pending: false,
            capture_generation: 0,
            show_welcome,
            uninstall_confirm: false,
            textures: HashMap::new(),
            logo,
            recording: None,
            speech_generation: 0,
            speech_busy: false,
            october: Value::Null,
            phone: Value::Null,
            session: None,
            auth_generation,
            auth_pending: false,
            vault,
            remember: true,
            email: String::new(),
            password: String::new(),
            ask_draft: Default::default(),
            ask_answer: String::new(),
            ask_model: String::new(),
            ask_generation: Arc::new(AtomicU64::new(0)),
            ask_history: Vec::new(),
            ask_question: String::new(),
            ask_agent: String::new(),
            ask_pending: false,
            update: None,
            update_pending: false,
            update_checked: Instant::now() - Duration::from_secs(86400),
            snapshot,
            snapshot_requested: false,
            started: Instant::now(),
        };
        app.appearance(ctx);
        app
    }
    fn appearance(&self, ctx: &egui::Context) {
        let mut visuals = if self.settings.light { egui::Visuals::light() } else { egui::Visuals::dark() };
        visuals.selection.bg_fill = if self.settings.light { Color32::from_rgb(245, 216, 192) } else { Color32::from_rgb(78, 52, 35) };
        visuals.selection.stroke.color = ACCENT;
        visuals.widgets.active.bg_fill = ACCENT;
        visuals.widgets.hovered.bg_stroke.color = ACCENT;
        if !self.settings.light {
            visuals.panel_fill = Color32::from_rgb(19, 23, 29);
            visuals.window_fill = Color32::from_rgb(26, 31, 39);
            visuals.extreme_bg_color = Color32::from_rgb(14, 18, 24);
            visuals.faint_bg_color = Color32::from_rgb(29, 35, 44);
        }
        ctx.set_visuals(visuals);
        ctx.set_zoom_factor(self.settings.scale);
        ctx.all_styles_mut(|style| {
            style.wrap_mode = Some(egui::TextWrapMode::Wrap);
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
            style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(23.0));
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.settings.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
    }
    fn request(&mut self, mut request: Value) -> Option<String> {
        if !self.connected {
            return None;
        }
        self.next_request += 1;
        let id = format!("desktop-{}", self.next_request);
        request["requestId"] = id.clone().into();
        if let Err(error) = self.engine.send(&request) {
            self.connected = false;
            self.status = format!("Engine: {error:#}");
            return None;
        }
        Some(id)
    }
    fn history(&mut self) {
        if self.history_pending.is_none()
            && let Some(id) = self.selected.clone()
        {
            self.history_pending = self.request(json!({"type":"history","agentId":id}));
            self.history_at = Instant::now();
        }
    }
    fn select(&mut self, id: String) {
        if self.selected.as_deref() != Some(&id) {
            self.cancel_speech();
            self.selected = Some(id);
            self.messages.clear();
            self.history_pending = None;
            self.history();
        }
        self.page = Page::Sessions;
    }
    fn cancel_speech(&mut self) {
        self.recording.take();
        self.speech_generation += 1;
        self.speech_busy = false;
    }
    fn event(&mut self, event: Value) {
        match string(&event, "type") {
            "hello" => {
                self.connected = event["protocol"] == 3;
                self.status = if self.connected {
                    "All caught up. Your agents are close by.".into()
                } else {
                    "Unsupported engine protocol; rebuild desktop and sidecar together".into()
                };
            }
            "snapshot" => {
                let agents = event["agents"].as_array().cloned().unwrap_or_default();
                self.agents = agents;
                if self.selected.as_ref().is_some_and(|id| !self.agents.iter().any(|agent| string(agent, "id") == id)) {
                    self.cancel_speech();
                    self.selected = None;
                    self.messages.clear();
                    self.history_pending = None;
                }
                if self.selected.is_none()
                    && let Some(agent) = self.agents.iter().find(|agent| agent["state"] == "needs_input").or(self.agents.first())
                {
                    self.selected = Some(string(agent, "id").into());
                    self.history();
                }
            }
            "installed" => {
                self.installed = event["installed"]["kinds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect();
                if self.kind.is_empty() {
                    self.kind = self.installed.first().cloned().unwrap_or_default();
                    self.load_models();
                }
            }
            "models" if event["kind"] == self.kind => {
                self.models = event["models"].as_array().cloned().unwrap_or_default();
                self.choosable = event["choosable"] == true;
            }
            "historyResult" if self.history_pending.as_deref() == Some(string(&event, "requestId")) => {
                self.history_pending = None;
                if self.selected.as_deref() == Some(string(&event, "agentId")) && event["busy"] != true {
                    self.messages = event["messages"].as_array().cloned().unwrap_or_default();
                }
            }
            "replyResult" if self.pending_reply.as_ref().is_some_and(|pending| pending.request == string(&event, "requestId")) => {
                let pending = self.pending_reply.take().expect("matched reply");
                if event["ok"] == true {
                    state::clear_delivered(&mut self.drafts, &pending);
                }
                self.status = event["message"]
                    .as_str()
                    .unwrap_or(if event["ok"] == true { "Reply delivered" } else { "Reply failed; draft kept" })
                    .into();
                self.history();
            }
            "launchResult" if self.launch_pending.as_deref() == Some(string(&event, "requestId")) => {
                self.launch_pending = None;
                if event["ok"] == true {
                    self.new_draft = Default::default();
                    self.show_new = false;
                    self.status = format!("Session started. {}", event["warning"].as_str().unwrap_or(""));
                } else {
                    self.status = event["message"].as_str().unwrap_or("Launch failed").into();
                }
            }
            "attachResult" => {
                self.status = if event["ok"] == true {
                    "Terminal opened".into()
                } else {
                    event["message"].as_str().unwrap_or("Could not open terminal").into()
                };
            }
            "october" => {
                self.october = event["october"].clone();
            }
            "phone" => {
                self.phone = event;
            }
            "bus" => {
                self.status = event["message"]
                    .as_str()
                    .unwrap_or(if event["ready"] == true {
                        "October Bus binary is available; sessions start its daemon"
                    } else {
                        "Bus could not start"
                    })
                    .into();
            }
            "toolkit" => {
                self.status = if event["ok"] == true {
                    format!("Toolkit refreshed: {}", string(&event, "path"))
                } else {
                    event["message"].as_str().unwrap_or("Toolkit refresh failed").into()
                };
            }
            _ => {}
        }
    }
    fn load_models(&mut self) {
        self.model.clear();
        self.models.clear();
        self.choosable = false;
        self.request(json!({"type":"models","kind":self.kind}));
    }
    fn integration(&mut self, event: Event, ctx: &egui::Context) {
        match event {
            Event::Status(status) => {
                self.status = status;
            }
            Event::Screenshot { owner, generation, result } => {
                self.capture_pending = false;
                if generation != self.capture_generation {
                    return;
                }
                match result {
                    Ok(path) => {
                        self.draft_for(&owner).screenshot = Some(path);
                        self.status = "Screenshot attached. Review it before sending.".into();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    }
                    Err(error) => {
                        self.status = error;
                    }
                }
            }
            Event::Folder(result) => match result {
                Ok(path) => self.cwd = path.to_string_lossy().into_owned(),
                Err(error) => self.status = error,
            },
            Event::Transcript { owner, generation, result } if generation == self.speech_generation => {
                self.speech_busy = false;
                match result {
                    Ok(text) => {
                        let draft = self.draft_for(&owner);
                        if !draft.text.is_empty() {
                            draft.text.push(' ');
                        }
                        draft.text.push_str(&text);
                        self.status = "Dictation added to your draft".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Event::Transcript { .. } => {}
            Event::Activate(id) => {
                self.set_compact(false, ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                match id.as_str() {
                    "settings" => self.page = Page::Settings,
                    "connections" => self.page = Page::Connections,
                    "ask" => self.page = Page::Ask,
                    "new" => self.show_new = true,
                    "compact" => self.set_compact(true, ctx),
                    "help" => self.page = Page::Help,
                    "welcome" => self.show_welcome = true,
                    _ => {}
                }
                if id == "capture" {
                    self.page = Page::Ask;
                    self.capture("__ask".into());
                } else if id != "show" && self.agents.iter().any(|agent| string(agent, "id") == id) {
                    self.select(id);
                }
            }
            Event::Compact => {
                self.set_compact(!self.compact, ctx);
            }
            Event::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Event::Tray(available) => {
                if self.tray && !available {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
                self.tray = available;
            }
            Event::Shortcuts(available) => {
                self.shortcuts = available;
                if available {
                    self.settings.global_shortcuts = true;
                    if let Err(error) = self.settings.save() {
                        self.status = format!("Could not save shortcut preference: {error:#}");
                    }
                }
                self.shortcuts_pending = false;
                if available {
                    self.status = "Global shortcuts registered with your desktop".into();
                }
            }
            Event::Account { generation, result } if generation == self.auth_generation.load(Ordering::SeqCst) => {
                let was_pending = self.auth_pending;
                self.auth_pending = false;
                match result {
                    Ok(session) => {
                        self.status = format!("Signed in as {}", session.email);
                        self.session = Some(session);
                    }
                    Err(error) => {
                        if self.session.is_some() {
                            self.session = None;
                        }
                        if was_pending || error.starts_with("Session expired") {
                            self.status = error;
                        }
                    }
                }
            }
            Event::Account { .. } => {}
            Event::Ask { generation, result } if generation == self.ask_generation.load(Ordering::SeqCst) => {
                self.ask_pending = false;
                match result {
                    Ok(answer) => {
                        self.ask_history.push(json!({"role":"user","content":self.ask_question}));
                        self.ask_history.push(json!({"role":"assistant","content":answer}));
                        if self.ask_history.len() > 20 {
                            self.ask_history.drain(..self.ask_history.len() - 20);
                        }
                        self.ask_answer = answer;
                    }
                    Err(error) => self.status = error,
                }
            }
            Event::Ask { .. } => {}
            Event::AskDelta { generation, text } if generation == self.ask_generation.load(Ordering::SeqCst) => {
                self.ask_answer.push_str(&text);
            }
            Event::AskDelta { .. } => {}
            Event::Update(result) => {
                self.update_pending = false;
                match result {
                    Ok(release) => {
                        self.status = if release.is_newer() {
                            format!("{} update available: {}", updates::system_name(), release.version)
                        } else {
                            "Lantern is up to date".into()
                        };
                        self.update = Some(release);
                    }
                    Err(error) => self.status = error,
                }
            }
        }
    }
    fn draft_for(&mut self, owner: &str) -> &mut Draft {
        match owner {
            "__new" => &mut self.new_draft,
            "__ask" => &mut self.ask_draft,
            _ => self.drafts.entry(owner.into()).or_default(),
        }
    }
    fn capture(&mut self, owner: String) {
        if self.capture_pending {
            return;
        }
        self.capture_pending = true;
        self.capture_generation += 1;
        let generation = self.capture_generation;
        self.status = "Choose a screen, window or area in your desktop's screenshot dialog…".into();
        os::task(self.tx.clone(), os::screenshot, move |result| Event::Screenshot { owner, generation, result });
    }
    fn dictation(&mut self, owner: String) {
        if let Some((recording, recorded_owner, generation)) = self.recording.take() {
            self.speech_busy = true;
            let settings = self.settings.clone();
            self.status = "Transcribing on your computer…".into();
            os::task(
                self.tx.clone(),
                move || recording.finish(settings),
                move |result| Event::Transcript { owner: recorded_owner, generation, result },
            );
        } else {
            if self.settings.whisper_model.is_empty() || !std::path::Path::new(&self.settings.whisper_model).is_file() {
                self.page = Page::Settings;
                self.status = "Configure the local speech model in Settings first".into();
                return;
            }
            self.speech_generation += 1;
            match os::Recording::start() {
                Ok(recording) => {
                    self.recording = Some((recording, owner, self.speech_generation));
                    self.status = "Listening. Click Stop dictation when you're done.".into();
                }
                Err(error) => self.status = format!("{error:#}"),
            }
        }
    }
    fn set_compact(&mut self, compact: bool, ctx: &egui::Context) {
        if self.compact == compact {
            return;
        }
        if compact {
            self.expanded_size =
                ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size())).unwrap_or(egui::vec2(1060.0, 760.0));
        }
        self.compact = compact;
        self.window_mode_pending = true;
    }
    fn apply_window_mode(&mut self, ctx: &egui::Context) {
        if !self.window_mode_pending {
            return;
        }
        self.window_mode_pending = false;
        let compact = self.compact;
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!compact));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(if compact {
            egui::vec2(320.0, 84.0)
        } else {
            egui::vec2(680.0, 500.0)
        }));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(if compact { egui::vec2(360.0, 104.0) } else { self.expanded_size }));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if compact || self.settings.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
    }
    fn send_reply(&mut self, agent: &Value) {
        if self.pending_reply.is_some() || agent["canReply"] != true {
            return;
        }
        let id = string(agent, "id").to_owned();
        let draft = self.drafts.get(&id).cloned().unwrap_or_default();
        if draft.text.trim().is_empty() {
            return;
        }
        self.cancel_speech();
        let mut text = draft.text.clone();
        if let Some(path) = &draft.screenshot {
            text.push_str(&format!("\n\nScreenshot attached at {}. Please open this image.", path.display()));
        }
        if let Some(request) = self.request(json!({"type":"reply","agentId":id,"text":text})) {
            self.pending_reply = Some(PendingReply { request, agent: id, draft: Some(draft) });
            self.status = "Sending reply…".into();
        }
    }
    fn screenshot_preview(&mut self, ui: &mut egui::Ui, owner: &str) {
        let Some(path) = self.draft_for(owner).screenshot.clone() else {
            return;
        };
        if !self.textures.contains_key(&path)
            && let Ok(bytes) = std::fs::read(&path)
            && let Ok(icon) = eframe::icon_data::from_png_bytes(&bytes)
        {
            let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
            if self.textures.len() >= 8 {
                self.textures.clear();
            }
            self.textures.insert(path.clone(), ui.ctx().load_texture(path.to_string_lossy(), image, Default::default()));
        }
        ui.horizontal(|ui| {
            if let Some(texture) = self.textures.get(&path) {
                ui.add(egui::Image::new(texture).max_size(egui::vec2(160.0, 90.0)));
            }
            ui.vertical(|ui| {
                ui.label(RichText::new("Screenshot attached").strong());
                ui.label(RichText::new("Shared when you send").small().color(MUTED));
                if ui.button("Remove").clicked() {
                    self.draft_for(owner).screenshot = None;
                    self.textures.remove(&path);
                }
            });
        });
    }
    fn install_update(&mut self, release: updates::Release) {
        self.cancel_speech();
        self.update_pending = true;
        self.status = format!("Getting the {} update…", updates::system_name());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = updates::install(release).and_then(|path| {
                Ok(std::process::Command::new(path)
                    .env_remove("LANTERN_ENGINE")
                    .args(["--restart-after", &std::process::id().to_string()])
                    .spawn()?)
            });
            match result {
                Ok(_) => {
                    let _ = tx.send(Event::Quit);
                }
                Err(error) => {
                    let _ = tx.send(Event::Update(Err(format!("Update failed: {error:#}"))));
                }
            }
        });
    }
    fn account_flow(&self, generation: u64) -> account::Flow {
        account::Flow {
            generation,
            current: self.auth_generation.clone(),
            engine: self.engine.sender(),
            vault: self.vault.clone(),
            remember: self.remember,
            events: self.tx.clone(),
        }
    }
    fn reconnect(&mut self, engine: Sidecar) {
        let generation = self.auth_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.engine = engine;
        self.engine.observe(self.tx.clone(), self.bridge_snapshot.clone(), self.settings.notifications, self.settings.notify_finished);
        self.connected = false;
        self.history_pending = None;
        self.status = "Reconnecting…".into();
        self.auth_pending = false;
        if let Some(session) = &self.session {
            let token = session.refresh.clone();
            self.account_flow(generation).start(move || account::refresh(token));
        }
    }
    fn auth(&mut self, work: impl FnOnce(u64, Arc<AtomicU64>) -> anyhow::Result<account::Session> + Send + 'static) {
        let generation = self.auth_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.auth_generation.clone();
        self.auth_pending = true;
        self.status = "Signing in to October…".into();
        self.account_flow(generation).start(move || work(generation, current));
    }
    fn sign_out(&mut self) {
        let generation = self.auth_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.auth_pending = false;
        if let Some(session) = self.session.take() {
            std::thread::spawn(move || account::logout(session.access));
        }
        let _ = self.engine.sender().credentials(generation, &self.auth_generation, None, || {
            let _ = self.vault.send(account::VaultCommand::Clear);
        });
        self.phone = Value::Null;
        self.ask_generation.fetch_add(1, Ordering::SeqCst);
        self.ask_pending = false;
        self.ask_answer.clear();
        self.ask_history.clear();
        self.status = "Signed out".into();
    }
}

fn draw_qr(ui: &mut egui::Ui, text: &str) {
    if let Ok(code) = qrcode::QrCode::new(text) {
        let quiet = 4;
        let modules = code.width() + quiet * 2;
        let scale = (220.0 / modules as f32).floor().max(1.0);
        let side = scale * modules as f32;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        for y in 0..code.width() {
            for x in 0..code.width() {
                if code[(x, y)] == qrcode::Color::Dark {
                    let pos = rect.min + egui::vec2((x + quiet) as f32 * scale, (y + quiet) as f32 * scale);
                    ui.painter().rect_filled(egui::Rect::from_min_size(pos, egui::vec2(scale, scale)), 0.0, Color32::BLACK);
                }
            }
        }
    } else {
        ui.label("Could not render pairing QR code; cancel and try again.");
    }
}

impl Drop for Lantern {
    fn drop(&mut self) {
        self.auth_generation.fetch_add(1, Ordering::SeqCst);
        self.ask_generation.fetch_add(1, Ordering::SeqCst);
    }
}

impl eframe::App for Lantern {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.engine.events.try_recv() {
            match event {
                Ok(event) => self.event(event),
                Err(error) => {
                    self.connected = false;
                    self.status = format!("{error:#}");
                }
            }
        }
        for event in self.engine.take_latest() {
            self.event(event);
        }
        self.engine.notification_preferences(self.settings.notifications, self.settings.notify_finished);
        while let Ok(event) = self.events.try_recv() {
            self.integration(event, ui.ctx());
        }
        if ui.input(|input| input.viewport().close_requested())
            && !self.quitting
            && self.settings.close_to_tray
            && self.tray
            && self.snapshot.is_none()
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ui.ctx().send_viewport_cmd(if self.can_hide {
                egui::ViewportCommand::Visible(false)
            } else {
                egui::ViewportCommand::Minimized(true)
            });
        }
        if let Some(path) = &self.snapshot {
            if !self.snapshot_requested && self.started.elapsed() > Duration::from_secs(2) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.snapshot_requested = true;
            }
            for event in ui.input(|input| input.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let icon = egui::IconData {
                        rgba: image.pixels.iter().flat_map(|pixel| pixel.to_array()).collect(),
                        width: image.width() as u32,
                        height: image.height() as u32,
                    };
                    if let Err(error) =
                        icon.to_png_bytes().map_err(anyhow::Error::msg).and_then(|bytes| std::fs::write(path, bytes).map_err(Into::into))
                    {
                        eprintln!("Saving snapshot: {error:#}");
                    }
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if self.snapshot.is_none()
            && self.settings.check_updates
            && !self.update_pending
            && self.update_checked.elapsed() > Duration::from_secs(86400)
        {
            self.update_checked = Instant::now();
            self.update_pending = true;
            os::task(self.tx.clone(), updates::check, Event::Update);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(if self.compact { 400 } else { 150 }));
        if self.connected && self.selected.is_some() && self.history_at.elapsed() > Duration::from_secs(5) {
            self.history();
        }
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.show_new = true;
            self.cancel_speech();
            self.set_compact(false, ui.ctx());
        }
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Comma)) {
            self.page = Page::Settings;
            self.cancel_speech();
            self.set_compact(false, ui.ctx());
        }
        self.apply_window_mode(ui.ctx());
        if self.compact {
            egui::CentralPanel::default().show(ui, |ui| {
                if ui.interact(ui.max_rect(), egui::Id::new("compact-drag"), egui::Sense::drag()).drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).max_size(egui::vec2(40.0, 40.0)));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("lantern").strong().size(19.0));
                        ui.label(format!(
                            "{} agents · {} need you",
                            self.agents.len(),
                            self.agents.iter().filter(|agent| agent["state"] == "needs_input").count()
                        ));
                    });
                    if ui.button("Open").clicked() {
                        self.set_compact(false, ui.ctx());
                    }
                });
                ui.label(RichText::new(short(&self.status, 52)).small().color(MUTED));
            });
            return;
        }
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::symmetric(18, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    dot(ui, if self.connected { Color32::from_rgb(115, 204, 160) } else { ACCENT });
                    ui.label(RichText::new(short(&self.status, 125)).small().color(MUTED)).on_hover_text(&self.status);
                });
            });
        let width = if ui.available_width() < 850.0 { 220.0 } else { 260.0 };
        egui::Panel::left("workspace")
            .resizable(true)
            .default_size(width)
            .min_size(190.0)
            .max_size(330.0)
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(16.0))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(24.0)).show(ui, |ui| {
            match self.page {
                Page::Sessions => self.conversation(ui),
                Page::Ask => self.point_ask(ui),
                Page::Connections => self.connections(ui),
                Page::Settings => self.settings_ui(ui),
                Page::Help => self.help(ui),
            }
        });
        self.new_session(ui.ctx());
        self.welcome(ui.ctx());
    }
}
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--restart-after") {
        let pid: u32 = args.get(index + 1).ok_or_else(|| anyhow::anyhow!("missing restart process"))?.parse()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while os::process_alive(pid) {
            anyhow::ensure!(Instant::now() < deadline, "Previous Lantern process did not exit");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if args.iter().any(|arg| arg == "--check-desktop") {
        println!("{}", os::diagnostics()?);
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--install-launcher") {
        updates::install_launcher(std::env::current_exe()?.parent().ok_or_else(|| anyhow::anyhow!("no bundle folder"))?)?;
        println!("Application launcher installed");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--check-sidecar") {
        let engine = Sidecar::start()?;
        let event = engine.events.recv_timeout(Duration::from_secs(10))??;
        anyhow::ensure!(event["type"] == "hello" && event["protocol"] == 3, "unsupported engine protocol");
        println!("Sidecar connected: protocol {}, engine {}", event["protocol"], event["version"]);
        return Ok(());
    }
    if let Some(index) = args.iter().position(|arg| arg == "--fixture") {
        let fixture = args.get(index + 1).ok_or_else(|| anyhow::anyhow!("--fixture requires a JSON path"))?;
        let out = match args.iter().position(|arg| arg == "--out") {
            Some(index) => PathBuf::from(args.get(index + 1).ok_or_else(|| anyhow::anyhow!("--out requires a PNG path"))?),
            None => PathBuf::from(fixture).with_extension("png"),
        };
        let scale = match args.iter().position(|arg| arg == "--scale") {
            Some(index) => args.get(index + 1).ok_or_else(|| anyhow::anyhow!("--scale requires 1 or 2"))?.parse()?,
            None => 1.0,
        };
        return app::fixture::run(std::path::Path::new(fixture), out, scale);
    }
    if args.iter().any(|arg| arg == "--shell") {
        let settings = Settings::load().unwrap_or_default();
        return app::run(&settings.terminal);
    }
    if args.iter().any(|arg| arg == "--help") {
        println!(
            "October Lantern\n--check-sidecar   Verify the Rust engine\n--check-desktop   Verify desktop portals\n--install-launcher   Install the application launcher\n--snapshot FILE   Render a preview\n--fixture FILE.json [--out FILE.png] [--scale 1|2]   Render one surface from a saved model\n--shell   The Mac-parity pill and panel (in progress)\n--settings | --connections | --ask | --help-page | --welcome | --new-session | --compact"
        );
        return Ok(());
    }
    let snapshot = match args.iter().position(|arg| arg == "--snapshot") {
        Some(index) => Some(PathBuf::from(args.get(index + 1).ok_or_else(|| anyhow::anyhow!("--snapshot requires a PNG path"))?)),
        None => None,
    };
    let action = if args.iter().any(|arg| arg == "--settings") {
        "settings"
    } else if args.iter().any(|arg| arg == "--connections") {
        "connections"
    } else if args.iter().any(|arg| arg == "--ask") {
        "ask"
    } else if args.iter().any(|arg| arg == "--new-session") {
        "new"
    } else if args.iter().any(|arg| arg == "--compact") {
        "compact"
    } else if args.iter().any(|arg| arg == "--help-page") {
        "help"
    } else if args.iter().any(|arg| arg == "--welcome") {
        "welcome"
    } else {
        "show"
    };
    if snapshot.is_none() && os::show_existing(action) {
        return Ok(());
    }
    let page = if args.iter().any(|arg| arg == "--help-page") {
        Page::Help
    } else if args.iter().any(|arg| arg == "--settings") {
        Page::Settings
    } else if args.iter().any(|arg| arg == "--connections") {
        Page::Connections
    } else if args.iter().any(|arg| arg == "--ask") {
        Page::Ask
    } else {
        Page::Sessions
    };
    let (settings, error) = match Settings::load() {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(format!("Could not load preferences: {error:#}"))),
    };
    let engine = Sidecar::with_terminal(&settings.terminal)?;
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../logo.png"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("dev.october.lantern")
            .with_inner_size([1060.0, 760.0])
            .with_min_inner_size([680.0, 500.0])
            .with_window_level(if settings.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal })
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "October Lantern",
        options,
        Box::new(move |cc| {
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = cc.window_handle()
                    && let RawWindowHandle::Win32(win32) = handle.as_raw()
                {
                    os::set_main_window(win32.hwnd.get());
                }
            }
            let window = cc.winit_window().cloned();
            let can_hide = window.as_ref().is_none_or(|window| window.is_visible().is_some());
            let ctx = cc.egui_ctx.clone();
            let mut app = Lantern::new(engine, settings, error, snapshot, page, &cc.egui_ctx, move |show| {
                if show && let Some(window) = &window {
                    if can_hide {
                        window.set_visible(true);
                        window.set_minimized(false);
                        window.focus_window();
                    } else {
                        os::raise_window();
                    }
                }
                ctx.request_repaint();
            });
            app.can_hide = can_hide;
            app.show_new = args.iter().any(|arg| arg == "--new-session");
            app.show_welcome |= args.iter().any(|arg| arg == "--welcome");
            if args.iter().any(|arg| arg == "--compact") {
                app.set_compact(true, &cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
