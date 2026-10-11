//! The composer's drafts (Swift `LanternCore/Drafts.swift`): one per conversation, keyed by recipient
//! agent id ("" while no recipient is chosen). Every edit gets a new revision, so a send
//! acknowledgement clears exactly the text that was sent to that agent, never another agent's draft
//! or a later retype of the same words.
//!
//! The UI changes text only through [`Drafts::edit_text`]. Recipient changes (`address`, `readdress`)
//! and acknowledgements are domain transitions made by `app::Lantern::apply`.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub text: String,
    pub revision: u64,
}

/// What a send carries, so its acknowledgement can clear only what was sent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ticket {
    pub recipient: String,
    pub revision: u64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Drafts {
    /// Once chosen (or once you start typing) it stays chosen: a draft never quietly changes
    /// recipient because the agent list changed.
    recipient: Option<String>,
    drafts: HashMap<String, Draft>,
    next_revision: u64,
}

impl Drafts {
    fn key(&self) -> &str {
        self.recipient.as_deref().unwrap_or("")
    }
    pub fn recipient(&self) -> Option<&str> {
        self.recipient.as_deref()
    }
    /// The draft shown for the current recipient.
    pub fn text(&self) -> &str {
        self.text_for(self.key())
    }
    pub fn text_for(&self, recipient: &str) -> &str {
        self.drafts.get(recipient).map_or("", |draft| draft.text.as_str())
    }
    pub fn revision_for(&self, recipient: &str) -> Option<u64> {
        self.drafts.get(recipient).map(|draft| draft.revision)
    }
    pub fn is_empty(&self) -> bool {
        self.text().is_empty()
    }

    /// The UI's only way to change text: `edit` gets the current recipient's text to bind to a
    /// text field. When it changed, it's typed in (new revision; the first keystroke with no chosen
    /// recipient locks in `fallback`).
    pub fn edit_text<R>(&mut self, fallback: Option<&str>, edit: impl FnOnce(&mut String) -> R) -> R {
        let mut text = self.text().to_string();
        let result = edit(&mut text);
        if text != self.text() {
            self.type_text(text, fallback);
        }
        result
    }

    /// Typing (Swift `type`).
    pub fn type_text(&mut self, text: String, fallback: Option<&str>) {
        if !text.is_empty()
            && self.recipient.is_none()
            && let Some(fallback) = fallback
        {
            // Text typed before any agent existed moves to the recipient it's now locked to.
            self.drafts.remove("");
            self.recipient = Some(fallback.to_string());
        }
        let key = self.key().to_string();
        self.set(text, &key);
    }

    /// Sets one conversation's draft, whichever is showing (dictation writes to the draft it
    /// started in). A new revision whenever the text changes.
    pub fn set(&mut self, text: String, recipient: &str) {
        if self.drafts.get(recipient).is_some_and(|draft| draft.text == text) || text.is_empty() && !self.drafts.contains_key(recipient) {
            return;
        }
        if text.is_empty() {
            self.drafts.remove(recipient);
        } else {
            self.next_revision += 1;
            self.drafts.insert(recipient.to_string(), Draft { text, revision: self.next_revision });
        }
    }

    /// Adds dictated text to the draft it was started in.
    pub fn append(&mut self, text: &str, recipient: &str) {
        let current = self.text_for(recipient);
        let joined = if current.is_empty() { text.to_string() } else { format!("{current} {text}") };
        self.set(joined, recipient);
    }

    /// Switches conversation (opening a chat, a card, a notification). Drafts stay with their agents.
    pub fn address(&mut self, recipient: Option<&str>) {
        self.recipient = recipient.map(str::to_string);
    }

    /// Changes the recipient of the draft being written: an explicit choice. The text moves only
    /// when the new recipient has no draft of its own.
    pub fn readdress(&mut self, recipient: &str) {
        if self.recipient.as_deref() == Some(recipient) {
            return;
        }
        let key = self.key().to_string();
        if self.drafts.contains_key(&key)
            && !self.drafts.contains_key(recipient)
            && let Some(moving) = self.drafts.remove(&key)
        {
            self.drafts.insert(recipient.to_string(), moving);
        }
        self.recipient = Some(recipient.to_string());
    }

    /// The current draft, ready to send; `None` with nothing to send or no recipient.
    pub fn ticket(&self) -> Option<Ticket> {
        let recipient = self.recipient.as_ref()?;
        let draft = self.drafts.get(recipient)?;
        let text = draft.text.trim();
        (!text.is_empty()).then(|| Ticket { recipient: recipient.clone(), revision: draft.revision, text: text.to_string() })
    }

