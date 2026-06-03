use lindelion_audio_expression::{
    DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
    DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
    DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT, DEFAULT_BRIGHTNESS_CEILING_HZ, DEFAULT_BRIGHTNESS_FLOOR_HZ,
    DEFAULT_PITCH_BEND_RANGE_SEMITONES, DEFAULT_PRESSURE_CEILING_RMS, DEFAULT_PRESSURE_FLOOR_RMS,
};
use lindelion_dsp_utils::db_to_gain;
use lindelion_plugin_shell::{ParameterCodec, ParameterPatchPath};

use crate::dsp::constants::{
    FILTER_RESONANCE, MASTER_GAIN_DB, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION,
};
use crate::{
    AudioInputMode, FilterMode, LiveExcitationMode, ModalConfig, ModalPreset, OutputConfig,
    ResonatorRouting, ResonatorSynthPatch, SurroundingConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterPath {
    Output(OutputParameter),
    RoutingMode,
    ParallelMixA,
    ParallelMixB,
    ParallelMixBalance,
    RetriggerResonators,
    AudioInputMode,
    AudioExpression(AudioExpressionParameter),
    NoteDetection(AudioNoteDetectionParameter),
    LiveExcitation(LiveExcitationParameter),
    Resonator {
        slot: ResonatorSlot,
        parameter: ModalParameter,
    },
    Surrounding(SurroundingParameter),
}

impl ParameterPatchPath<ResonatorSynthPatch> for ParameterPath {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        match self {
            Self::Output(parameter) => parameter.plain_value(patch.output),
            Self::RoutingMode => RoutingMode::from_routing(patch.routing).plain(),
            Self::ParallelMixA => parallel_mix_a(patch.routing),
            Self::ParallelMixB => parallel_mix_b(patch.routing),
            Self::ParallelMixBalance => parallel_mix_balance(patch.routing),
            Self::RetriggerResonators => bool_plain(patch.retrigger_resonators),
            Self::AudioInputMode => patch.audio_input.mode.plain(),
            Self::AudioExpression(parameter) => parameter.plain_value(patch),
            Self::NoteDetection(parameter) => parameter.plain_value(patch),
            Self::LiveExcitation(parameter) => parameter.plain_value(patch),
            Self::Resonator { slot, parameter } => parameter.plain_value(slot.config(patch)),
            Self::Surrounding(parameter) => parameter.plain_value(patch.surrounding),
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
            Self::ParallelMixBalance => {
                patch.routing = set_parallel_mix_balance(patch.routing, value)
            }
            Self::RetriggerResonators => patch.retrigger_resonators = bool_from_plain(value),
            Self::AudioInputMode => patch.audio_input.mode = AudioInputMode::from_plain(value),
            Self::AudioExpression(parameter) => parameter.apply_plain(patch, value),
            Self::NoteDetection(parameter) => parameter.apply_plain(patch, value),
            Self::LiveExcitation(parameter) => parameter.apply_plain(patch, value),
            Self::Resonator { slot, parameter } => {
                parameter.apply_plain(slot.config_mut(patch), value);
            }
            Self::Surrounding(parameter) => parameter.apply_plain(&mut patch.surrounding, value),
        }
        patch.normalize_routing();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputParameter {
    MasterGain,
    FilterCutoff,
    Saturation,
    Pan,
    FilterResonance,
    FilterMode,
}

impl OutputParameter {
    fn plain_value(self, output: OutputConfig) -> f32 {
        match self {
            Self::MasterGain => output.master_gain_db,
            Self::FilterCutoff => output.filter_cutoff,
            Self::Saturation => output.saturation_drive,
            Self::Pan => output.master_pan,
            Self::FilterResonance => output.filter_resonance,
            Self::FilterMode => output.filter_mode.plain(),
        }
    }

    fn apply_plain(self, output: &mut OutputConfig, value: f32) {
        match self {
            Self::MasterGain => output.master_gain_db = MASTER_GAIN_DB.clamp(value),
            Self::FilterCutoff => output.filter_cutoff = OUTPUT_FILTER_CUTOFF_HZ.clamp(value),
            Self::Saturation => output.saturation_drive = finite_value(value, 0.0, 1.0, 0.0),
            Self::Pan => output.master_pan = finite_value(value, -1.0, 1.0, 0.0),
            Self::FilterResonance => output.filter_resonance = FILTER_RESONANCE.clamp(value),
            Self::FilterMode => output.filter_mode = FilterMode::from_plain(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioExpressionParameter {
    Enabled,
    PitchBendRange,
    PressureFloor,
    PressureCeiling,
    BrightnessFloor,
    BrightnessCeiling,
}

impl AudioExpressionParameter {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        let mapping = patch.audio_expression.mapping;
        match self {
            Self::Enabled => bool_plain(patch.audio_expression.enabled),
            Self::PitchBendRange => mapping.pitch_bend_range_semitones,
            Self::PressureFloor => mapping.pressure_floor_rms,
            Self::PressureCeiling => mapping.pressure_ceiling_rms,
            Self::BrightnessFloor => mapping.brightness_floor_hz,
            Self::BrightnessCeiling => mapping.brightness_ceiling_hz,
        }
    }

    fn apply_plain(self, patch: &mut ResonatorSynthPatch, value: f32) {
        match self {
            Self::Enabled => patch.audio_expression.enabled = bool_from_plain(value),
            Self::PitchBendRange => {
                patch.audio_expression.mapping.pitch_bend_range_semitones =
                    finite_value(value, 0.0, 48.0, DEFAULT_PITCH_BEND_RANGE_SEMITONES);
            }
            Self::PressureFloor => {
                patch.audio_expression.mapping.pressure_floor_rms =
                    finite_value(value, 0.0, 1.0, DEFAULT_PRESSURE_FLOOR_RMS);
            }
            Self::PressureCeiling => {
                patch.audio_expression.mapping.pressure_ceiling_rms =
                    finite_value(value, 0.0, 1.0, DEFAULT_PRESSURE_CEILING_RMS);
            }
            Self::BrightnessFloor => {
                patch.audio_expression.mapping.brightness_floor_hz =
                    finite_value(value, 0.0, 48_000.0, DEFAULT_BRIGHTNESS_FLOOR_HZ);
            }
            Self::BrightnessCeiling => {
                patch.audio_expression.mapping.brightness_ceiling_hz =
                    finite_value(value, 0.0, 48_000.0, DEFAULT_BRIGHTNESS_CEILING_HZ);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioNoteDetectionParameter {
    OnsetSensitivity,
    ReleaseFloor,
    MinimumLength,
    PitchConfidence,
    VelocityAmount,
}

impl AudioNoteDetectionParameter {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        let detection = patch.note_detection;
        match self {
            Self::OnsetSensitivity => detection.onset_sensitivity,
            Self::ReleaseFloor => detection.note_release_floor_rms,
            Self::MinimumLength => detection.minimum_note_length_ms,
            Self::PitchConfidence => detection.pitch_confidence,
            Self::VelocityAmount => detection.velocity_amount,
        }
    }

    fn apply_plain(self, patch: &mut ResonatorSynthPatch, value: f32) {
        let detection = &mut patch.note_detection;
        match self {
            Self::OnsetSensitivity => {
                detection.onset_sensitivity =
                    finite_value(value, 0.0, 1.0, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY);
            }
            Self::ReleaseFloor => {
                detection.note_release_floor_rms =
                    finite_value(value, 0.0, 1.0, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS);
            }
            Self::MinimumLength => {
                detection.minimum_note_length_ms =
                    finite_value(value, 1.0, 2_000.0, DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS);
            }
            Self::PitchConfidence => {
                detection.pitch_confidence =
                    finite_value(value, 0.0, 1.0, DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE);
            }
            Self::VelocityAmount => {
                detection.velocity_amount =
                    finite_value(value, 0.0, 1.0, DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveExcitationParameter {
    Mode,
    Gain,
    LatchWindow,
    LatchPreRoll,
    LatchFade,
}

impl LiveExcitationParameter {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        let excitation = patch.live_excitation;
        match self {
            Self::Mode => excitation.mode.plain(),
            Self::Gain => excitation.gain_db,
            Self::LatchWindow => excitation.latch_window_ms,
            Self::LatchPreRoll => excitation.latch_pre_roll_ms,
            Self::LatchFade => excitation.latch_fade_ms,
        }
    }

    fn apply_plain(self, patch: &mut ResonatorSynthPatch, value: f32) {
        let excitation = &mut patch.live_excitation;
        match self {
            Self::Mode => excitation.mode = LiveExcitationMode::from_plain(value),
            Self::Gain => excitation.gain_db = finite_value(value, -60.0, 24.0, 0.0),
            Self::LatchWindow => {
                excitation.latch_window_ms = finite_value(value, 1.0, 2_000.0, 120.0);
            }
            Self::LatchPreRoll => {
                excitation.latch_pre_roll_ms = finite_value(value, 0.0, 500.0, 20.0);
            }
            Self::LatchFade => {
                excitation.latch_fade_ms = finite_value(value, 0.0, 250.0, 5.0);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResonatorSlot {
    A,
    B,
}

impl ResonatorSlot {
    fn config(self, patch: &ResonatorSynthPatch) -> ModalConfig {
        match self {
            Self::A => patch.resonator_a,
            Self::B => patch.resonator_b,
        }
    }

    fn config_mut(self, patch: &mut ResonatorSynthPatch) -> &mut ModalConfig {
        match self {
            Self::A => &mut patch.resonator_a,
            Self::B => &mut patch.resonator_b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalParameter {
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
            Self::StrikePosition => config.position_of_strike = STRIKE_POSITION.clamp(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurroundingParameter {
    MechanicalNoise,
    RadiationBrightness,
    Sympathetic,
}

impl SurroundingParameter {
    fn plain_value(self, config: SurroundingConfig) -> f32 {
        match self {
            Self::MechanicalNoise => config.mechanical_noise,
            Self::RadiationBrightness => config.radiation_brightness,
            Self::Sympathetic => config.sympathetic,
        }
    }

    fn apply_plain(self, config: &mut SurroundingConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.0);
        match self {
            Self::MechanicalNoise => config.mechanical_noise = value,
            Self::RadiationBrightness => config.radiation_brightness = value,
            Self::Sympathetic => config.sympathetic = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixSide {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingMode {
    Parallel,
    Series,
    BodyColor,
}

impl RoutingMode {
    fn from_routing(routing: ResonatorRouting) -> Self {
        match routing {
            ResonatorRouting::Parallel { .. } => Self::Parallel,
            ResonatorRouting::Series { .. } => Self::Series,
            ResonatorRouting::BodyColor { .. } => Self::BodyColor,
        }
    }

    fn apply_to(self, current: ResonatorRouting) -> ResonatorRouting {
        let mix_a = parallel_mix_a(current);
        let mix_b = parallel_mix_b(current);
        match self {
            Self::Parallel => ResonatorRouting::Parallel { mix_a, mix_b },
            Self::Series => ResonatorRouting::Series { mix_a, mix_b },
            Self::BodyColor => ResonatorRouting::BodyColor { mix_a, mix_b },
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for AudioInputMode {
        max: 2;
        fallback: Self::Off;
        0 => Self::Off, "Off";
        1 => Self::AudioCreatesNotes, "Audio Notes";
        2 => Self::MidiPlusAudioCreatesNotes, "MIDI + Audio";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for LiveExcitationMode {
        max: 3;
        fallback: Self::Off;
        0 => Self::Off, "Off";
        1 => Self::Continuous, "Continuous";
        2 => Self::NoteLatched, "Note Latched";
        3 => Self::ContinuousAndNoteLatched, "Cont + Latch";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for FilterMode {
        max: 2;
        fallback: Self::LowPass;
        0 => Self::LowPass, "LP";
        1 => Self::BandPass, "BP";
        2 => Self::HighPass, "HP";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for RoutingMode {
        max: 2;
        fallback: Self::Parallel;
        0 => Self::Parallel, "Parallel";
        1 => Self::Series, "Series";
        2 => Self::BodyColor, "Body Color";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ModalPreset {
        max: 6;
        fallback: Self::GenericStrike;
        0 => Self::Kalimba, "Kalimba";
        1 => Self::Marimba, "Marimba";
        2 => Self::Bell, "Bell";
        3 => Self::GlassBowl, "Glass Bowl";
        4 => Self::MetalBar, "Metal Bar";
        5 => Self::Woodblock, "Woodblock";
        6 => Self::GenericStrike, "Generic";
    }
}

pub(crate) fn output_gain_from_plain(gain_db: f32) -> f32 {
    db_to_gain(MASTER_GAIN_DB.clamp(gain_db))
}

fn parallel_mix_a(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_a, .. } => mix_a,
        ResonatorRouting::Series { mix_a, .. } => mix_a,
        ResonatorRouting::BodyColor { mix_a, .. } => mix_a,
    }
}

fn parallel_mix_b(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_b, .. } => mix_b,
        ResonatorRouting::Series { mix_b, .. } => mix_b,
        ResonatorRouting::BodyColor { mix_b, .. } => mix_b,
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
        ResonatorRouting::BodyColor { .. } => ResonatorRouting::BodyColor { mix_a, mix_b },
    }
}

fn parallel_mix_balance(routing: ResonatorRouting) -> f32 {
    let mix_a = finite_value(parallel_mix_a(routing), 0.0, 1.0, 0.5);
    let mix_b = finite_value(parallel_mix_b(routing), 0.0, 1.0, 0.5);
    let total = mix_a + mix_b;
    if total > f32::EPSILON {
        mix_b / total
    } else {
        0.5
    }
}

fn set_parallel_mix_balance(routing: ResonatorRouting, value: f32) -> ResonatorRouting {
    let mix_b = finite_value(value, 0.0, 1.0, 0.5);
    let mix_a = 1.0 - mix_b;
    match routing {
        ResonatorRouting::Parallel { .. } => ResonatorRouting::Parallel { mix_a, mix_b },
        ResonatorRouting::Series { .. } => ResonatorRouting::Series { mix_a, mix_b },
        ResonatorRouting::BodyColor { .. } => ResonatorRouting::BodyColor { mix_a, mix_b },
    }
}

fn bool_from_plain(value: f32) -> bool {
    finite_value(value, 0.0, 1.0, 0.0) >= 0.5
}

fn bool_plain(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

pub(crate) fn audio_input_mode_label_from_plain(value: f32) -> &'static str {
    AudioInputMode::label_from_plain(value)
}

pub(crate) fn live_excitation_mode_label_from_plain(value: f32) -> &'static str {
    LiveExcitationMode::label_from_plain(value)
}

pub(crate) fn filter_mode_label_from_plain(value: f32) -> &'static str {
    FilterMode::label_from_plain(value)
}

pub(crate) fn routing_label_from_plain(value: f32) -> &'static str {
    RoutingMode::label_from_plain(value)
}

pub(crate) fn modal_preset_label_from_plain(value: f32) -> &'static str {
    ModalPreset::label_from_plain(value)
}

pub(crate) fn retrigger_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) {
        "Retrigger"
    } else {
        "Carry"
    }
}

pub(crate) fn enabled_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) { "On" } else { "Off" }
}

fn finite_value(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}
