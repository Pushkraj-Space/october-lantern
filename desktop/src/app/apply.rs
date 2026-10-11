//! The one place UI intent becomes effects (CONTRACT §4). Actions carry what the person saw
//! (prompt ids, turn keys); stale ones are dropped here.
use super::{Lantern, Pending};
use crate::model::{Chat, PanelMode};
use crate::ui::UiAction;
use serde_json::json;

impl Lantern {
    pub(super) fn apply(&mut self, action: UiAction) {
        match action {
            UiAction::SetPanel(panel) => self.set_panel(panel),
            UiAction::TogglePanel(mode) => self.set_panel(if self.model.panel == Some(mode) { None } else { Some(mode) }),
            UiAction::OpenChat { agent_id } => self.open_chat(&agent_id),
            UiAction::CloseChat => self.close_chat(),
            UiAction::Compose { agent_id: Some(agent_id) } => self.open_chat(&agent_id),
            UiAction::Compose { agent_id: None } => {
                if self.model.panel != Some(PanelMode::Assistant) {
                    self.set_panel(Some(PanelMode::Assistant));
                }
                self.model.compose_focus += 1;
            }
            UiAction::Readdress { agent_id } => self.inputs.drafts.readdress(&agent_id),
            UiAction::SendReply { agent_id } => self.send_reply(&agent_id),
            UiAction::OpenAgent { agent_id } => {
                crate::os::allow_focus(self.engine.pid());
                if let Some(id) = self.request(json!({"type": "focus", "agentId": agent_id})) {
                    self.pending.insert(id, Pending::Focus { agent_id });
                }
            }
            UiAction::DismissTurns { turn_keys } => {
                self.turns.dismiss(&turn_keys, &self.model.agents);
                self.model.inbox = self.turns.inbox(&self.model.agents, &self.model.prefs);
            }
            UiAction::AnswerPermission { agent_id, prompt_id, allow } => self.answer_permission(&agent_id, &prompt_id, allow),
            UiAction::Copy(text) => self.windows.copy(text),
            UiAction::Quit => self.windows.quit(),
            // TODO(apollo): the remaining actions land with their surfaces (new session, assistant,
            // point & ask, account, settings).
            other => log::debug(&other),
        }
    }

    /// Changing panels (or closing it) also leaves any open conversation; leaving Waiting means
    /// you've seen what was in it.
    pub(super) fn set_panel(&mut self, panel: Option<PanelMode>) {
        let old = self.model.panel;
        if old == panel {
            return;
        }
        super::trace!("panel {old:?} -> {panel:?}");
        self.model.panel = panel;
        self.close_chat();
        if old == Some(PanelMode::Inbox) && self.turns.mark_seen(&self.model.agents, &self.model.prefs) {
            self.model.inbox = self.turns.inbox(&self.model.agents, &self.model.prefs);
        }
    }

    /// Opens an agent's conversation in the panel; the composer then talks to it.
    pub(super) fn open_chat(&mut self, agent_id: &str) {
        if self.model.agent(agent_id).is_none() {
            return;
        }
        if !self.model.panel.is_some_and(PanelMode::is_list) {
            self.set_panel(Some(PanelMode::Inbox));
        }
        if self.model.chat.as_ref().map(|chat| chat.agent_id.as_str()) != Some(agent_id) {
            self.model.chat = Some(Chat { agent_id: agent_id.into(), supported: true, loading: true, ..Default::default() });
        }
        self.inputs.drafts.address(Some(agent_id));
        self.refresh_chat();
        self.model.compose_focus += 1;
    }

    pub(super) fn close_chat(&mut self) {
        // TODO(apollo): end the launch preview here, with the ordering fix (CONTRACT §9.1).
        if let Some(old) = self.history_request.take() {
            self.pending.remove(&old);
        }
        self.model.chat = None;
    }

    fn send_reply(&mut self, agent_id: &str) {
        if self.inputs.drafts.recipient() != Some(agent_id) || self.model.sending {
            return;
        }
        let Some(ticket) = self.inputs.drafts.ticket() else { return };
        if !self.model.agent(agent_id).is_some_and(|agent| agent.can_reply) {
            // TODO(apollo): copy and open the terminal, with the platform's paste instruction.
            return;
        }
        if let Some(id) = self.request(json!({"type": "reply", "agentId": agent_id, "text": ticket.text})) {
            self.pending.insert(id, Pending::Reply(ticket));
            self.model.sending = true;
        }
    }

    /// Allow/Deny answer exactly the prompt the person saw, or nothing.
    fn answer_permission(&mut self, agent_id: &str, prompt_id: &str, allow: bool) {
        let current =
            self.model.agent(agent_id).filter(|agent| agent.can_answer_permission() && agent.prompt_id.as_deref() == Some(prompt_id));
        if current.is_none() {
            return;
        }
        let key = if allow { "1" } else { "Escape" };
        if let Some(id) = self.request(json!({"type": "keys", "agentId": agent_id, "keys": [key], "promptId": prompt_id})) {
            self.pending.insert(id, Pending::Keys { agent_id: agent_id.into() });
        }
    }
}

mod log {
    /// Actions whose effects haven't landed yet; visible with LANTERN_DEBUG=1.
    pub fn debug(action: &crate::ui::UiAction) {
        if std::env::var_os("LANTERN_DEBUG").is_some() {
            eprintln!("lantern: not handled yet: {action:?}");
        }
    }
}