    /// A send finished: clears the draft only if it is still the revision that was sent.
    pub fn sent(&mut self, ticket: &Ticket) {
        if self.revision_for(&ticket.recipient) == Some(ticket.revision) {
            self.drafts.remove(&ticket.recipient);
        }
    }

    /// Forgets drafts for agents that are gone, except the one still addressed.
    pub fn prune(&mut self, live: &HashSet<String>) {
        let recipient = self.recipient.clone();
        self.drafts.retain(|key, _| key.is_empty() || Some(key) == recipient.as_ref() || live.contains(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(d: &mut Drafts, text: &str) {
        d.type_text(text.into(), None);
    }

    #[test]
    fn opening_another_chat_does_not_carry_the_draft() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "private A context");
        d.address(Some("B"));
        assert_eq!(d.text(), "");
        assert_eq!(d.recipient(), Some("B"));
        d.address(Some("A"));
        assert_eq!(d.text(), "private A context");
    }

    #[test]
    fn ack_for_one_agent_keeps_same_text_draft_for_another() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "yes");
        let sent_to_a = d.ticket().unwrap();
        d.address(Some("B"));
        typed(&mut d, "yes");
        d.sent(&sent_to_a);
        assert_eq!(d.text_for("A"), "");
        assert_eq!(d.text_for("B"), "yes");
    }

    #[test]
    fn ack_keeps_a_retyped_draft_with_the_same_text() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "run the tests");
        let old = d.ticket().unwrap();
        typed(&mut d, "something else");
        typed(&mut d, "run the tests");
        d.sent(&old);
        assert_eq!(d.text(), "run the tests");
    }

    #[test]
    fn ack_clears_the_unchanged_draft() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "  go  ");
        let t = d.ticket().unwrap();
        assert_eq!(t.text, "go");
        d.sent(&t);
        assert_eq!(d.text(), "");
    }

    #[test]
    fn first_keystroke_locks_the_fallback_recipient() {
        let mut d = Drafts::default();
        d.type_text("h".into(), Some("A"));
        assert_eq!(d.recipient(), Some("A"));
        d.type_text("hi".into(), Some("B"));
        assert_eq!(d.recipient(), Some("A"));
        assert_eq!(d.text_for("A"), "hi");
    }

    #[test]
    fn no_ticket_without_recipient_or_text() {
        let mut d = Drafts::default();
        typed(&mut d, "hello");
        assert!(d.ticket().is_none());
        d.address(Some("A"));
        typed(&mut d, "   ");
        assert!(d.ticket().is_none());
    }

    #[test]
    fn readdress_moves_text_only_to_an_empty_draft() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "for someone");
        d.readdress("B");
        assert_eq!(d.text_for("B"), "for someone");
        assert_eq!(d.text_for("A"), "");
        d.address(Some("C"));
        typed(&mut d, "C's own");
        d.address(Some("B"));
        d.readdress("C");
        assert_eq!(d.text(), "C's own");
        assert_eq!(d.text_for("B"), "for someone");
    }

    #[test]
    fn prune_keeps_the_addressed_draft() {
        let mut d = Drafts::default();
        d.address(Some("gone"));
        typed(&mut d, "keep me");
        d.address(Some("other"));
        typed(&mut d, "drop me");
        d.address(Some("gone"));
        d.prune(&HashSet::new());
        assert_eq!(d.text_for("gone"), "keep me");
        assert_eq!(d.text_for("other"), "");
    }

    // Beyond the Swift tests: the single edit path and dictation.

    #[test]
    fn edit_text_bumps_the_revision_only_when_the_text_changes() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        d.edit_text(None, |text| text.push_str("hi"));
        let first = d.revision_for("A").unwrap();
        d.edit_text(None, |_| {});
        assert_eq!(d.revision_for("A"), Some(first));
        d.edit_text(None, |text| text.push('!'));
        assert!(d.revision_for("A").unwrap() > first);
        assert_eq!(d.text(), "hi!");
    }

    #[test]
    fn a_late_transcript_is_a_new_revision_so_an_older_ack_keeps_it() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "check this");
        let sent = d.ticket().unwrap();
        d.address(Some("B"));
        d.append("and the logs", "A");
        d.sent(&sent);
        assert_eq!(d.text_for("A"), "check this and the logs");
    }

    #[test]
    fn clearing_a_draft_removes_it() {
        let mut d = Drafts::default();
        d.address(Some("A"));
        typed(&mut d, "x");
        d.edit_text(None, String::clear);
        assert_eq!(d.revision_for("A"), None);
        assert!(d.is_empty());
    }
}
