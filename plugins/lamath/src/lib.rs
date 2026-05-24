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
    PhraseAnalysisExpressionFrameSource, RmsCentroidLoudnessTracker,
    StreamingAudioAnalysisExpressionSource, StreamingAudioAnalysisFrameSource,
    StreamingAudioExpressionFrameSource, StreamingLoudnessFrame, StreamingLoudnessTracker,
};
pub use parameters::PARAMETERS;
pub use patch::{
    EnvelopeConfig, ExcitationSlot, FilterMode, LfoConfig, LfoShape, ModalConfig, ModalPreset,
    ModulationConfig, ModulationDestination, ModulationSlot, ModulationSource, OutputConfig,
    ResonatorConfig, ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
};
pub use plugin::{
    LoadedExcitationBuffer, ResonatorSynth, ResonatorTelemetry, SampleLoadError, SampleLoadReport,
};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

use lindelion_plugin_shell::PluginDescriptor;

#[cfg(test)]
pub(crate) use parameters::ParameterCodec;
#[cfg(test)]
pub(crate) use parameters::patch_parameter_plain_value;
pub(crate) use parameters::{
    FILTER_CUTOFF_PARAMETER_ID, FILTER_RESONANCE_PARAMETER_ID, MASTER_GAIN_PARAMETER_ID,
    MASTER_PAN_PARAMETER_ID, PARALLEL_MIX_A_PARAMETER_ID, PARALLEL_MIX_B_PARAMETER_ID,
    PARAMETER_BINDING_COUNT, RuntimeParameterTarget, SATURATION_PARAMETER_ID,
    apply_parameter_normalized_for_controller, normalized_parameter_value, parameter_binding,
    parameter_binding_by_index, parameter_binding_index,
    parameter_default_normalized_value_by_index, parameter_info, patch_parameter_normalized_value,
    smoothed_runtime_parameter,
};
#[cfg(test)]
pub(crate) use parameters::{ParameterApplyKind, apply_parameter_plain};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Lamath", *b"lamath_resonator");

pub(crate) const RESONATOR_MOD_WHEEL_CONTROLLER: u8 = 1;
pub(crate) const RESONATOR_BRIGHTNESS_CONTROLLER: u8 = 74;

#[cfg(test)]
mod plugin_tests;
