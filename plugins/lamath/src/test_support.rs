use lindelion_dsp_utils::math::midi_note_to_hz;

use crate::{AudioInputMode, ResonatorSynthPatch};

pub(crate) const TEST_SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const AUDIO_NOTE_TEST_PITCH_CONFIDENCE: f32 = 0.05;
pub(crate) const AUDIO_NOTE_TEST_MINIMUM_LENGTH_MS: f32 = 2_000.0;

pub(crate) fn configure_audio_note_detection(
    patch: &mut ResonatorSynthPatch,
    mode: AudioInputMode,
) {
    patch.audio_input.mode = mode;
    patch.note_detection.pitch_confidence = AUDIO_NOTE_TEST_PITCH_CONFIDENCE;
    patch.note_detection.minimum_note_length_ms = AUDIO_NOTE_TEST_MINIMUM_LENGTH_MS;
}

pub(crate) fn audio_note_detection_patch(
    mut patch: ResonatorSynthPatch,
    mode: AudioInputMode,
) -> ResonatorSynthPatch {
    configure_audio_note_detection(&mut patch, mode);
    patch
}

pub(crate) fn sidechain_sine_note(note: f32, amplitude: f32, len: usize) -> Vec<f32> {
    sidechain_sine_hz(midi_note_to_hz(note), amplitude, len)
}

pub(crate) fn sidechain_sine_hz(frequency_hz: f32, amplitude: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / TEST_SAMPLE_RATE;
            phase.sin() * amplitude
        })
        .collect()
}
