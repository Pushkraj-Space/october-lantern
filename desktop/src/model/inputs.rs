//! Text the person is typing that the domain also writes (CONTRACT §4): reply drafts (dictation
//! appends, acknowledgements clear a revision, readdress moves them), the Ask Lantern box
//! (suggestions fill it, submit clears it), the Point & Ask box, and the New Session form (assistant
//! prefill, task defaults and the last model per agent fill it). Text only the UI touches (email,
//! password, model search) lives in `ui::UiState` instead.
use super::{AgentKind, Drafts};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewSessionForm {
    pub prompt: String,
    /// `None`: the most recently used installed agent.
    pub kind: Option<AgentKind>,
    /// `None`: the agent's own default model.
    pub model: Option<String>,
    pub folder: Option<String>,
    /// "Work on <app>": a task for the app you were in.
    pub task: bool,
    pub screenshot: bool,
    /// Terminal window (true) or In Lantern (false).
    pub open_in_terminal: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Inputs {
    pub drafts: Drafts,
    pub assistant: String,
    pub point_ask: String,
    pub new_session: NewSessionForm,
}
