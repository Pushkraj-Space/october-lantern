use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
#[derive(Clone, Default, PartialEq)]
pub struct Draft {
    pub text: String,
    pub screenshot: Option<PathBuf>,
}
pub struct PendingReply {
    pub request: String,
    pub agent: String,
    pub draft: Option<Draft>,
}
pub fn clear_delivered(drafts: &mut HashMap<String, Draft>, pending: &PendingReply) {
    if let Some(sent) = &pending.draft
        && drafts.get(&pending.agent) == Some(sent)
    {
        drafts.remove(&pending.agent);
    }
}
#[derive(Default)]
pub struct Notices {
    previous: HashMap<String, (String, String)>,
    seen: HashSet<String>,
}
impl Notices {
    pub fn transitions(&mut self, agents: &[Value], finished: bool) -> Vec<Value> {
        let mut next = HashMap::new();
        let mut changes = Vec::new();
        for agent in agents {
            let id = agent["id"].as_str().unwrap_or("");
            let state = agent["state"].as_str().unwrap_or("");
            let fingerprint = format!("{id}:{state}:{}:{}", agent["promptId"], agent["stateSince"]);
            next.insert(id.to_string(), (state.to_string(), fingerprint.clone()));
            if let Some((previous, old)) = self.previous.get(id)
                && old != &fingerprint
                && (state == "needs_input" || finished && (previous == "working" || previous == "waiting") && state == "waiting")
                && self.seen.insert(fingerprint)
            {
                changes.push(agent.clone());
            }
        }
        self.previous = next;
        if self.seen.len() > 4096 {
            self.seen.clear();
        }
        changes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn late_delivery_preserves_newer_text_and_other_sessions() {
        let sent = Draft { text: "original".into(), ..Default::default() };
        let mut drafts = HashMap::from([("a".into(), sent.clone()), ("b".into(), Draft { text: "other".into(), ..Default::default() })]);
        let pending = PendingReply { request: "1".into(), agent: "a".into(), draft: Some(sent) };
        drafts.get_mut("a").unwrap().text = "edited".into();
        clear_delivered(&mut drafts, &pending);
        assert_eq!(drafts["a"].text, "edited");
        assert_eq!(drafts["b"].text, "other");
        drafts.get_mut("a").unwrap().text = "original".into();
        clear_delivered(&mut drafts, &pending);
        assert!(!drafts.contains_key("a"));
        assert!(drafts.contains_key("b"));
    }
    #[test]
    fn notifications_skip_startup_and_repeat_snapshots() {
        let mut notices = Notices::default();
        let a = |state| json!({"id":"a", "state":state,"promptId":"p"});
        assert!(notices.transitions(&[a("needs_input")], true).is_empty());
        assert!(notices.transitions(&[a("working")], true).is_empty());
        assert_eq!(notices.transitions(&[a("waiting")], true).len(), 1);
        assert!(notices.transitions(&[a("waiting")], true).is_empty());
        assert_eq!(notices.transitions(&[a("needs_input")], true).len(), 1);
        assert_eq!(notices.transitions(&[json!({"id":"a","state":"needs_input","promptId":"new-prompt"})], true).len(), 1);
        assert!(notices.transitions(&[json!({"id":"a","state":"needs_input","promptId":"new-prompt"})], true).is_empty());
    }
}
