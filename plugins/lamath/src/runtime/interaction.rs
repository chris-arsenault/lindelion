use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AudioMidiInteractionPolicy {
    mode: AudioInputMode,
}

impl AudioMidiInteractionPolicy {
    pub(super) const fn new(mode: AudioInputMode) -> Self {
        Self { mode }
    }

    pub(super) fn creates_audio_notes(self) -> bool {
        matches!(
            self.mode,
            AudioInputMode::AudioCreatesNotes | AudioInputMode::MidiPlusAudioCreatesNotes
        )
    }

    fn handles_midi_notes(self) -> bool {
        !matches!(self.mode, AudioInputMode::AudioCreatesNotes)
    }

    pub(super) fn should_handle_midi_event(self, event: MidiEvent) -> bool {
        !matches!(event, MidiEvent::Note(_)) || self.handles_midi_notes()
    }
}
