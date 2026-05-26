use lindelion_ui::linnod_vizia::{
    LinnodEditorEnvelope, LinnodEditorPlaybackConfig, LinnodEditorPlaybackMode,
};

use crate::{EnvelopeConfig, PlaybackConfig, PlaybackMode};

pub(super) fn editor_playback_config(config: PlaybackConfig) -> LinnodEditorPlaybackConfig {
    let config = config.sanitized();
    LinnodEditorPlaybackConfig {
        mode: editor_playback_mode(config.mode),
        envelope: editor_envelope(config.envelope),
    }
}

pub(super) fn editor_envelope(envelope: EnvelopeConfig) -> LinnodEditorEnvelope {
    let envelope = envelope.sanitized();
    LinnodEditorEnvelope {
        attack_ms: envelope.attack_ms,
        decay_ms: envelope.decay_ms,
        sustain: envelope.sustain,
        release_ms: envelope.release_ms,
    }
}

pub(super) fn editor_playback_mode(mode: PlaybackMode) -> LinnodEditorPlaybackMode {
    match mode {
        PlaybackMode::OneShot => LinnodEditorPlaybackMode::OneShot,
        PlaybackMode::Gated => LinnodEditorPlaybackMode::Gated,
        PlaybackMode::Looped => LinnodEditorPlaybackMode::Looped,
        PlaybackMode::Continue => LinnodEditorPlaybackMode::Continue,
    }
}

pub(super) fn playback_mode_from_editor(mode: LinnodEditorPlaybackMode) -> PlaybackMode {
    match mode {
        LinnodEditorPlaybackMode::OneShot => PlaybackMode::OneShot,
        LinnodEditorPlaybackMode::Gated => PlaybackMode::Gated,
        LinnodEditorPlaybackMode::Looped => PlaybackMode::Looped,
        LinnodEditorPlaybackMode::Continue => PlaybackMode::Continue,
    }
}

pub(super) fn envelope_from_editor(envelope: LinnodEditorEnvelope) -> EnvelopeConfig {
    EnvelopeConfig {
        attack_ms: envelope.attack_ms,
        decay_ms: envelope.decay_ms,
        sustain: envelope.sustain,
        release_ms: envelope.release_ms,
    }
}
