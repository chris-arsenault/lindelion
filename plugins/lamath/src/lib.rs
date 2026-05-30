#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]

mod dsp;
mod parameters;
mod patch;
pub mod patch_io;
mod plugin;
mod runtime;
mod vst3_entry;

pub use dsp::WaveguideStyle;
pub use lindelion_audio_expression::{
    AudioAnalysisExpressionSource, AudioExpressionFeatures, AudioExpressionFrame,
    AudioExpressionFrameSource, AudioExpressionMapping, AudioExpressionSource,
    AudioNoteDetectionConfig, AudioNoteEvent, DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS,
    DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY, DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE,
    DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS, DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT,
    DEFAULT_BRIGHTNESS_CEILING_HZ, DEFAULT_BRIGHTNESS_FLOOR_HZ, DEFAULT_PITCH_BEND_RANGE_SEMITONES,
    DEFAULT_PRESSURE_CEILING_RMS, DEFAULT_PRESSURE_FLOOR_RMS,
    DefaultStreamingAudioAnalysisExpressionSource, PhraseAnalysisExpressionFrameSource,
    RealtimeStreamingAudioAnalysisExpressionSource, RealtimeStreamingAudioAnalysisNoteDetector,
    RmsCentroidLoudnessTracker, StreamingAudioAnalysisExpressionSource,
    StreamingAudioAnalysisFrameSource, StreamingAudioAnalysisNoteDetector,
    StreamingAudioExpressionFrameSource, StreamingAudioNoteDetector, StreamingLoudnessFrame,
    StreamingLoudnessTracker, realtime_audio_analysis_expression_source,
    realtime_audio_analysis_note_detector, streaming_audio_analysis_expression_source,
    streaming_audio_analysis_note_detector,
};
pub use parameters::PARAMETERS;
pub(crate) use patch::normalize_routing_for_resonator_models;
pub use patch::{
    AudioExpressionConfig, AudioInputConfig, AudioInputMode, EnvelopeConfig, ExcitationSlot,
    FilterMode, LfoConfig, LfoShape, LiveExcitationConfig, LiveExcitationMode, MeshConfig,
    ModalConfig, ModalPreset, ModulationConfig, ModulationDestination, ModulationSlot,
    ModulationSource, OutputConfig, ResonatorConfig, ResonatorRouting, ResonatorSynthPatch,
    WaveguideConfig,
};
pub use plugin::{
    LoadedExcitationBuffer, ResonatorSidechainTelemetry, ResonatorSynth, ResonatorTelemetry,
    SampleLoadError, SampleLoadReport,
};

#[doc(hidden)]
pub use dsp::{
    SelectedExcitations as BenchSelectedExcitations, SynthEngine as BenchSynthEngine,
    VoiceTrigger as BenchVoiceTrigger,
    modal::{ModalBank as BenchModalBank, ModalBankParams as BenchModalBankParams},
    waveguide::{
        MeshResonator as BenchMeshResonator, MeshVoiceParams as BenchMeshVoiceParams,
        WaveguideParams as BenchWaveguideParams, WaveguideResonator as BenchWaveguideResonator,
    },
};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

use lindelion_plugin_shell::PluginDescriptor;

#[cfg(test)]
pub(crate) use parameters::ParameterCodec;
#[cfg(test)]
pub(crate) use parameters::RESONATOR_MIX_PARAMETER_ID;
#[cfg(test)]
pub(crate) use parameters::patch_parameter_plain_value;
pub(crate) use parameters::{
    FILTER_CUTOFF_PARAMETER_ID, FILTER_RESONANCE_PARAMETER_ID, MASTER_GAIN_PARAMETER_ID,
    MASTER_PAN_PARAMETER_ID, PARALLEL_MIX_A_PARAMETER_ID, PARALLEL_MIX_B_PARAMETER_ID,
    PARAMETER_BINDING_COUNT, RuntimeParameterTarget, SATURATION_PARAMETER_ID,
    apply_parameter_normalized_for_controller, normalized_parameter_value, parameter_binding,
    parameter_binding_by_index, parameter_binding_index, parameter_info,
    smoothed_runtime_parameter,
};
#[cfg(test)]
pub(crate) use parameters::{ParameterApplyKind, apply_parameter_plain};

#[cfg(test)]
mod test_support;

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Lamath", *b"lamath_resonator");
pub use lindelion_plugin_metadata::LAMATH_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;

pub(crate) const RESONATOR_MOD_WHEEL_CONTROLLER: u8 = 1;
pub(crate) const RESONATOR_BRIGHTNESS_CONTROLLER: u8 = 74;

#[cfg(test)]
mod plugin_tests;
