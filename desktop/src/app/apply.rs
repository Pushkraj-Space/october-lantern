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
                self.save_prefs();
            }
            UiAction::AnswerPermission { agent_id, prompt_id, allow } => self.answer_permission(&agent_id, &prompt_id, allow),
            UiAction::StartNewSession => self.start_new_session(),
            UiAction::ChooseSessionAgent { kind } => self.choose_session_agent(kind),
            UiAction::ChooseSessionModel { kind, model } => {
                if self.inputs.new_session.kind.as_ref() == Some(&kind) {
                    self.inputs.new_session.model = model;
                }
            }
            UiAction::ChooseFolder => {
                // Losing focus to the picker doesn't close the panel (CONTRACT §6).
                self.windows.native_dialog = true;
                crate::os::task(self.tx.clone(), crate::os::folder, crate::os::Event::Folder);
            }
            UiAction::SetSessionScreenshot(on) => self.set_session_screenshot(on),
            // Asking again shows the desktop's screen-capture permission (Linux portal).
            UiAction::RequestCapturePermission | UiAction::RetakeSessionScreenshot => {
                if self.inputs.new_session.screenshot {
                    self.capture_screen();
                }
            }
            UiAction::SetOpenInTerminal(terminal) => {
                self.inputs.new_session.open_in_terminal = terminal;
                self.model.prefs.open_in_terminal = terminal;
                self.save_prefs();
            }
            UiAction::GetAgent { kind } => {
                if let Some(url) = kind.website() {
                    self.open_url(url);
                }
            }
            UiAction::Launch(request) => {
                self.launch(*request);
            }
            UiAction::SignInProvider(provider) => {
                let provider = match provider {
                    crate::ui::Provider::Google => "google",
                    crate::ui::Provider::GitHub => "github",
                    crate::ui::Provider::Apple => "apple",
                };
                self.sign_in(move |generation, current| crate::account::oauth(provider, generation, current));
            }
            UiAction::SignInEmail { email, password } => {
                self.sign_in(move |_, _| crate::account::password(email.trim().to_string(), password));
            }
            UiAction::CancelSignIn => self.cancel_sign_in(),
            UiAction::SignOut => self.sign_out(),
            UiAction::ToggleDictation { owner } => self.toggle_dictation(owner),
            UiAction::ShowSettings => {
                self.refresh_notifications();
                self.refresh_hooks();
                self.windows.dialogs.settings = true;
            }
            UiAction::ChangeSetting(change) => self.change_setting(change),
            UiAction::SetHooks { enabled } => self.set_hooks(enabled),
            UiAction::RefreshToolkit => self.refresh_toolkit(false),
            UiAction::OpenToolkit => self.refresh_toolkit(true),
            UiAction::OpenNotificationSettings => self.open_notification_settings(),
            UiAction::CheckUpdates => self.check_updates(),
            // The confirmation step is the Settings view's own state.
            UiAction::ConfirmUninstall => self.uninstall(),
            UiAction::ShowWelcome => self.show_welcome(),
            UiAction::FinishWelcome => self.finish_welcome(),
            UiAction::StartPointAsk => self.start_point_ask(),
            UiAction::ReselectPointAsk => self.reselect_point_ask(),
            UiAction::CancelPointAsk => self.cancel_point_ask(),
            UiAction::ChoosePointAskRecipient(agent_id) => self.choose_point_recipient(agent_id),
            UiAction::SubmitPointAsk => self.submit_point_ask(),
            UiAction::SendPointAskToAgent => self.send_point(),
            UiAction::CopyPointAskAnswer => self.copy_point_ask_answer(),
            UiAction::SubmitAssistant => self.submit_assistant(),
            UiAction::ClearAssistant => self.clear_assistant(),
            UiAction::CancelAssistantTask => self.cancel_assistant_task(),
            UiAction::StartAssistantTaskNow => self.start_pending_task(),
            UiAction::CloseLaunchPreview => self.close_preview(),
            UiAction::RetryPreviewInTerminal => self.retry_preview_in_terminal(),
            UiAction::OpenUrl(url) => self.open_url(&url),
            // October Desktop (engine/src/october_link.rs).
            UiAction::DesktopConnect => {
                self.request(json!({"type": "october.pair"}));
            }
            UiAction::DesktopCancelPairing => {
                self.request(json!({"type": "october.cancelPair"}));
            }
            UiAction::DesktopDisconnect => {
                self.request(json!({"type": "october.forget"}));
            }
            // The Mac opens /Applications/October.app; elsewhere October registers its october:
            // URL scheme, so the system starts whichever October is installed.
            UiAction::OpenOctober => {
                if crate::os::open("october://").is_err() {
                    self.show("Couldn't open October Desktop. Is it installed?");
                }
            }
            UiAction::GetOctober => self.open_url("https://www.october.dev"),
            UiAction::ReportProblem => {
                let mut url = url::Url::parse("https://github.com/Pushkraj-Space/october-lantern/issues/new").expect("issue URL");
                let system = crate::updates::system_name();
                url.query_pairs_mut().append_pair("title", &format!("{system}: ")).append_pair(
                    "body",
                    &format!(
                        "Lantern: {}\nOS: {system} {}\nDesktop: {}\n\nWhat happened:\n\nSteps to reproduce:\n",
                        env!("CARGO_PKG_VERSION"),
                        std::env::consts::ARCH,
                        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
                    ),
                );
                self.open_url(url.as_str());
            }
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
            self.save_prefs();
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
        self.stop_dictation_if_away(Some(agent_id));
        self.inputs.drafts.address(Some(agent_id));
        self.refresh_chat();
        self.model.compose_focus += 1;
    }

    /// Leaving a conversation also ends a starting session's preview (Swift `closeChat`). Launches
    /// set the panel before the preview, so this never ends the preview they just showed (§9.1).
    pub(super) fn close_chat(&mut self) {
        if self.model.launch_preview.is_some() {
            self.end_preview();
        }
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

impl Lantern {
    fn open_url(&mut self, url: &str) {
        if let Err(error) = crate::os::open(url) {
            self.show(format!("Couldn't open {url}: {error:#}"));
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
