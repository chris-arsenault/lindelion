use lindelion_dsp_utils::{
    db_to_gain, params::StructuralChangePolicy, smoothing::SmoothedParamSpec,
};
use lindelion_plugin_shell::{
    ParameterId, ParameterInfo, ParameterRange, SmoothedAtomicParam, SmoothedAtomicParamSpec,
};
pub(crate) use lindelion_ui::resonator_vizia::{
    ResonatorEditorControlKind as EditorControlKind,
    ResonatorEditorSurfaceSlot as EditorSurfaceSlot,
};

use crate::dsp::constants::{
    FILTER_RESONANCE, MASTER_GAIN_DB, MASTER_GAIN_LINEAR, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION,
    TUBE_BOUNDARY, WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ, WAVEGUIDE_LOOP_GAIN,
};
use crate::{
    EnvelopeConfig, FilterMode, LfoShape, ModalConfig, ModalPreset, ModulationConfig,
    ModulationDestination, ModulationSource, ResonatorConfig, ResonatorRouting,
    ResonatorSynthPatch, WaveguideConfig, WaveguideStyle,
};

const LIVE: ParameterApplyKind = ParameterApplyKind::Live;
const NOTE_BOUNDARY: ParameterApplyKind =
    ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary);
const LIVE_MUTE_RAMP: ParameterApplyKind =
    ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp);
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

macro_rules! runtime_smoothing {
    () => {
        None
    };
    ($smoothing:expr) => {
        Some($smoothing)
    };
}

macro_rules! parameter_range {
    ($range:expr) => {
        ParameterRange::linear($range.min, $range.max, $range.default)
    };
}

macro_rules! parameter_binding_registry {
    ($($info:expr => {
        path: $path:expr,
        apply: $apply:expr,
        runtime: $runtime:expr,
        $(smoothing: $smoothing:expr,)?
        format: $format:expr,
        editor: $editor:expr $(,)?
    }),+ $(,)?) => {
        pub const PARAMETERS: &[ParameterInfo] = &[
            $($info),+
        ];

        pub(crate) const PARAMETER_BINDINGS: &[ParameterBinding] = &[
            $(ParameterBinding::new(
                $info,
                $path,
                $apply,
                $runtime,
                runtime_smoothing!($($smoothing)?),
                $format,
                $editor,
            )),+
        ];
    };
}

include!("parameters/registry.rs");
include!("parameters/bindings.rs");
include!("parameters/paths.rs");
include!("parameters/codecs.rs");
include!("parameters/helpers.rs");

#[cfg(test)]
#[path = "parameters/tests.rs"]
mod tests;
