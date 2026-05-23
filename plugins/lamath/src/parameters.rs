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

parameter_binding_registry! {
    ParameterInfo::continuous(MASTER_GAIN_PARAMETER_ID, "Master Gain", "dB", parameter_range!(MASTER_GAIN_DB)) => {
        path: ParameterPath::Output(OutputParameter::MasterGain),
        apply: LIVE,
        runtime: RuntimeParameterTarget::Output,
        smoothing: RuntimeSmoothing::mapped(
            SmoothedParamSpec::new(
                MASTER_GAIN_LINEAR.min,
                MASTER_GAIN_LINEAR.max,
                MASTER_GAIN_LINEAR.default,
                RUNTIME_PARAMETER_SMOOTH_MS,
                RUNTIME_PARAMETER_EPSILON,
            ),
            output_gain_from_plain,
        ),
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Master, EditorSurfaceGroup::OutputKnobs, 0, "Master")),
    },
    ParameterInfo::continuous(FILTER_CUTOFF_PARAMETER_ID, "Filter Cutoff", "Hz", parameter_range!(OUTPUT_FILTER_CUTOFF_HZ)) => {
        path: ParameterPath::Output(OutputParameter::FilterCutoff),
        apply: LIVE,
        runtime: RuntimeParameterTarget::Output,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, FILTER_CUTOFF_EPSILON),
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Cutoff, EditorSurfaceGroup::OutputFilter, 0, "Cutoff")),
    },
    ParameterInfo::continuous(SATURATION_PARAMETER_ID, "Saturation", "", ParameterRange::linear(0.0, 1.0, 0.0)) => {
        path: ParameterPath::Output(OutputParameter::Saturation),
        apply: LIVE,
        runtime: RuntimeParameterTarget::Output,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON),
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Saturation, EditorSurfaceGroup::OutputKnobs, 2, "Saturate")),
    },
    ParameterInfo::continuous(MASTER_PAN_PARAMETER_ID, "Master Pan", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::Output(OutputParameter::Pan),
        apply: LIVE,
        runtime: RuntimeParameterTarget::Output,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON),
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Pan, EditorSurfaceGroup::OutputKnobs, 1, "Pan")),
    },
    ParameterInfo::continuous(FILTER_RESONANCE_PARAMETER_ID, "Filter Resonance", "", parameter_range!(FILTER_RESONANCE)) => {
        path: ParameterPath::Output(OutputParameter::FilterResonance),
        apply: LIVE,
        runtime: RuntimeParameterTarget::Output,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON),
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Resonance, EditorSurfaceGroup::OutputFilter, 1, "Res")),
    },
    ParameterInfo::stepped(7, "Filter Mode", "", ParameterRange::linear(0.0, 2.0, 0.0), 2) => {
        path: ParameterPath::Output(OutputParameter::FilterMode),
        apply: LIVE_MUTE_RAMP,
        runtime: RuntimeParameterTarget::Output,
        format: ParameterFormatter::Label(filter_mode_label_from_plain),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::FilterMode, EditorSurfaceGroup::OutputFilter, 2, "Mode")),
    },
    ParameterInfo::stepped(10, "Routing", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::RoutingMode,
        apply: LIVE_MUTE_RAMP,
        runtime: RuntimeParameterTarget::Routing,
        format: ParameterFormatter::Label(routing_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::Routing, EditorSurfaceGroup::Routing, 0, "Routing", "Parallel", "Series", 144.0)),
    },
    ParameterInfo::continuous(PARALLEL_MIX_A_PARAMETER_ID, "Parallel Mix A", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::ParallelMixA,
        apply: LIVE,
        runtime: RuntimeParameterTarget::Routing,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON),
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(PARALLEL_MIX_B_PARAMETER_ID, "Parallel Mix B", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::ParallelMixB,
        apply: LIVE,
        runtime: RuntimeParameterTarget::Routing,
        smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON),
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(13, "Retrigger Resonators", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::RetriggerResonators,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(retrigger_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::RetriggerResonators, EditorSurfaceGroup::ResonatorHeader, 0, "Retrigger", "Carry", "Retrig", 144.0)),
    },

    ParameterInfo::stepped(20, "Resonator A Model", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Model },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(resonator_model_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::ResonatorAModel, EditorSurfaceGroup::ResonatorAHeader, 0, "Model", "Modal", "Wave", 118.0)),
    },
    ParameterInfo::stepped(21, "Resonator A Modal Preset", "", ParameterRange::linear(0.0, 6.0, 1.0), 6) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Preset) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(modal_preset_label_from_plain),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorAPreset, EditorSurfaceGroup::ResonatorAControls, 0, "Preset")),
    },
    ParameterInfo::stepped(22, "Resonator A Mode Count", "", ParameterRange::linear(16.0, 256.0, 64.0), 240) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::ModeCount) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(23, "Resonator A Semitone", "st", ParameterRange::linear(-24.0, 24.0, 0.0), 48) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Semitone) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(24, "Resonator A Cents", "ct", ParameterRange::linear(-100.0, 100.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Cents) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(25, "Resonator A Inharmonicity", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Inharmonicity) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(26, "Resonator A Brightness", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Brightness) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorABrightness, EditorSurfaceGroup::ResonatorAControls, 1, "Bright")),
    },
    ParameterInfo::continuous(27, "Resonator A Decay", "s", ParameterRange::linear(0.05, 10.0, 1.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::Decay) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorADecay, EditorSurfaceGroup::ResonatorAControls, 2, "Decay")),
    },
    ParameterInfo::continuous(28, "Resonator A Decay Tilt", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::DecayTilt) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(29, "Resonator A Strike Position", "", parameter_range!(STRIKE_POSITION)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Modal(ModalParameter::StrikePosition) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(30, "Resonator A Loop Filter", "Hz", parameter_range!(WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopFilter) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(31, "Resonator A Loop Resonance", "", parameter_range!(FILTER_RESONANCE)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopResonance) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(32, "Resonator A Loop Gain", "", parameter_range!(WAVEGUIDE_LOOP_GAIN)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopGain) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(33, "Resonator A Nonlinearity", "", ParameterRange::linear(0.0, 1.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Nonlinearity) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(34, "Resonator A Waveguide Position", "", parameter_range!(STRIKE_POSITION)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Position) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(35, "Resonator A Waveguide Style", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Style) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(waveguide_style_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::ResonatorAWaveguideStyle, EditorSurfaceGroup::ResonatorAHeader, 1, "Style", "String", "Tube", 118.0)),
    },
    ParameterInfo::continuous(36, "Resonator A Boundary Reflection", "", parameter_range!(TUBE_BOUNDARY.reflection)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ResonatorParameter::Waveguide(WaveguideParameter::BoundaryReflection) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorABoundaryReflection, EditorSurfaceGroup::ResonatorAControls, 3, "Reflect")),
    },

    ParameterInfo::stepped(40, "Resonator B Model", "", ParameterRange::linear(0.0, 1.0, 1.0), 1) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Model },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(resonator_model_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::ResonatorBModel, EditorSurfaceGroup::ResonatorBHeader, 0, "Model", "Modal", "Wave", 118.0)),
    },
    ParameterInfo::stepped(41, "Resonator B Modal Preset", "", ParameterRange::linear(0.0, 6.0, 1.0), 6) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Preset) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(modal_preset_label_from_plain),
        editor: None,
    },
    ParameterInfo::stepped(42, "Resonator B Mode Count", "", ParameterRange::linear(16.0, 256.0, 64.0), 240) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::ModeCount) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(43, "Resonator B Semitone", "st", ParameterRange::linear(-24.0, 24.0, 0.0), 48) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Semitone) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(44, "Resonator B Cents", "ct", ParameterRange::linear(-100.0, 100.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Cents) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(45, "Resonator B Inharmonicity", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Inharmonicity) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(46, "Resonator B Brightness", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Brightness) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(47, "Resonator B Decay", "s", ParameterRange::linear(0.05, 10.0, 1.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::Decay) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(48, "Resonator B Decay Tilt", "", ParameterRange::linear(0.0, 1.0, 0.5)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::DecayTilt) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(49, "Resonator B Strike Position", "", parameter_range!(STRIKE_POSITION)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Modal(ModalParameter::StrikePosition) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(50, "Resonator B Loop Filter", "Hz", parameter_range!(WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopFilter) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBLoopFilter, EditorSurfaceGroup::ResonatorBControls, 0, "Filter")),
    },
    ParameterInfo::continuous(51, "Resonator B Loop Resonance", "", parameter_range!(FILTER_RESONANCE)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopResonance) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(52, "Resonator B Loop Gain", "", parameter_range!(WAVEGUIDE_LOOP_GAIN)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::LoopGain) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBLoopGain, EditorSurfaceGroup::ResonatorBControls, 1, "Loop")),
    },
    ParameterInfo::continuous(53, "Resonator B Nonlinearity", "", ParameterRange::linear(0.0, 1.0, 0.0)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Nonlinearity) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBNonlinearity, EditorSurfaceGroup::ResonatorBControls, 2, "Drive")),
    },
    ParameterInfo::continuous(54, "Resonator B Waveguide Position", "", parameter_range!(STRIKE_POSITION)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Position) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(55, "Resonator B Waveguide Style", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::Style) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(waveguide_style_label_from_plain),
        editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::ResonatorBWaveguideStyle, EditorSurfaceGroup::ResonatorBHeader, 1, "Style", "String", "Tube", 118.0)),
    },
    ParameterInfo::continuous(56, "Resonator B Boundary Reflection", "", parameter_range!(TUBE_BOUNDARY.reflection)) => {
        path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ResonatorParameter::Waveguide(WaveguideParameter::BoundaryReflection) },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBBoundaryReflection, EditorSurfaceGroup::ResonatorBControls, 3, "Reflect")),
    },

    ParameterInfo::continuous(60, "Amp Attack", "ms", ParameterRange::linear(0.0, 5_000.0, 1.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Amp, parameter: EnvelopeParameter::Attack },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AmpAttack, EditorSurfaceGroup::OutputEnvelope, 0, "Attack")),
    },
    ParameterInfo::continuous(61, "Amp Decay", "ms", ParameterRange::linear(0.0, 5_000.0, 80.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Amp, parameter: EnvelopeParameter::Decay },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(62, "Amp Sustain", "", ParameterRange::linear(0.0, 1.0, 1.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Amp, parameter: EnvelopeParameter::Sustain },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(63, "Amp Release", "ms", ParameterRange::linear(0.0, 10_000.0, 250.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Amp, parameter: EnvelopeParameter::Release },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AmpRelease, EditorSurfaceGroup::OutputEnvelope, 1, "Release")),
    },
    ParameterInfo::continuous(64, "Secondary Attack", "ms", ParameterRange::linear(0.0, 5_000.0, 0.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Secondary, parameter: EnvelopeParameter::Attack },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(65, "Secondary Decay", "ms", ParameterRange::linear(0.0, 5_000.0, 250.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Secondary, parameter: EnvelopeParameter::Decay },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(66, "Secondary Sustain", "", ParameterRange::linear(0.0, 1.0, 0.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Secondary, parameter: EnvelopeParameter::Sustain },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(67, "Secondary Release", "ms", ParameterRange::linear(0.0, 10_000.0, 150.0)) => {
        path: ParameterPath::Envelope { target: EnvelopeTarget::Secondary, parameter: EnvelopeParameter::Release },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(68, "LFO Rate", "Hz", ParameterRange::linear(0.01, 100.0, 2.0)) => {
        path: ParameterPath::LfoRate,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LfoRate, EditorSurfaceGroup::OutputModulation, 0, "LFO")),
    },
    ParameterInfo::stepped(69, "LFO Shape", "", ParameterRange::linear(0.0, 4.0, 0.0), 4) => {
        path: ParameterPath::LfoShape,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(lfo_shape_label_from_plain),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LfoShape, EditorSurfaceGroup::OutputModulation, 1, "Shape")),
    },
    ParameterInfo::stepped(70, "LFO Tempo Sync", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::LfoTempoSync,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(tempo_sync_label_from_plain),
        editor: None,
    },
    ParameterInfo::continuous(71, "Pitch Bend Range", "st", ParameterRange::linear(0.0, 24.0, 2.0)) => {
        path: ParameterPath::PitchBendRange,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::continuous(72, "Velocity Excitation", "", ParameterRange::linear(0.0, 1.0, 1.0)) => {
        path: ParameterPath::VelocityExcitationDepth,
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },

    ParameterInfo::stepped(80, "Mod 1 Enabled", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::ModulationSlot { slot: 0, parameter: ModulationSlotParameter::Enabled },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(enabled_label_from_plain),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Mod1Enabled, EditorSurfaceGroup::OutputModulation, 2, "Enable")),
    },
    ParameterInfo::stepped(81, "Mod 1 Source", "", ParameterRange::linear(0.0, 5.0, 2.0), 5) => {
        path: ParameterPath::ModulationSlot { slot: 0, parameter: ModulationSlotParameter::Source },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_source_label),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Mod1Source, EditorSurfaceGroup::OutputModulation, 3, "Source")),
    },
    ParameterInfo::stepped(82, "Mod 1 Destination", "", ParameterRange::linear(0.0, 6.0, 0.0), 6) => {
        path: ParameterPath::ModulationSlot { slot: 0, parameter: ModulationSlotParameter::Destination },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_destination_label),
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Mod1Destination, EditorSurfaceGroup::OutputModulation, 4, "Target")),
    },
    ParameterInfo::continuous(83, "Mod 1 Amount", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::ModulationSlot { slot: 0, parameter: ModulationSlotParameter::Amount },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Mod1Amount, EditorSurfaceGroup::OutputModulation, 5, "Amount")),
    },
    ParameterInfo::stepped(84, "Mod 2 Enabled", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::ModulationSlot { slot: 1, parameter: ModulationSlotParameter::Enabled },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(enabled_label_from_plain),
        editor: None,
    },
    ParameterInfo::stepped(85, "Mod 2 Source", "", ParameterRange::linear(0.0, 5.0, 2.0), 5) => {
        path: ParameterPath::ModulationSlot { slot: 1, parameter: ModulationSlotParameter::Source },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_source_label),
        editor: None,
    },
    ParameterInfo::stepped(86, "Mod 2 Destination", "", ParameterRange::linear(0.0, 6.0, 0.0), 6) => {
        path: ParameterPath::ModulationSlot { slot: 1, parameter: ModulationSlotParameter::Destination },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_destination_label),
        editor: None,
    },
    ParameterInfo::continuous(87, "Mod 2 Amount", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::ModulationSlot { slot: 1, parameter: ModulationSlotParameter::Amount },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(88, "Mod 3 Enabled", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::ModulationSlot { slot: 2, parameter: ModulationSlotParameter::Enabled },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(enabled_label_from_plain),
        editor: None,
    },
    ParameterInfo::stepped(89, "Mod 3 Source", "", ParameterRange::linear(0.0, 5.0, 2.0), 5) => {
        path: ParameterPath::ModulationSlot { slot: 2, parameter: ModulationSlotParameter::Source },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_source_label),
        editor: None,
    },
    ParameterInfo::stepped(90, "Mod 3 Destination", "", ParameterRange::linear(0.0, 6.0, 0.0), 6) => {
        path: ParameterPath::ModulationSlot { slot: 2, parameter: ModulationSlotParameter::Destination },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_destination_label),
        editor: None,
    },
    ParameterInfo::continuous(91, "Mod 3 Amount", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::ModulationSlot { slot: 2, parameter: ModulationSlotParameter::Amount },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
    ParameterInfo::stepped(92, "Mod 4 Enabled", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => {
        path: ParameterPath::ModulationSlot { slot: 3, parameter: ModulationSlotParameter::Enabled },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(enabled_label_from_plain),
        editor: None,
    },
    ParameterInfo::stepped(93, "Mod 4 Source", "", ParameterRange::linear(0.0, 5.0, 2.0), 5) => {
        path: ParameterPath::ModulationSlot { slot: 3, parameter: ModulationSlotParameter::Source },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_source_label),
        editor: None,
    },
    ParameterInfo::stepped(94, "Mod 4 Destination", "", ParameterRange::linear(0.0, 6.0, 0.0), 6) => {
        path: ParameterPath::ModulationSlot { slot: 3, parameter: ModulationSlotParameter::Destination },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Label(format_modulation_destination_label),
        editor: None,
    },
    ParameterInfo::continuous(95, "Mod 4 Amount", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => {
        path: ParameterPath::ModulationSlot { slot: 3, parameter: ModulationSlotParameter::Amount },
        apply: NOTE_BOUNDARY,
        runtime: RuntimeParameterTarget::None,
        format: ParameterFormatter::Plain,
        editor: None,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterApplyKind {
    Live,
    Structural(StructuralChangePolicy),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParameterTarget {
    None,
    Output,
    Routing,
}

impl RuntimeParameterTarget {
    pub(crate) const fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RuntimeSmoothing {
    Identity {
        smoothing_ms: f32,
        epsilon: f32,
    },
    Mapped {
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: fn(f32) -> f32,
    },
}

impl RuntimeSmoothing {
    pub(crate) const fn identity(smoothing_ms: f32, epsilon: f32) -> Self {
        Self::Identity {
            smoothing_ms,
            epsilon,
        }
    }

    pub(crate) const fn mapped(
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: fn(f32) -> f32,
    ) -> Self {
        Self::Mapped {
            smoothed,
            plain_to_smoothed,
        }
    }

    pub(crate) fn spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
        match self {
            Self::Identity {
                smoothing_ms,
                epsilon,
            } => SmoothedAtomicParamSpec::from_parameter(info, smoothing_ms, epsilon),
            Self::Mapped {
                smoothed,
                plain_to_smoothed,
            } => SmoothedAtomicParamSpec::mapped(info, smoothed, plain_to_smoothed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParameterBinding {
    info: ParameterInfo,
    path: ParameterPath,
    apply_kind: ParameterApplyKind,
    runtime_target: RuntimeParameterTarget,
    smoothing: Option<RuntimeSmoothing>,
    formatter: ParameterFormatter,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    editor: Option<EditorParameterBinding>,
}

impl ParameterBinding {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        info: ParameterInfo,
        path: ParameterPath,
        apply_kind: ParameterApplyKind,
        runtime_target: RuntimeParameterTarget,
        smoothing: Option<RuntimeSmoothing>,
        formatter: ParameterFormatter,
        editor: Option<EditorParameterBinding>,
    ) -> Self {
        Self {
            info,
            path,
            apply_kind,
            runtime_target,
            smoothing,
            formatter,
            editor,
        }
    }

    pub(crate) const fn info(self) -> ParameterInfo {
        self.info
    }

    pub(crate) const fn id(self) -> ParameterId {
        self.info.id
    }

    pub(crate) const fn runtime_target(self) -> RuntimeParameterTarget {
        self.runtime_target
    }

    pub(crate) fn smoothed_atomic_spec(self) -> Option<SmoothedAtomicParamSpec> {
        self.smoothing.map(|smoothing| smoothing.spec(self.info))
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn editor(self) -> Option<EditorParameterBinding> {
        self.editor
    }

    pub(crate) fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        self.path.plain_value(patch)
    }

    pub(crate) fn apply_plain(
        self,
        patch: &mut ResonatorSynthPatch,
        value: f32,
    ) -> ParameterApplyKind {
        self.path.apply_plain(patch, value);
        self.apply_kind
    }

    pub(crate) fn format_plain_value(self, value: f32) -> String {
        self.formatter.format(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorParameterBinding {
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    slot: EditorSurfaceSlot,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    group: EditorSurfaceGroup,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    order: u8,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    label: &'static str,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    control: EditorControlKind,
}

impl EditorParameterBinding {
    pub(crate) const fn knob(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Knob,
        }
    }

    pub(crate) const fn slider(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Slider,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn binary(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Binary {
                left_label,
                right_label,
                width,
            },
        }
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn slot(self) -> EditorSurfaceSlot {
        self.slot
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn group(self) -> EditorSurfaceGroup {
        self.group
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn order(self) -> u8 {
        self.order
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn label(self) -> &'static str {
        self.label
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn control(self) -> EditorControlKind {
        self.control
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorSurfaceGroup {
    ResonatorHeader,
    ResonatorAHeader,
    ResonatorAControls,
    ResonatorBHeader,
    ResonatorBControls,
    Routing,
    OutputKnobs,
    OutputFilter,
    OutputEnvelope,
    OutputModulation,
}

impl EditorSurfaceGroup {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const REQUIRED: [Self; 10] = [
        Self::ResonatorHeader,
        Self::ResonatorAHeader,
        Self::ResonatorAControls,
        Self::ResonatorBHeader,
        Self::ResonatorBControls,
        Self::Routing,
        Self::OutputKnobs,
        Self::OutputFilter,
        Self::OutputEnvelope,
        Self::OutputModulation,
    ];
}

#[derive(Debug, Clone, Copy)]
enum ParameterFormatter {
    Plain,
    Label(fn(f32) -> &'static str),
}

impl ParameterFormatter {
    fn format(self, value: f32) -> String {
        match self {
            Self::Plain => format_plain_value(value),
            Self::Label(label) => label(value).to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParameterPath {
    Output(OutputParameter),
    RoutingMode,
    ParallelMixA,
    ParallelMixB,
    RetriggerResonators,
    Resonator {
        slot: ResonatorSlot,
        parameter: ResonatorParameter,
    },
    Envelope {
        target: EnvelopeTarget,
        parameter: EnvelopeParameter,
    },
    LfoRate,
    LfoShape,
    LfoTempoSync,
    PitchBendRange,
    VelocityExcitationDepth,
    ModulationSlot {
        slot: usize,
        parameter: ModulationSlotParameter,
    },
}

impl ParameterPath {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        match self {
            Self::Output(parameter) => parameter.plain_value(patch.output),
            Self::RoutingMode => RoutingMode::from_routing(patch.routing).plain(),
            Self::ParallelMixA => parallel_mix_a(patch.routing),
            Self::ParallelMixB => parallel_mix_b(patch.routing),
            Self::RetriggerResonators => bool_plain(patch.retrigger_resonators),
            Self::Resonator { slot, parameter } => parameter.plain_value(slot.config(patch)),
            Self::Envelope { target, parameter } => parameter.plain_value(target.config(patch)),
            Self::LfoRate => patch.modulation.lfo.rate_hz,
            Self::LfoShape => patch.modulation.lfo.shape.plain(),
            Self::LfoTempoSync => bool_plain(patch.modulation.lfo.tempo_sync),
            Self::PitchBendRange => patch.modulation.pitch_bend_range_semitones,
            Self::VelocityExcitationDepth => patch.modulation.velocity_to_excitation_depth,
            Self::ModulationSlot { slot, parameter } => parameter
                .plain_value(&patch.modulation, slot)
                .unwrap_or_default(),
        }
    }

    fn apply_plain(self, patch: &mut ResonatorSynthPatch, value: f32) {
        match self {
            Self::Output(parameter) => parameter.apply_plain(&mut patch.output, value),
            Self::RoutingMode => {
                patch.routing = RoutingMode::from_plain(value).apply_to(patch.routing);
            }
            Self::ParallelMixA => {
                patch.routing = set_parallel_mix(patch.routing, MixSide::A, value)
            }
            Self::ParallelMixB => {
                patch.routing = set_parallel_mix(patch.routing, MixSide::B, value)
            }
            Self::RetriggerResonators => patch.retrigger_resonators = bool_from_plain(value),
            Self::Resonator { slot, parameter } => {
                parameter.apply_plain(slot.config_mut(patch), value);
            }
            Self::Envelope { target, parameter } => {
                parameter.apply_plain(target.config_mut(patch), value);
            }
            Self::LfoRate => {
                patch.modulation.lfo.rate_hz = finite_value(value, 0.01, 100.0, 2.0);
            }
            Self::LfoShape => {
                patch.modulation.lfo.shape = LfoShape::from_plain(value);
            }
            Self::LfoTempoSync => {
                patch.modulation.lfo.tempo_sync = bool_from_plain(value);
            }
            Self::PitchBendRange => {
                patch.modulation.pitch_bend_range_semitones = finite_value(value, 0.0, 24.0, 2.0);
            }
            Self::VelocityExcitationDepth => {
                patch.modulation.velocity_to_excitation_depth = finite_value(value, 0.0, 1.0, 1.0);
            }
            Self::ModulationSlot { slot, parameter } => {
                parameter.apply_plain(&mut patch.modulation, slot, value);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputParameter {
    MasterGain,
    FilterCutoff,
    Saturation,
    Pan,
    FilterResonance,
    FilterMode,
}

impl OutputParameter {
    fn plain_value(self, output: crate::OutputConfig) -> f32 {
        match self {
            Self::MasterGain => output.master_gain_db,
            Self::FilterCutoff => output.filter_cutoff,
            Self::Saturation => output.saturation_drive,
            Self::Pan => output.master_pan,
            Self::FilterResonance => output.filter_resonance,
            Self::FilterMode => output.filter_mode.plain(),
        }
    }

    fn apply_plain(self, output: &mut crate::OutputConfig, value: f32) {
        match self {
            Self::MasterGain => output.master_gain_db = MASTER_GAIN_DB.clamp(value),
            Self::FilterCutoff => {
                output.filter_cutoff = OUTPUT_FILTER_CUTOFF_HZ.clamp(value);
            }
            Self::Saturation => output.saturation_drive = finite_value(value, 0.0, 1.0, 0.0),
            Self::Pan => output.master_pan = finite_value(value, -1.0, 1.0, 0.0),
            Self::FilterResonance => {
                output.filter_resonance = FILTER_RESONANCE.clamp(value);
            }
            Self::FilterMode => output.filter_mode = FilterMode::from_plain(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorSlot {
    A,
    B,
}

impl ResonatorSlot {
    fn config(self, patch: &ResonatorSynthPatch) -> ResonatorConfig {
        match self {
            Self::A => patch.resonator_a,
            Self::B => patch.resonator_b,
        }
    }

    fn config_mut(self, patch: &mut ResonatorSynthPatch) -> &mut ResonatorConfig {
        match self {
            Self::A => &mut patch.resonator_a,
            Self::B => &mut patch.resonator_b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorParameter {
    Model,
    Modal(ModalParameter),
    Waveguide(WaveguideParameter),
}

impl ResonatorParameter {
    fn plain_value(self, config: ResonatorConfig) -> f32 {
        match self {
            Self::Model => ResonatorModel::from_config(config).plain(),
            Self::Modal(parameter) => parameter.plain_value(modal_config_from(config)),
            Self::Waveguide(parameter) => parameter.plain_value(waveguide_config_from(config)),
        }
    }

    fn apply_plain(self, config: &mut ResonatorConfig, value: f32) {
        match self {
            Self::Model => *config = ResonatorModel::from_plain(value).config_from(*config),
            Self::Modal(parameter) => parameter.apply_if_selected(config, value),
            Self::Waveguide(parameter) => parameter.apply_if_selected(config, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalParameter {
    Preset,
    ModeCount,
    Semitone,
    Cents,
    Inharmonicity,
    Brightness,
    Decay,
    DecayTilt,
    StrikePosition,
}

impl ModalParameter {
    fn plain_value(self, config: ModalConfig) -> f32 {
        match self {
            Self::Preset => config.preset.plain(),
            Self::ModeCount => f32::from(config.mode_count),
            Self::Semitone => f32::from(config.semitone_offset),
            Self::Cents => config.cent_offset,
            Self::Inharmonicity => config.inharmonicity,
            Self::Brightness => config.brightness,
            Self::Decay => config.decay_global,
            Self::DecayTilt => config.decay_tilt,
            Self::StrikePosition => config.position_of_strike,
        }
    }

    fn apply_if_selected(self, config: &mut ResonatorConfig, value: f32) {
        match config {
            ResonatorConfig::Modal(modal) => self.apply_plain(modal, value),
            ResonatorConfig::Waveguide(waveguide) => {
                // Fundamental tune and strike position are shared resonator concepts.
                match self {
                    Self::Semitone => {
                        waveguide.semitone_offset =
                            finite_value(value, -24.0, 24.0, 0.0).round() as i8;
                    }
                    Self::Cents => waveguide.cent_offset = finite_value(value, -100.0, 100.0, 0.0),
                    Self::StrikePosition => {
                        waveguide.position_of_strike = STRIKE_POSITION.clamp(value);
                    }
                    _ => {}
                }
            }
        }
    }

    fn apply_plain(self, config: &mut ModalConfig, value: f32) {
        match self {
            Self::Preset => config.preset = ModalPreset::from_plain(value),
            Self::ModeCount => {
                config.mode_count = finite_value(value, 16.0, 256.0, 64.0).round() as u16;
            }
            Self::Semitone => {
                config.semitone_offset = finite_value(value, -24.0, 24.0, 0.0).round() as i8;
            }
            Self::Cents => config.cent_offset = finite_value(value, -100.0, 100.0, 0.0),
            Self::Inharmonicity => config.inharmonicity = finite_value(value, -1.0, 1.0, 0.0),
            Self::Brightness => config.brightness = finite_value(value, 0.0, 1.0, 0.5),
            Self::Decay => config.decay_global = finite_value(value, 0.05, 10.0, 1.0),
            Self::DecayTilt => config.decay_tilt = finite_value(value, 0.0, 1.0, 0.5),
            Self::StrikePosition => {
                config.position_of_strike = STRIKE_POSITION.clamp(value);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaveguideParameter {
    LoopFilter,
    LoopResonance,
    LoopGain,
    Nonlinearity,
    Position,
    Style,
    BoundaryReflection,
}

impl WaveguideParameter {
    fn plain_value(self, config: WaveguideConfig) -> f32 {
        match self {
            Self::LoopFilter => config.loop_filter_cutoff,
            Self::LoopResonance => config.loop_filter_resonance,
            Self::LoopGain => config.loop_gain,
            Self::Nonlinearity => config.loop_nonlinearity,
            Self::Position => config.position_of_strike,
            Self::Style => config.style.plain(),
            Self::BoundaryReflection => config.boundary_reflection,
        }
    }

    fn apply_if_selected(self, config: &mut ResonatorConfig, value: f32) {
        match config {
            ResonatorConfig::Waveguide(waveguide) => self.apply_plain(waveguide, value),
            ResonatorConfig::Modal(modal) => {
                if self == Self::Position {
                    modal.position_of_strike = STRIKE_POSITION.clamp(value);
                }
            }
        }
    }

    fn apply_plain(self, config: &mut WaveguideConfig, value: f32) {
        match self {
            Self::LoopFilter => {
                config.loop_filter_cutoff = WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ.clamp(value)
            }
            Self::LoopResonance => config.loop_filter_resonance = FILTER_RESONANCE.clamp(value),
            Self::LoopGain => config.loop_gain = WAVEGUIDE_LOOP_GAIN.clamp(value),
            Self::Nonlinearity => config.loop_nonlinearity = finite_value(value, 0.0, 1.0, 0.0),
            Self::Position => config.position_of_strike = STRIKE_POSITION.clamp(value),
            Self::Style => config.style = WaveguideStyle::from_plain(value),
            Self::BoundaryReflection => {
                config.boundary_reflection = TUBE_BOUNDARY.reflection(value);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeTarget {
    Amp,
    Secondary,
}

impl EnvelopeTarget {
    fn config(self, patch: &ResonatorSynthPatch) -> EnvelopeConfig {
        match self {
            Self::Amp => patch.modulation.amp_envelope,
            Self::Secondary => patch.modulation.secondary_envelope,
        }
    }

    fn config_mut(self, patch: &mut ResonatorSynthPatch) -> &mut EnvelopeConfig {
        match self {
            Self::Amp => &mut patch.modulation.amp_envelope,
            Self::Secondary => &mut patch.modulation.secondary_envelope,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeParameter {
    Attack,
    Decay,
    Sustain,
    Release,
}

impl EnvelopeParameter {
    fn plain_value(self, config: EnvelopeConfig) -> f32 {
        match self {
            Self::Attack => config.attack_ms,
            Self::Decay => config.decay_ms,
            Self::Sustain => config.sustain,
            Self::Release => config.release_ms,
        }
    }

    fn apply_plain(self, config: &mut EnvelopeConfig, value: f32) {
        match self {
            Self::Attack => config.attack_ms = finite_value(value, 0.0, 5_000.0, 1.0),
            Self::Decay => config.decay_ms = finite_value(value, 0.0, 5_000.0, 80.0),
            Self::Sustain => config.sustain = finite_value(value, 0.0, 1.0, 1.0),
            Self::Release => config.release_ms = finite_value(value, 0.0, 10_000.0, 250.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModulationSlotParameter {
    Enabled,
    Source,
    Destination,
    Amount,
}

impl ModulationSlotParameter {
    fn plain_value(self, config: &ModulationConfig, slot: usize) -> Option<f32> {
        let slot = config.slots.get(slot)?;
        Some(match self {
            Self::Enabled => bool_plain(slot.enabled),
            Self::Source => slot.source.plain(),
            Self::Destination => slot.destination.plain(),
            Self::Amount => slot.amount,
        })
    }

    fn apply_plain(self, config: &mut ModulationConfig, slot: usize, value: f32) {
        let Some(slot) = config.slots.get_mut(slot) else {
            return;
        };
        match self {
            Self::Enabled => slot.enabled = bool_from_plain(value),
            Self::Source => slot.source = ModulationSource::from_plain(value),
            Self::Destination => slot.destination = ModulationDestination::from_plain(value),
            Self::Amount => slot.amount = finite_value(value, -1.0, 1.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixSide {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorModel {
    Modal,
    Waveguide,
}

impl ResonatorModel {
    fn from_config(config: ResonatorConfig) -> Self {
        match config {
            ResonatorConfig::Modal(_) => Self::Modal,
            ResonatorConfig::Waveguide(_) => Self::Waveguide,
        }
    }

    fn config_from(self, current: ResonatorConfig) -> ResonatorConfig {
        match self {
            Self::Modal => ResonatorConfig::Modal(modal_config_from(current)),
            Self::Waveguide => ResonatorConfig::Waveguide(waveguide_config_from(current)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingMode {
    Parallel,
    Series,
}

impl RoutingMode {
    fn from_routing(routing: ResonatorRouting) -> Self {
        match routing {
            ResonatorRouting::Parallel { .. } => Self::Parallel,
            ResonatorRouting::Series { .. } => Self::Series,
        }
    }

    fn apply_to(self, current: ResonatorRouting) -> ResonatorRouting {
        let mix_a = parallel_mix_a(current);
        let mix_b = parallel_mix_b(current);
        match self {
            Self::Parallel => ResonatorRouting::Parallel { mix_a, mix_b },
            Self::Series => ResonatorRouting::Series { mix_a, mix_b },
        }
    }
}

pub(crate) trait ParameterCodec: Copy + Sized {
    const MAX_INDEX: u32;

    fn from_index(index: u32) -> Self;
    fn to_index(self) -> u32;
    fn label(self) -> &'static str;

    fn from_plain(value: f32) -> Self {
        Self::from_index(stepped_index(value, Self::MAX_INDEX))
    }

    fn plain(self) -> f32 {
        self.to_index() as f32
    }

    fn label_from_plain(value: f32) -> &'static str {
        Self::from_plain(value).label()
    }
}

impl ParameterCodec for FilterMode {
    const MAX_INDEX: u32 = 2;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::BandPass,
            2 => Self::HighPass,
            _ => Self::LowPass,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::LowPass => 0,
            Self::BandPass => 1,
            Self::HighPass => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::BandPass => "BP",
            Self::HighPass => "HP",
        }
    }
}

impl ParameterCodec for RoutingMode {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::Series
        } else {
            Self::Parallel
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Parallel => 0,
            Self::Series => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Parallel => "Parallel",
            Self::Series => "Series",
        }
    }
}

impl ParameterCodec for ResonatorModel {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::Waveguide
        } else {
            Self::Modal
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Modal => 0,
            Self::Waveguide => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Modal => "Modal",
            Self::Waveguide => "Waveguide",
        }
    }
}

impl ParameterCodec for ModalPreset {
    const MAX_INDEX: u32 = 6;

    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Kalimba,
            1 => Self::Marimba,
            2 => Self::Bell,
            3 => Self::GlassBowl,
            4 => Self::MetalBar,
            5 => Self::Woodblock,
            _ => Self::GenericStrike,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Kalimba => 0,
            Self::Marimba => 1,
            Self::Bell => 2,
            Self::GlassBowl => 3,
            Self::MetalBar => 4,
            Self::Woodblock => 5,
            Self::GenericStrike => 6,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Kalimba => "Kalimba",
            Self::Marimba => "Marimba",
            Self::Bell => "Bell",
            Self::GlassBowl => "Glass Bowl",
            Self::MetalBar => "Metal Bar",
            Self::Woodblock => "Woodblock",
            Self::GenericStrike => "Generic",
        }
    }
}

impl ParameterCodec for LfoShape {
    const MAX_INDEX: u32 = 4;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Triangle,
            2 => Self::Saw,
            3 => Self::Square,
            4 => Self::SampleAndHold,
            _ => Self::Sine,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Sine => 0,
            Self::Triangle => 1,
            Self::Saw => 2,
            Self::Square => 3,
            Self::SampleAndHold => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::Saw => "Saw",
            Self::Square => "Square",
            Self::SampleAndHold => "S&H",
        }
    }
}

impl ParameterCodec for ModulationSource {
    const MAX_INDEX: u32 = 5;

    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::SecondaryEnvelope,
            1 => Self::Lfo,
            2 => Self::Velocity,
            3 => Self::Aftertouch,
            4 => Self::ModWheel,
            _ => Self::Brightness,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::SecondaryEnvelope => 0,
            Self::Lfo => 1,
            Self::Velocity => 2,
            Self::Aftertouch => 3,
            Self::ModWheel => 4,
            Self::Brightness => 5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SecondaryEnvelope => "Secondary Env",
            Self::Lfo => "LFO",
            Self::Velocity => "Velocity",
            Self::Aftertouch => "Pressure",
            Self::ModWheel => "Mod Wheel",
            Self::Brightness => "Brightness",
        }
    }
}

impl ParameterCodec for ModulationDestination {
    const MAX_INDEX: u32 = 6;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::ResonatorADamping,
            2 => Self::ResonatorBDamping,
            3 => Self::ResonatorAPosition,
            4 => Self::ResonatorBPosition,
            5 => Self::ExcitationGain,
            6 => Self::LfoRate,
            _ => Self::FilterCutoff,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::FilterCutoff => 0,
            Self::ResonatorADamping => 1,
            Self::ResonatorBDamping => 2,
            Self::ResonatorAPosition => 3,
            Self::ResonatorBPosition => 4,
            Self::ExcitationGain => 5,
            Self::LfoRate => 6,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FilterCutoff => "Filter Cutoff",
            Self::ResonatorADamping => "Res A Damping",
            Self::ResonatorBDamping => "Res B Damping",
            Self::ResonatorAPosition => "Res A Position",
            Self::ResonatorBPosition => "Res B Position",
            Self::ExcitationGain => "Excitation Gain",
            Self::LfoRate => "LFO Rate",
        }
    }
}

impl ParameterCodec for WaveguideStyle {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 { Self::Tube } else { Self::String }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::String => 0,
            Self::Tube => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Tube => "Tube",
        }
    }
}

pub(crate) fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .find(|binding| binding.id() == ParameterId(id))
}

pub(crate) fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS.get(index)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PARAMETER_BINDING_COUNT: usize = PARAMETER_BINDINGS.len();

pub(crate) fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_BINDINGS
        .iter()
        .position(|binding| binding.id() == ParameterId(id))
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) fn editor_parameter_bindings() -> impl Iterator<Item = &'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .filter(|binding| binding.editor().is_some())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn editor_parameter_binding(
    slot: EditorSurfaceSlot,
) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .find(|binding| binding.editor().is_some_and(|editor| editor.slot() == slot))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn editor_parameter_bindings_for_group(
    group: EditorSurfaceGroup,
) -> impl Iterator<Item = &'static ParameterBinding> {
    editor_parameter_bindings().filter(move |binding| {
        binding
            .editor()
            .is_some_and(|editor| editor.group() == group)
    })
}

pub(crate) fn patch_parameter_plain_value(patch: &ResonatorSynthPatch, id: u32) -> Option<f32> {
    parameter_binding(id).map(|binding| binding.plain_value(patch))
}

pub(crate) fn smoothed_runtime_parameter(
    id: u32,
    sample_rate: f32,
    initial_plain: f32,
) -> Option<SmoothedAtomicParam> {
    let binding = parameter_binding(id)?;
    let spec = binding.smoothed_atomic_spec()?;
    Some(SmoothedAtomicParam::with_initial_plain(
        spec,
        sample_rate,
        initial_plain,
    ))
}

pub(crate) fn apply_parameter_plain(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    value: f32,
) -> ParameterApplyKind {
    let Some(binding) = parameter_binding(id) else {
        return ParameterApplyKind::Ignored;
    };
    binding.apply_plain(patch, value)
}

pub(crate) fn apply_parameter_plain_for_controller(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    value: f32,
) -> bool {
    !matches!(
        apply_parameter_plain(patch, id, value),
        ParameterApplyKind::Ignored
    )
}

pub(crate) fn finite_value(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

fn output_gain_from_plain(gain_db: f32) -> f32 {
    db_to_gain(MASTER_GAIN_DB.clamp(gain_db))
}

fn modal_config_from(config: ResonatorConfig) -> ModalConfig {
    match config {
        ResonatorConfig::Modal(config) => config,
        ResonatorConfig::Waveguide(config) => ModalConfig {
            semitone_offset: config.semitone_offset,
            cent_offset: config.cent_offset,
            position_of_strike: config.position_of_strike,
            ..ModalConfig::default()
        },
    }
}

fn waveguide_config_from(config: ResonatorConfig) -> WaveguideConfig {
    match config {
        ResonatorConfig::Waveguide(config) => config,
        ResonatorConfig::Modal(config) => WaveguideConfig {
            semitone_offset: config.semitone_offset,
            cent_offset: config.cent_offset,
            position_of_strike: config.position_of_strike,
            ..WaveguideConfig::default()
        },
    }
}

fn parallel_mix_a(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_a, .. } => mix_a,
        ResonatorRouting::Series { mix_a, .. } => mix_a,
    }
}

fn parallel_mix_b(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_b, .. } => mix_b,
        ResonatorRouting::Series { mix_b, .. } => mix_b,
    }
}

fn set_parallel_mix(routing: ResonatorRouting, side: MixSide, value: f32) -> ResonatorRouting {
    let mut mix_a = parallel_mix_a(routing);
    let mut mix_b = parallel_mix_b(routing);
    match side {
        MixSide::A => mix_a = finite_value(value, 0.0, 1.0, 0.5),
        MixSide::B => mix_b = finite_value(value, 0.0, 1.0, 0.5),
    }
    match routing {
        ResonatorRouting::Parallel { .. } => ResonatorRouting::Parallel { mix_a, mix_b },
        ResonatorRouting::Series { .. } => ResonatorRouting::Series { mix_a, mix_b },
    }
}

fn bool_from_plain(value: f32) -> bool {
    finite_value(value, 0.0, 1.0, 0.0) >= 0.5
}

fn bool_plain(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn stepped_index(value: f32, max: u32) -> u32 {
    finite_value(value, 0.0, max as f32, 0.0).round() as u32
}

fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn filter_mode_label_from_plain(value: f32) -> &'static str {
    FilterMode::label_from_plain(value)
}

fn routing_label_from_plain(value: f32) -> &'static str {
    RoutingMode::label_from_plain(value)
}

fn resonator_model_label_from_plain(value: f32) -> &'static str {
    ResonatorModel::label_from_plain(value)
}

fn modal_preset_label_from_plain(value: f32) -> &'static str {
    ModalPreset::label_from_plain(value)
}

fn lfo_shape_label_from_plain(value: f32) -> &'static str {
    LfoShape::label_from_plain(value)
}

fn waveguide_style_label_from_plain(value: f32) -> &'static str {
    <WaveguideStyle as ParameterCodec>::label_from_plain(value)
}

fn format_modulation_source_label(value: f32) -> &'static str {
    ModulationSource::label_from_plain(value)
}

fn format_modulation_destination_label(value: f32) -> &'static str {
    ModulationDestination::label_from_plain(value)
}

fn retrigger_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) {
        "Retrigger"
    } else {
        "Carry"
    }
}

fn tempo_sync_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) {
        "Sync"
    } else {
        "Free"
    }
}

fn enabled_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) { "On" } else { "Off" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;

    #[test]
    fn exposed_parameters_have_exactly_one_binding() {
        assert_eq!(PARAMETERS.len(), PARAMETER_BINDINGS.len());

        for (index, parameter) in PARAMETERS.iter().enumerate() {
            let binding = parameter_binding(parameter.id.0).expect("missing binding");
            assert_eq!(binding.info(), *parameter);
            assert_eq!(parameter_binding_index(parameter.id.0), Some(index));
        }

        for (left, left_binding) in PARAMETER_BINDINGS.iter().enumerate() {
            for right_binding in PARAMETER_BINDINGS.iter().skip(left + 1) {
                assert_ne!(
                    left_binding.id(),
                    right_binding.id(),
                    "duplicate parameter id {}",
                    left_binding.id().0
                );
            }
        }
    }

    #[test]
    fn every_binding_round_trips_patch_get_set() {
        for binding in PARAMETER_BINDINGS {
            let mut patch = ResonatorSynthPatch::default();
            prepare_patch_for_binding(&mut patch, *binding);

            let value = non_default_probe_value(binding.info().range);
            binding.apply_plain(&mut patch, value);

            let actual = binding.plain_value(&patch);
            assert!(
                (actual - value).abs() < 0.001,
                "parameter {} ({}) round-tripped as {actual}, expected {value}",
                binding.id().0,
                binding.info().name
            );
        }
    }

    #[test]
    fn formatters_are_owned_by_bindings() {
        assert_eq!(parameter_binding(7).unwrap().format_plain_value(2.0), "HP");
        assert_eq!(
            parameter_binding(10).unwrap().format_plain_value(1.0),
            "Series"
        );
        assert_eq!(
            parameter_binding(13).unwrap().format_plain_value(1.0),
            "Retrigger"
        );
        assert_eq!(
            parameter_binding(20).unwrap().format_plain_value(1.0),
            "Waveguide"
        );
        assert_eq!(
            parameter_binding(35).unwrap().format_plain_value(1.0),
            "Tube"
        );
        assert_eq!(
            parameter_binding(81).unwrap().format_plain_value(5.0),
            "Brightness"
        );
        assert_eq!(
            parameter_binding(82).unwrap().format_plain_value(4.0),
            "Res B Position"
        );
    }

    #[test]
    fn live_smoothed_parameters_are_declared_in_registry() {
        for id in [
            MASTER_GAIN_PARAMETER_ID,
            FILTER_CUTOFF_PARAMETER_ID,
            SATURATION_PARAMETER_ID,
            MASTER_PAN_PARAMETER_ID,
            FILTER_RESONANCE_PARAMETER_ID,
            PARALLEL_MIX_A_PARAMETER_ID,
            PARALLEL_MIX_B_PARAMETER_ID,
        ] {
            let binding = parameter_binding(id).expect("missing parameter binding");
            assert!(
                binding.smoothed_atomic_spec().is_some(),
                "parameter {} ({}) should declare smoothing metadata",
                binding.id().0,
                binding.info().name
            );
        }

        assert!(
            parameter_binding(7)
                .unwrap()
                .smoothed_atomic_spec()
                .is_none()
        );
        assert!(
            parameter_binding(10)
                .unwrap()
                .smoothed_atomic_spec()
                .is_none()
        );
    }

    #[test]
    fn registry_smoothing_metadata_maps_master_gain_to_linear_gain() {
        let spec = parameter_binding(MASTER_GAIN_PARAMETER_ID)
            .unwrap()
            .smoothed_atomic_spec()
            .unwrap();

        assert_eq!(spec.info.id, ParameterId(MASTER_GAIN_PARAMETER_ID));
        assert_eq!(spec.smoothed_value(0.0), 1.0);
        assert!((spec.smoothed_value(-60.0) - MASTER_GAIN_LINEAR.min).abs() < 0.000_001);
        assert!((spec.smoothed_value(12.0) - MASTER_GAIN_LINEAR.max).abs() < 0.000_01);
    }

    #[test]
    fn smoothed_runtime_parameter_update_does_not_allocate() {
        let mut parameter =
            smoothed_runtime_parameter(FILTER_CUTOFF_PARAMETER_ID, 48_000.0, 20_000.0).unwrap();

        assert_no_allocations("smoothed runtime parameter update", || {
            parameter.atomic().store_normalized(0.25);
            assert!(parameter.sync_from_atomic());
            for _ in 0..8 {
                parameter.next_sample();
            }
        });
    }

    #[test]
    fn editor_bindings_are_single_source_metadata() {
        let mut count = 0;
        for binding in editor_parameter_bindings() {
            count += 1;
            assert_editor_binding_roundtrips(binding);
            assert_editor_metadata_valid(binding);
        }

        assert_eq!(count, EditorSurfaceSlot::ALL.len());
        assert_eq!(PARAMETER_BINDING_COUNT, PARAMETER_BINDINGS.len());
    }

    fn assert_editor_binding_roundtrips(binding: &ParameterBinding) {
        let editor = binding
            .editor()
            .expect("visible binding should have editor metadata");
        assert_eq!(
            editor_parameter_binding(editor.slot())
                .expect("surface slot should map back to a binding")
                .id(),
            binding.id()
        );
        assert_eq!(
            parameter_binding(binding.id().0)
                .expect("editor binding should be real")
                .id(),
            binding.id()
        );
    }

    fn assert_editor_metadata_valid(binding: &ParameterBinding) {
        let editor = binding
            .editor()
            .expect("visible binding should have editor metadata");
        assert!(!editor.label().is_empty());
        match editor.control() {
            EditorControlKind::Knob | EditorControlKind::Slider => {}
            EditorControlKind::Binary {
                left_label,
                right_label,
                width,
            } => {
                assert!(!left_label.is_empty());
                assert!(!right_label.is_empty());
                assert!(width > 0.0);
            }
        }
    }

    #[test]
    fn every_editor_surface_slot_resolves_to_a_binding() {
        for slot in EditorSurfaceSlot::ALL {
            let binding = editor_parameter_binding(slot)
                .unwrap_or_else(|| panic!("missing binding for editor slot {slot:?}"));
            assert_eq!(binding.editor().unwrap().slot(), slot);
        }
    }

    #[test]
    fn required_editor_surface_groups_have_visible_parameters() {
        for group in EditorSurfaceGroup::REQUIRED {
            assert!(
                editor_parameter_bindings_for_group(group).next().is_some(),
                "missing visible editor parameter for {group:?}",
            );
        }
    }

    #[test]
    fn editor_surface_group_orders_are_unique() {
        for group in EditorSurfaceGroup::REQUIRED {
            let mut orders = Vec::new();
            for binding in editor_parameter_bindings_for_group(group) {
                let order = binding.editor().unwrap().order();
                assert!(
                    !orders.contains(&order),
                    "duplicate editor order {order} in {group:?}",
                );
                orders.push(order);
            }
        }
    }

    #[test]
    fn enum_codecs_round_trip() {
        assert_codec_roundtrip(&[
            FilterMode::LowPass,
            FilterMode::BandPass,
            FilterMode::HighPass,
        ]);
        assert_codec_roundtrip(&[RoutingMode::Parallel, RoutingMode::Series]);
        assert_codec_roundtrip(&[ResonatorModel::Modal, ResonatorModel::Waveguide]);
        assert_codec_roundtrip(&[
            ModalPreset::Kalimba,
            ModalPreset::Marimba,
            ModalPreset::Bell,
            ModalPreset::GlassBowl,
            ModalPreset::MetalBar,
            ModalPreset::Woodblock,
            ModalPreset::GenericStrike,
        ]);
        assert_codec_roundtrip(&[
            LfoShape::Sine,
            LfoShape::Triangle,
            LfoShape::Saw,
            LfoShape::Square,
            LfoShape::SampleAndHold,
        ]);
        assert_codec_roundtrip(&[
            ModulationSource::SecondaryEnvelope,
            ModulationSource::Lfo,
            ModulationSource::Velocity,
            ModulationSource::Aftertouch,
            ModulationSource::ModWheel,
            ModulationSource::Brightness,
        ]);
        assert_codec_roundtrip(&[
            ModulationDestination::FilterCutoff,
            ModulationDestination::ResonatorADamping,
            ModulationDestination::ResonatorBDamping,
            ModulationDestination::ResonatorAPosition,
            ModulationDestination::ResonatorBPosition,
            ModulationDestination::ExcitationGain,
            ModulationDestination::LfoRate,
        ]);
        assert_codec_roundtrip(&[WaveguideStyle::String, WaveguideStyle::Tube]);
    }

    fn prepare_patch_for_binding(patch: &mut ResonatorSynthPatch, binding: ParameterBinding) {
        if let ParameterPath::Resonator { slot, parameter } = binding.path {
            match parameter {
                ResonatorParameter::Modal(_) => {
                    *slot.config_mut(patch) = ResonatorConfig::Modal(ModalConfig::default());
                }
                ResonatorParameter::Waveguide(_) => {
                    *slot.config_mut(patch) =
                        ResonatorConfig::Waveguide(WaveguideConfig::default());
                }
                ResonatorParameter::Model => {}
            }
        }
    }

    fn non_default_probe_value(range: ParameterRange) -> f32 {
        if (range.default - range.min).abs() > 0.001 {
            range.min
        } else {
            range.max
        }
    }

    fn assert_codec_roundtrip<T>(values: &[T])
    where
        T: ParameterCodec + std::fmt::Debug + PartialEq,
    {
        for (index, value) in values.iter().copied().enumerate() {
            assert_eq!(value.to_index(), index as u32);
            assert_eq!(T::from_plain(value.plain()), value);
            assert!(!value.label().is_empty());
        }
    }
}
