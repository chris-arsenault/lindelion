use lindelion_audio_expression::{
    DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
    DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
    DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT, DEFAULT_BRIGHTNESS_CEILING_HZ, DEFAULT_BRIGHTNESS_FLOOR_HZ,
    DEFAULT_PITCH_BEND_RANGE_SEMITONES, DEFAULT_PRESSURE_CEILING_RMS, DEFAULT_PRESSURE_FLOOR_RMS,
};
mod editor;
mod paths;

use lindelion_dsp_utils::{params::StructuralChangePolicy, smoothing::SmoothedParamSpec};
pub(crate) use lindelion_plugin_shell::ParameterCodec;
use lindelion_plugin_shell::{
    ParameterApplyDispatcher, ParameterBinding as RegistryParameterBinding, ParameterFormatter,
    ParameterInfo, ParameterRange, ParameterRegistry, ParameterSmoothingSpec, SmoothedAtomicParam,
    SmoothedAtomicParamSpec,
};

use crate::dsp::constants::{
    FILTER_RESONANCE, MASTER_GAIN_DB, MASTER_GAIN_LINEAR, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION,
};
use crate::{ModalPreset, ResonatorSynthPatch};
use editor::{EditorParameterBinding, EditorSurfaceSlot};
use paths::{
    AudioExpressionParameter, AudioNoteDetectionParameter, LiveExcitationParameter, ModalParameter,
    OutputParameter, ParameterPath, ResonatorSlot, SurroundingParameter,
    audio_input_mode_label_from_plain, enabled_label_from_plain, filter_mode_label_from_plain,
    live_excitation_mode_label_from_plain, modal_preset_label_from_plain, output_gain_from_plain,
    retrigger_label_from_plain, routing_label_from_plain,
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

const AUDIO_INPUT_MODE_EDITOR_LABELS: &[&str] = &["Off", "Audio Notes", "MIDI + Audio"];
const LIVE_EXCITATION_MODE_EDITOR_LABELS: &[&str] = &["Off", "Cont", "Latch", "Both"];
const ROUTING_MODE_EDITOR_LABELS: &[&str] = &["Parallel", "Series", "Body"];

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
pub(crate) const SURROUNDING_MECHANICAL_NOISE_PARAMETER_ID: u32 = 150;
pub(crate) const SURROUNDING_RADIATION_BRIGHTNESS_PARAMETER_ID: u32 = 151;
pub(crate) const SURROUNDING_SYMPATHETIC_PARAMETER_ID: u32 = 152;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterApplyKind {
    Live,
    Structural(StructuralChangePolicy),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParameterTarget {
    None,
    Patch,
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

    fn spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
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

impl ParameterSmoothingSpec for RuntimeSmoothing {
    fn smoothed_atomic_spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
        self.spec(info)
    }
}

lindelion_plugin_shell::define_parameter_bindings! {
    binding: ParameterBinding;
    parameters: pub const PARAMETERS;
    bindings: pub(crate) const PARAMETER_BINDINGS;
    defaults {
        runtime: RuntimeParameterTarget::None,
        smoothing: None::<RuntimeSmoothing>,
    }

    ParameterInfo::continuous(MASTER_GAIN_PARAMETER_ID, "Master Gain", "dB", parameter_range!(MASTER_GAIN_DB)) => { path: ParameterPath::Output(OutputParameter::MasterGain), apply: LIVE, runtime: RuntimeParameterTarget::Output, smoothing: RuntimeSmoothing::mapped(SmoothedParamSpec::new(MASTER_GAIN_LINEAR.min, MASTER_GAIN_LINEAR.max, MASTER_GAIN_LINEAR.default, RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), output_gain_from_plain), format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Master, "Master")), },
    ParameterInfo::continuous(FILTER_CUTOFF_PARAMETER_ID, "Filter Cutoff", "Hz", parameter_range!(OUTPUT_FILTER_CUTOFF_HZ)) => { path: ParameterPath::Output(OutputParameter::FilterCutoff), apply: LIVE, runtime: RuntimeParameterTarget::Output, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, FILTER_CUTOFF_EPSILON), format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Cutoff, "Cutoff")), },
    ParameterInfo::continuous(SATURATION_PARAMETER_ID, "Saturation", "", ParameterRange::linear(0.0, 1.0, 0.0)) => { path: ParameterPath::Output(OutputParameter::Saturation), apply: LIVE, runtime: RuntimeParameterTarget::Output, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Saturation, "Saturate")), },
    ParameterInfo::continuous(MASTER_PAN_PARAMETER_ID, "Master Pan", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => { path: ParameterPath::Output(OutputParameter::Pan), apply: LIVE, runtime: RuntimeParameterTarget::Output, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::Pan, "Pan")), },
    ParameterInfo::continuous(FILTER_RESONANCE_PARAMETER_ID, "Filter Resonance", "", parameter_range!(FILTER_RESONANCE)) => { path: ParameterPath::Output(OutputParameter::FilterResonance), apply: LIVE, runtime: RuntimeParameterTarget::Output, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::Resonance, "Res")), },
    ParameterInfo::stepped(7, "Filter Mode", "", ParameterRange::linear(0.0, 2.0, 0.0), 2) => { path: ParameterPath::Output(OutputParameter::FilterMode), apply: LIVE_MUTE_RAMP, runtime: RuntimeParameterTarget::Output, format: ParameterFormatter::Label(filter_mode_label_from_plain), editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::FilterMode, "Mode")), },
    ParameterInfo::stepped(10, "Routing", "", ParameterRange::linear(0.0, 2.0, 0.0), 2) => { path: ParameterPath::RoutingMode, apply: LIVE_MUTE_RAMP, runtime: RuntimeParameterTarget::Routing, format: ParameterFormatter::Label(routing_label_from_plain), editor: Some(EditorParameterBinding::segmented(EditorSurfaceSlot::Routing, "Routing", ROUTING_MODE_EDITOR_LABELS, 184.0)), },
    ParameterInfo::continuous(PARALLEL_MIX_A_PARAMETER_ID, "Parallel Mix A", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::ParallelMixA, apply: LIVE, runtime: RuntimeParameterTarget::Routing, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(PARALLEL_MIX_B_PARAMETER_ID, "Parallel Mix B", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::ParallelMixB, apply: LIVE, runtime: RuntimeParameterTarget::Routing, smoothing: RuntimeSmoothing::identity(RUNTIME_PARAMETER_SMOOTH_MS, RUNTIME_PARAMETER_EPSILON), format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::stepped(13, "Retrigger Resonators", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => { path: ParameterPath::RetriggerResonators, apply: NOTE_BOUNDARY, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Label(retrigger_label_from_plain), editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::RetriggerResonators, "Retrigger", "Carry", "Retrig", 144.0)), },
    ParameterInfo::continuous(RESONATOR_MIX_PARAMETER_ID, "Resonator Mix", "", ParameterRange::linear(0.0, 1.0, 0.0)) => { path: ParameterPath::ParallelMixBalance, apply: LIVE, runtime: RuntimeParameterTarget::Routing, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::knob(EditorSurfaceSlot::ResonatorMix, "Mix")), },

    ParameterInfo::stepped(21, "Resonator A Modal Preset", "", ParameterRange::linear(0.0, 6.0, 1.0), 6) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Preset }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Label(modal_preset_label_from_plain), editor: Some(EditorParameterBinding::selector(EditorSurfaceSlot::ResonatorAPreset, "A Preset", <ModalPreset as ParameterCodec>::LABELS, 120.0)), },
    ParameterInfo::stepped(22, "Resonator A Mode Count", "", ParameterRange::linear(16.0, 256.0, 64.0), 240) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::ModeCount }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::stepped(23, "Resonator A Semitone", "st", ParameterRange::linear(-24.0, 24.0, 0.0), 48) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Semitone }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(24, "Resonator A Cents", "ct", ParameterRange::linear(-100.0, 100.0, 0.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Cents }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(25, "Resonator A Inharmonicity", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Inharmonicity }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(26, "Resonator A Brightness", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Brightness }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorABrightness, "A Bright")), },
    ParameterInfo::continuous(27, "Resonator A Decay", "s", ParameterRange::linear(0.05, 10.0, 1.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::Decay }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorADecay, "A Decay")), },
    ParameterInfo::continuous(28, "Resonator A Decay Tilt", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::DecayTilt }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(29, "Resonator A Strike Position", "", parameter_range!(STRIKE_POSITION)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::A, parameter: ModalParameter::StrikePosition }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },

    ParameterInfo::stepped(41, "Resonator B Modal Preset", "", ParameterRange::linear(0.0, 6.0, 2.0), 6) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Preset }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Label(modal_preset_label_from_plain), editor: Some(EditorParameterBinding::selector(EditorSurfaceSlot::ResonatorBPreset, "B Preset", <ModalPreset as ParameterCodec>::LABELS, 120.0)), },
    ParameterInfo::stepped(42, "Resonator B Mode Count", "", ParameterRange::linear(16.0, 256.0, 64.0), 240) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::ModeCount }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::stepped(43, "Resonator B Semitone", "st", ParameterRange::linear(-24.0, 24.0, 0.0), 48) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Semitone }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(44, "Resonator B Cents", "ct", ParameterRange::linear(-100.0, 100.0, 0.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Cents }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(45, "Resonator B Inharmonicity", "", ParameterRange::linear(-1.0, 1.0, 0.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Inharmonicity }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(46, "Resonator B Brightness", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Brightness }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBBrightness, "B Bright")), },
    ParameterInfo::continuous(47, "Resonator B Decay", "s", ParameterRange::linear(0.05, 10.0, 1.0)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::Decay }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::ResonatorBDecay, "B Decay")), },
    ParameterInfo::continuous(48, "Resonator B Decay Tilt", "", ParameterRange::linear(0.0, 1.0, 0.5)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::DecayTilt }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(49, "Resonator B Strike Position", "", parameter_range!(STRIKE_POSITION)) => { path: ParameterPath::Resonator { slot: ResonatorSlot::B, parameter: ModalParameter::StrikePosition }, apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },

    ParameterInfo::stepped(AUDIO_INPUT_MODE_PARAMETER_ID, "Audio Input Mode", "", ParameterRange::linear(0.0, 2.0, 0.0), 2) => { path: ParameterPath::AudioInputMode, apply: NOTE_BOUNDARY, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Label(audio_input_mode_label_from_plain), editor: Some(EditorParameterBinding::segmented(EditorSurfaceSlot::AudioInputMode, "Input", AUDIO_INPUT_MODE_EDITOR_LABELS, 184.0)), },
    ParameterInfo::stepped(AUDIO_EXPRESSION_ENABLE_PARAMETER_ID, "Audio Expression", "", ParameterRange::linear(0.0, 1.0, 0.0), 1) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::Enabled), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Label(enabled_label_from_plain), editor: Some(EditorParameterBinding::binary(EditorSurfaceSlot::AudioExpressionEnable, "Expr", "Off", "On", 96.0)), },
    ParameterInfo::continuous(AUDIO_EXPRESSION_PITCH_RANGE_PARAMETER_ID, "Audio Expression Pitch Range", "st", ParameterRange::linear(0.0, 48.0, DEFAULT_PITCH_BEND_RANGE_SEMITONES)) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::PitchBendRange), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioExpressionPitchRange, "Bend")), },
    ParameterInfo::continuous(AUDIO_EXPRESSION_PRESSURE_FLOOR_PARAMETER_ID, "Audio Expression Pressure Floor", "rms", ParameterRange::linear(0.0, 1.0, DEFAULT_PRESSURE_FLOOR_RMS)) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::PressureFloor), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioExpressionPressureFloor, "P Floor")), },
    ParameterInfo::continuous(AUDIO_EXPRESSION_PRESSURE_CEILING_PARAMETER_ID, "Audio Expression Pressure Ceiling", "rms", ParameterRange::linear(0.0, 1.0, DEFAULT_PRESSURE_CEILING_RMS)) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::PressureCeiling), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioExpressionPressureCeiling, "P Ceiling")), },
    ParameterInfo::continuous(AUDIO_EXPRESSION_BRIGHTNESS_FLOOR_PARAMETER_ID, "Audio Expression Brightness Floor", "Hz", ParameterRange::linear(0.0, 48_000.0, DEFAULT_BRIGHTNESS_FLOOR_HZ)) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::BrightnessFloor), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioExpressionBrightnessFloor, "B Floor")), },
    ParameterInfo::continuous(AUDIO_EXPRESSION_BRIGHTNESS_CEILING_PARAMETER_ID, "Audio Expression Brightness Ceiling", "Hz", ParameterRange::linear(0.0, 48_000.0, DEFAULT_BRIGHTNESS_CEILING_HZ)) => { path: ParameterPath::AudioExpression(AudioExpressionParameter::BrightnessCeiling), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioExpressionBrightnessCeiling, "B Ceiling")), },
    ParameterInfo::continuous(AUDIO_NOTE_ONSET_SENSITIVITY_PARAMETER_ID, "Audio Note Onset Sensitivity", "", ParameterRange::linear(0.0, 1.0, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY)) => { path: ParameterPath::NoteDetection(AudioNoteDetectionParameter::OnsetSensitivity), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioNoteOnsetSensitivity, "Onset")), },
    ParameterInfo::continuous(AUDIO_NOTE_RELEASE_FLOOR_PARAMETER_ID, "Audio Note Release Floor", "rms", ParameterRange::linear(0.0, 1.0, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS)) => { path: ParameterPath::NoteDetection(AudioNoteDetectionParameter::ReleaseFloor), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioNoteReleaseFloor, "Release")), },
    ParameterInfo::continuous(AUDIO_NOTE_MIN_LENGTH_PARAMETER_ID, "Audio Note Minimum Length", "ms", ParameterRange::linear(1.0, 2_000.0, DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS)) => { path: ParameterPath::NoteDetection(AudioNoteDetectionParameter::MinimumLength), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioNoteMinimumLength, "Min")), },
    ParameterInfo::continuous(AUDIO_NOTE_PITCH_CONFIDENCE_PARAMETER_ID, "Audio Note Pitch Confidence", "", ParameterRange::linear(0.0, 1.0, DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE)) => { path: ParameterPath::NoteDetection(AudioNoteDetectionParameter::PitchConfidence), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioNotePitchConfidence, "Conf")), },
    ParameterInfo::continuous(AUDIO_NOTE_VELOCITY_AMOUNT_PARAMETER_ID, "Audio Note Velocity Amount", "", ParameterRange::linear(0.0, 1.0, DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT)) => { path: ParameterPath::NoteDetection(AudioNoteDetectionParameter::VelocityAmount), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::AudioNoteVelocityAmount, "Vel")), },
    ParameterInfo::stepped(LIVE_EXCITATION_MODE_PARAMETER_ID, "Live Excitation Mode", "", ParameterRange::linear(0.0, 3.0, 0.0), 3) => { path: ParameterPath::LiveExcitation(LiveExcitationParameter::Mode), apply: NOTE_BOUNDARY, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Label(live_excitation_mode_label_from_plain), editor: Some(EditorParameterBinding::segmented(EditorSurfaceSlot::LiveExcitationMode, "Excite", LIVE_EXCITATION_MODE_EDITOR_LABELS, 184.0)), },
    ParameterInfo::continuous(LIVE_EXCITATION_GAIN_PARAMETER_ID, "Live Excitation Gain", "dB", ParameterRange::linear(-60.0, 24.0, 0.0)) => { path: ParameterPath::LiveExcitation(LiveExcitationParameter::Gain), apply: LIVE, runtime: RuntimeParameterTarget::Patch, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LiveExcitationGain, "Gain")), },
    ParameterInfo::continuous(LIVE_EXCITATION_LATCH_WINDOW_PARAMETER_ID, "Live Excitation Latch Window", "ms", ParameterRange::linear(1.0, 2_000.0, 120.0)) => { path: ParameterPath::LiveExcitation(LiveExcitationParameter::LatchWindow), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LiveExcitationLatchWindow, "Window")), },
    ParameterInfo::continuous(LIVE_EXCITATION_LATCH_PRE_ROLL_PARAMETER_ID, "Live Excitation Latch Pre-roll", "ms", ParameterRange::linear(0.0, 500.0, 20.0)) => { path: ParameterPath::LiveExcitation(LiveExcitationParameter::LatchPreRoll), apply: RESET_STATE, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LiveExcitationLatchPreRoll, "Pre-roll")), },
    ParameterInfo::continuous(LIVE_EXCITATION_LATCH_FADE_PARAMETER_ID, "Live Excitation Latch Fade", "ms", ParameterRange::linear(0.0, 250.0, 5.0)) => { path: ParameterPath::LiveExcitation(LiveExcitationParameter::LatchFade), apply: NOTE_BOUNDARY, runtime: RuntimeParameterTarget::None, format: ParameterFormatter::Plain, editor: Some(EditorParameterBinding::slider(EditorSurfaceSlot::LiveExcitationLatchFade, "Fade")), },

    ParameterInfo::continuous(SURROUNDING_MECHANICAL_NOISE_PARAMETER_ID, "Surrounding Mechanical Noise", "", ParameterRange::linear(0.0, 1.0, 0.0)) => { path: ParameterPath::Surrounding(SurroundingParameter::MechanicalNoise), apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(SURROUNDING_RADIATION_BRIGHTNESS_PARAMETER_ID, "Surrounding Radiation Brightness", "", ParameterRange::linear(0.0, 1.0, 0.0)) => { path: ParameterPath::Surrounding(SurroundingParameter::RadiationBrightness), apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
    ParameterInfo::continuous(SURROUNDING_SYMPATHETIC_PARAMETER_ID, "Surrounding Sympathetic", "", ParameterRange::linear(0.0, 1.0, 0.0)) => { path: ParameterPath::Surrounding(SurroundingParameter::Sympathetic), apply: NOTE_BOUNDARY, format: ParameterFormatter::Plain, editor: None, },
}

pub(crate) const PARAMETER_REGISTRY: ParameterRegistry<ParameterBinding> =
    ParameterRegistry::new(PARAMETER_BINDINGS);

pub(crate) fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding(id)
}

