use super::*;
use crate::{
    AudioInputMode, FilterMode, LiveExcitationMode, ModalConfig, ModalPreset,
    ModulationDestination, ModulationSlot, ModulationSource, OutputConfig, ResonatorConfig,
    ResonatorRouting, assert_no_allocations,
    test_support::{
        audio_note_detection_patch, configure_audio_note_detection, first_sample_above,
        sidechain_sine_hz, sidechain_sine_note, sidechain_sine_note_after_silence,
    },
};
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
    math::midi_note_to_hz,
};
use lindelion_midi::DetectedNote;
use lindelion_phrase_analysis::{PhraseAnalysisResult, SegmentedNote};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use lindelion_plugin_shell::{ExpressionStream, ManualExpressionSource, VoiceSlotState};

include!("basic_tests.rs");
include!("audio_note_tests.rs");
include!("live_excitation_tests.rs");
include!("expression_source_tests.rs");
include!("pressure_tests.rs");
include!("expression_tests.rs");
include!("test_helpers.rs");
