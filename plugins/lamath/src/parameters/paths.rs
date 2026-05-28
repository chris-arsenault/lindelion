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

impl ParameterPatchPath<ResonatorSynthPatch> for ParameterPath {
    fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        match self {
            Self::Output(parameter) => parameter.plain_value(patch.output),
            Self::RoutingMode => RoutingMode::from_routing(crate::normalize_routing_for_resonator_models(
                patch.routing,
                patch.resonator_a,
                patch.resonator_b,
            ))
            .plain(),
            Self::ParallelMixA => parallel_mix_a(patch.routing),
            Self::ParallelMixB => parallel_mix_b(patch.routing),
            Self::ParallelMixBalance => parallel_mix_balance(patch.routing),
            Self::RetriggerResonators => bool_plain(patch.retrigger_resonators),
            Self::AudioInputMode => patch.audio_input.mode.plain(),
            Self::AudioExpression(parameter) => parameter.plain_value(patch),
            Self::NoteDetection(parameter) => parameter.plain_value(patch),
            Self::LiveExcitation(parameter) => parameter.plain_value(patch),
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
                patch.modulation.pitch_bend_range_semitones = finite_value(
                    value,
                    0.0,
                    24.0,
                    DEFAULT_PITCH_BEND_RANGE_SEMITONES,
                );
            }
            Self::VelocityExcitationDepth => {
                patch.modulation.velocity_to_excitation_depth = finite_value(value, 0.0, 1.0, 1.0);
            }
            Self::ModulationSlot { slot, parameter } => {
                parameter.apply_plain(&mut patch.modulation, slot, value);
            }
        }
        patch.normalize_routing_for_resonator_models();
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
                patch.audio_expression.mapping.pitch_bend_range_semitones = finite_value(
                    value,
                    0.0,
                    48.0,
                    DEFAULT_PITCH_BEND_RANGE_SEMITONES,
                );
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
                detection.onset_sensitivity = finite_value(
                    value,
                    0.0,
                    1.0,
                    DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
                );
            }
            Self::ReleaseFloor => {
                detection.note_release_floor_rms = finite_value(
                    value,
                    0.0,
                    1.0,
                    DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
                );
            }
            Self::MinimumLength => {
                detection.minimum_note_length_ms = finite_value(
                    value,
                    1.0,
                    2_000.0,
                    DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS,
                );
            }
            Self::PitchConfidence => {
                detection.pitch_confidence = finite_value(
                    value,
                    0.0,
                    1.0,
                    DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE,
                );
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
pub(crate) enum ResonatorParameter {
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
pub(crate) enum WaveguideParameter {
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
pub(crate) enum EnvelopeTarget {
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
pub(crate) enum EnvelopeParameter {
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
pub(crate) enum ModulationSlotParameter {
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
