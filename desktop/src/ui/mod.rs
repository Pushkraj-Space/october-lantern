//! The Mac app's surfaces, drawn with egui (CONTRACT §4–5). Views read `&Model`, edit `Inputs`
//! text in place, keep view-only state in `UiState`, and report what the person did as
//! `UiAction`s; `app::Lantern::apply` turns those into effects.
// The new UI path is built while the legacy UI is still the default (CONTRACT §10).
#![allow(dead_code)]

pub mod assets;
pub mod components;
pub mod icons;
pub mod theme;

use crate::model::{AgentId, AgentKind, AppContext, Edge, Inputs, Model, PanelMode};
use eframe::egui;
use std::path::PathBuf;

pub use assets::Assets;

/// What a view needs from outside the model.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// Milliseconds since the epoch; fixed in fixtures.
    pub now_ms: f64,
    pub pixels_per_point: f32,
    pub reduce_motion: bool,
    /// The home folder, for `~` paths and "Home folder".
    pub home: String,
}

/// View-local state (Atlas): expanded cards, More toggles, request detail, menus, welcome step,
/// settings tab, model search, email/password, focus tokens, scroll anchors.
#[derive(Default)]
pub struct UiState {}

pub struct Ctx<'a> {
    pub model: &'a Model,
    pub inputs: &'a mut Inputs,
    pub view: &'a mut UiState,
    pub out: &'a mut Vec<UiAction>,
    pub assets: &'a Assets,
    pub env: &'a Env,
}

impl Ctx<'_> {
    pub fn emit(&mut self, action: UiAction) {
        self.out.push(action);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    GitHub,
    Apple,
}

/// Whose draft dictation writes into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DictationOwner {
    Reply(AgentId),
    Assistant,
    PointAsk,
}

/// A New Session start. Defaults, persistence and saving the screenshot first belong to `apply`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionRequest {
    pub kind: AgentKind,
    pub folder: String,
    pub prompt: String,
    pub model: Option<String>,
    pub screenshot: Option<PathBuf>,
    pub context: Option<AppContext>,
    pub toolkit: bool,
    pub bus: bool,
    /// In Lantern (true) or a Terminal window (false).
    pub background: bool,
    /// The app a Task is for (its id).
    pub task_app: Option<String>,
}

/// Visible preferences (Settings).
#[derive(Clone, Debug, PartialEq)]
pub enum SettingChange {
    OpenAtLogin(bool),
    Edge(Edge),
    MessageHotkey(String),
    PointAskHotkey(String),
    AutomaticUpdates(bool),
    ScreenshotNewSessions(bool),
    BusForSessions(bool),
    ToolkitInSessions(bool),
    NotificationsEnabled(bool),
    NotifyOnFinish(bool),
    AgentMuted { kind: AgentKind, muted: bool },
    ShareUsage(bool),
}

/// Everything a person can do (CONTRACT §4; Atlas owns this enum, payloads frozen with the skeleton).
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    // Panels and chats
    SetPanel(Option<PanelMode>),
    TogglePanel(PanelMode),
    OpenChat {
        agent_id: AgentId,
    },
    CloseChat,
    /// `None`: Ask Lantern, focused. `Some`: that agent's chat, composer focused.
    Compose {
        agent_id: Option<AgentId>,
    },
    Readdress {
        agent_id: AgentId,
    },
    SendReply {
        agent_id: AgentId,
    },
    OpenAgent {
        agent_id: AgentId,
    },
    /// The turns the person saw (`agentId@stateSince`).
    DismissTurns {
        turn_keys: Vec<String>,
    },
    AnswerPermission {
        agent_id: AgentId,
        prompt_id: String,
        allow: bool,
    },
    ToggleDictation {
        owner: DictationOwner,
    },
    // New Session
    /// Captures the foreground app before the panel takes focus.
    StartNewSession,
    ChooseSessionAgent {
        kind: AgentKind,
    },
    ChooseSessionModel {
        kind: AgentKind,
        model: Option<String>,
    },
    /// The native folder picker.
    ChooseFolder,
    SetSessionScreenshot(bool),
    RetakeSessionScreenshot,
    SetOpenInTerminal(bool),
    /// Opens where to get an agent that isn't installed.
    GetAgent {
        kind: AgentKind,
    },
    RequestCapturePermission,
    RequestAppContextPermission,
    /// Boxed: much larger than every other action.
    Launch(Box<SessionRequest>),
    CloseLaunchPreview,
    RetryPreviewInTerminal,
    // Point & Ask
    StartPointAsk,
    ReselectPointAsk,
    CancelPointAsk,
    ChoosePointAskRecipient(AgentId),
    SubmitPointAsk,
    SendPointAskToAgent,
    CopyPointAskAnswer,
    // Ask Lantern
    SubmitAssistant,
    ClearAssistant,
    CancelAssistantTask,
    StartAssistantTaskNow,
    // Account and October Desktop
    SignInProvider(Provider),
    SignInEmail {
        email: String,
        password: String,
    },
    CancelSignIn,
    SignOut,
    DesktopConnect,
    DesktopCancelPairing,
    DesktopDisconnect,
    OpenOctober,
    GetOctober,
    // Settings and app
    ShowSettings,
    ChangeSetting(SettingChange),
    SetHooks {
        enabled: bool,
    },
    RefreshToolkit,
    OpenToolkit,
    OpenNotificationSettings,
    CheckUpdates,
    ReportProblem,
    ShowWelcome,
    FinishWelcome,
    RequestUninstall,
    ConfirmUninstall,
    Copy(String),
    OpenUrl(String),
    Quit,
}

/// The main panel, 380 wide. Returns the measured outer height. Placeholder until Atlas's surfaces land.
pub fn render_panel(ui: &mut egui::Ui, cx: &mut Ctx, max_height: f32) -> f32 {
    placeholder(ui, cx, max_height, "Panel")
}

/// The Point & Ask card, 400 wide.
pub fn render_point_ask(ui: &mut egui::Ui, cx: &mut Ctx, max_height: f32) -> f32 {
    placeholder(ui, cx, max_height, "Point & Ask")
}

/// Welcome, 540 x 560.
pub fn render_welcome(ui: &mut egui::Ui, cx: &mut Ctx) -> f32 {
    placeholder(ui, cx, 560.0, "Welcome")
}

/// Settings, 520 x 480.
pub fn render_settings(ui: &mut egui::Ui, cx: &mut Ctx) -> f32 {
    placeholder(ui, cx, 480.0, "Settings")
}

fn placeholder(ui: &mut egui::Ui, cx: &mut Ctx, max_height: f32, title: &str) -> f32 {
    let top = ui.cursor().top();
    ui.vertical(|ui| {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(title).strong());
            ui.label(format!("· {:?} · {} agents · {} new", cx.model.panel, cx.model.agents.len(), cx.model.badge()));
        });
        ui.add_space(14.0);
    });
    (ui.cursor().top() - top).min(max_height)
}