pub(crate) fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding_by_index(index)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PARAMETER_BINDING_COUNT: usize = PARAMETER_REGISTRY.len();

pub(crate) fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_REGISTRY.binding_index(id)
}

pub(crate) fn parameter_info(id: u32) -> Option<ParameterInfo> {
    PARAMETER_REGISTRY.info(id)
}

pub(crate) fn normalized_parameter_value(id: u32, plain: f32) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_value(id, plain)
}

pub(crate) fn dispatch_parameter_normalized<Dispatcher>(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    normalized: f32,
    dispatcher: &mut Dispatcher,
) -> ParameterApplyKind
where
    Dispatcher:
        ParameterApplyDispatcher<ResonatorSynthPatch, ParameterApplyKind, RuntimeParameterTarget>,
{
    PARAMETER_REGISTRY
        .dispatch_normalized(patch, id, normalized, dispatcher)
        .map(|outcome| outcome.apply_kind)
        .unwrap_or(ParameterApplyKind::Ignored)
}

pub(crate) fn apply_parameter_normalized_for_controller(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    normalized: f32,
) -> bool {
    PARAMETER_REGISTRY
        .apply_normalized(patch, id, normalized)
        .is_some_and(|outcome| !matches!(outcome.apply_kind, ParameterApplyKind::Ignored))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn resonator_editor_parameter_bindings()
-> impl Iterator<Item = lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding> {
    PARAMETER_REGISTRY.projected_editor_bindings()
}

pub(crate) fn format_parameter_plain_value(id: u32, value: f32) -> String {
    PARAMETER_REGISTRY.formatted_plain_value(id, value)
}

pub(crate) fn smoothed_runtime_parameter(
    id: u32,
    sample_rate: f32,
    initial_plain: f32,
) -> Option<SmoothedAtomicParam> {
    PARAMETER_REGISTRY.smoothed_atomic_param(id, sample_rate, initial_plain)
}
