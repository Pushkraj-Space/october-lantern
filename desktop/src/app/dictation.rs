//! Dictation (Swift `Dictation`): speak into the draft you started it in. The Mac uses Apple's
//! on-device speech; here the audio is recorded and transcribed on this computer by whisper.cpp
//! (set up with scripts/setup-dictation.*). Each dictation belongs to one draft: moving to another
//! conversation ends it, and a late transcript only ever lands in the draft it was started for.
use super::Lantern;
use crate::os::Event;
use crate::ui::DictationOwner;

#[derive(Default)]
pub(super) struct Dictating {
    recording: Option<(crate::os::Recording, DictationOwner)>,
    /// The dictation whose transcript counts; cancelling makes older ones stale.
    generation: u64,
}

fn encode(owner: &DictationOwner) -> String {
    match owner {
        DictationOwner::Reply(agent) => format!("reply:{agent}"),
        DictationOwner::Assistant => "assistant".into(),
        DictationOwner::PointAsk => "point".into(),
    }
}

fn decode(owner: &str) -> Option<DictationOwner> {
    match owner {
        "assistant" => Some(DictationOwner::Assistant),
        "point" => Some(DictationOwner::PointAsk),
        other => other.strip_prefix("reply:").map(|agent| DictationOwner::Reply(agent.into())),
    }
}

impl Lantern {
    pub(super) fn toggle_dictation(&mut self, owner: DictationOwner) {
        if let Some((recording, owner)) = self.dictating.recording.take() {
            // Stop: transcribe what was said, on this computer.
            let settings = crate::settings::Settings::load().unwrap_or_default();
            let generation = self.dictating.generation;
            let owner = encode(&owner);
            crate::os::task(
                self.tx.clone(),
                move || recording.finish(settings),
                move |result| Event::Transcript { owner, generation, result },
            );
            return;
        }
        let settings = crate::settings::Settings::load().unwrap_or_default();
        if settings.whisper_model.is_empty() || !std::path::Path::new(&settings.whisper_model).is_file() {
            let setup = if cfg!(windows) { "scripts/setup-dictation.ps1" } else { "scripts/setup-dictation.sh" };
            return self.show(format!("Dictation runs on this computer: set it up once with {setup}."));
        }
        match crate::os::Recording::start() {
            Ok(recording) => {
                self.dictating.generation += 1;
                self.model.dictating = true;
                self.model.point_ask.listening = owner == DictationOwner::PointAsk;
                self.dictating.recording = Some((recording, owner));
            }
            Err(error) => self.show(format!("Couldn't start dictation: {error:#}")),
        }
    }

    /// Ends dictation without using it (its draft went away).
    pub(super) fn cancel_dictation(&mut self) {
        self.dictating.recording = None;
        self.dictating.generation += 1;
        self.model.dictating = false;
        self.model.point_ask.listening = false;
    }

    /// Moving to another conversation ends a dictation that belongs to a different draft.
    pub(super) fn stop_dictation_if_away(&mut self, recipient: Option<&str>) {
        if let Some((_, DictationOwner::Reply(owner))) = &self.dictating.recording
            && Some(owner.as_str()) != recipient
        {
            self.cancel_dictation();
        }
    }

    pub(super) fn transcript(&mut self, owner: &str, generation: u64, result: Result<String, String>) {
        if generation != self.dictating.generation {
            return;
        }
        self.model.dictating = false;
        self.model.point_ask.listening = false;
        let text = match result {
            Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
            Ok(_) => return self.show("Didn't catch that. Try again a little closer to the microphone."),
            Err(error) => return self.show(format!("Dictation: {error}")),
        };
        let append = |buffer: &mut String| {
            if !buffer.is_empty() && !buffer.ends_with(char::is_whitespace) {
                buffer.push(' ');
            }
            buffer.push_str(&text);
        };
        match decode(owner) {
            Some(DictationOwner::Reply(agent)) => self.inputs.drafts.append(&text, &agent),
            Some(DictationOwner::Assistant) => append(&mut self.inputs.assistant),
            Some(DictationOwner::PointAsk) => append(&mut self.inputs.point_ask),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lantern() -> Lantern {
        let (engine, _) = crate::sidecar::test_sender();
        Lantern::new(engine, None)
    }

    #[test]
    fn owners_round_trip() {
        for owner in [DictationOwner::Reply("claude:1:2".into()), DictationOwner::Assistant, DictationOwner::PointAsk] {
            assert_eq!(decode(&encode(&owner)), Some(owner));
        }
    }

    #[test]
    fn a_transcript_lands_only_in_the_draft_it_was_started_for() {
        let mut lantern = lantern();
        lantern.inputs.drafts.address(Some("b"));
        lantern.dictating.generation = 4;
        lantern.transcript("reply:a", 4, Ok("ship it".into()));
        assert_eq!(lantern.inputs.drafts.text_for("a"), "ship it");
        assert_eq!(lantern.inputs.drafts.text_for("b"), "");
        lantern.inputs.assistant = "find".into();
        lantern.transcript("assistant", 4, Ok(" my passport ".into()));
        assert_eq!(lantern.inputs.assistant, "find my passport");
    }

    #[test]
    fn a_stale_transcript_is_dropped() {
        let mut lantern = lantern();
        lantern.dictating.generation = 2;
        lantern.transcript("point", 1, Ok("late".into()));
        assert!(lantern.inputs.point_ask.is_empty());
    }
}
