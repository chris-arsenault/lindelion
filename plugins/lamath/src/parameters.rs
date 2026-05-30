use lindelion_audio_expression::{
    DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
    DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
    DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT, DEFAULT_BRIGHTNESS_CEILING_HZ, DEFAULT_BRIGHTNESS_FLOOR_HZ,
    DEFAULT_PITCH_BEND_RANGE_SEMITONES, DEFAULT_PRESSURE_CEILING_RMS, DEFAULT_PRESSURE_FLOOR_RMS,
};
use lindelion_dsp_utils::{
    db_to_gain, params::StructuralChangePolicy, smoothing::SmoothedParamSpec,
};
pub(crate) use lindelion_plugin_shell::ParameterCodec;
use lindelion_plugin_shell::{
    ParameterApplyDispatcher, ParameterBinding as RegistryParameterBinding,
    ParameterEditorBindingProjection, ParameterFormatter, ParameterId, ParameterInfo,
    ParameterPatchPath, ParameterRange, ParameterRegistry, ParameterSmoothingSpec,
    SmoothedAtomicParam, SmoothedAtomicParamSpec,
};
pub(crate) use lindelion_ui::resonator_vizia::{
    ResonatorEditorControlKind as EditorControlKind,
    ResonatorEditorSurfaceSlot as EditorSurfaceSlot,
};

use crate::dsp::constants::{
    FILTER_RESONANCE, MASTER_GAIN_DB, MASTER_GAIN_LINEAR, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION,
    TUBE_BOUNDARY, WAVEGUIDE_DISPERSION, WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ, WAVEGUIDE_LOOP_GAIN,
};
use crate::{
    AudioInputMode, EnvelopeConfig, FilterMode, LfoShape, LiveExcitationMode, MeshConfig,
    ModalConfig, ModalPreset, ModulationConfig, ModulationDestination, ModulationSource,
    ResonatorConfig, ResonatorRouting, ResonatorSynthPatch, WaveguideConfig, WaveguideStyle,
};

const LIVE: ParameterApplyKind = ParameterApplyKind::Live;
const NOTE_BOUNDARY: ParameterApplyKind =
    ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary);
const LIVE_MUTE_RAMP: ParameterApplyKind =
    ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp);
const RESET_STATE: ParameterApplyKind =
    ParameterApplyKind::Structural(StructuralChangePolicy::ResetState);
const RUNTIME_PARAMETER_SMOOTH_MS: f32 = 20.0;
const RUNTIME_PARAMETER_EPSILON: f32 = 0.000_001;
const FILTER_CUTOFF_EPSILON: f32 = 0.001;

pub(crate) const MASTER_GAIN_PARAMETER_ID: u32 = 1;
pub(crate) const FILTER_CUTOFF_PARAMETER_ID: u32 = 3;
pub(crate) const SATURATION_PARAMETER_ID: u32 = 4;
pub(crate) const MASTER_PAN_PARAMETER_ID: u32 = 5;
pub(crate) const FILTER_RESONANCE_PARAMETER_ID: u32 = 6;
pub(crate) const PARALLEL_MIX_A_PARAMETER_ID: u32 = 11;
pub(crate) const PARALLEL_MIX_B_PARAMETER_ID: u32 = 12;
pub(crate) const RESONATOR_MIX_PARAMETER_ID: u32 = 14;
pub(crate) const AUDIO_INPUT_MODE_PARAMETER_ID: u32 = 100;
pub(crate) const AUDIO_EXPRESSION_ENABLE_PARAMETER_ID: u32 = 101;
pub(crate) const AUDIO_EXPRESSION_PITCH_RANGE_PARAMETER_ID: u32 = 102;
pub(crate) const AUDIO_EXPRESSION_PRESSURE_FLOOR_PARAMETER_ID: u32 = 103;
pub(crate) const AUDIO_EXPRESSION_PRESSURE_CEILING_PARAMETER_ID: u32 = 104;
pub(crate) const AUDIO_EXPRESSION_BRIGHTNESS_FLOOR_PARAMETER_ID: u32 = 105;
pub(crate) const AUDIO_EXPRESSION_BRIGHTNESS_CEILING_PARAMETER_ID: u32 = 106;
pub(crate) const AUDIO_NOTE_ONSET_SENSITIVITY_PARAMETER_ID: u32 = 110;
pub(crate) const AUDIO_NOTE_RELEASE_FLOOR_PARAMETER_ID: u32 = 111;
pub(crate) const AUDIO_NOTE_MIN_LENGTH_PARAMETER_ID: u32 = 112;
pub(crate) const AUDIO_NOTE_PITCH_CONFIDENCE_PARAMETER_ID: u32 = 113;
pub(crate) const AUDIO_NOTE_VELOCITY_AMOUNT_PARAMETER_ID: u32 = 114;
pub(crate) const LIVE_EXCITATION_MODE_PARAMETER_ID: u32 = 120;
pub(crate) const LIVE_EXCITATION_GAIN_PARAMETER_ID: u32 = 121;
pub(crate) const LIVE_EXCITATION_LATCH_WINDOW_PARAMETER_ID: u32 = 122;
pub(crate) const LIVE_EXCITATION_LATCH_PRE_ROLL_PARAMETER_ID: u32 = 123;
pub(crate) const LIVE_EXCITATION_LATCH_FADE_PARAMETER_ID: u32 = 124;

pub(crate) type ParameterBinding = RegistryParameterBinding<
    ParameterPath,
    ParameterApplyKind,
    RuntimeParameterTarget,
    RuntimeSmoothing,
    ParameterFormatter,
    EditorParameterBinding,
>;

macro_rules! parameter_range {
    ($range:expr) => {
        ParameterRange::linear($range.min, $range.max, $range.default)
    };
}

include!("parameters/registry.rs");
pub(crate) const PARAMETER_REGISTRY: ParameterRegistry<ParameterBinding> =
    ParameterRegistry::new(PARAMETER_BINDINGS);
include!("parameters/bindings.rs");
include!("parameters/paths.rs");
include!("parameters/codecs.rs");
include!("parameters/helpers.rs");

#[cfg(test)]
#[path = "parameters/tests.rs"]
mod tests;
