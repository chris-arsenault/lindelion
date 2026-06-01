use lindelion_dsp_utils::{
    envelope::{Adsr, AdsrState, EnvelopePhase},
    math::finite_clamp,
    smoothing::{SmoothedParam, SmoothedParamSpec},
};

use crate::{
    ModulationConfig, ModulationDestination, ModulationSource, dsp::constants::WAVEGUIDE_LOOP_GAIN,
};

use super::{PARAMETER_EPSILON, PARAMETER_SMOOTH_MS, VoiceExpression};
use crate::dsp::energy_follower::EnergyFollower;

const PITCH_BEND_SMOOTH_MS: f32 = 8.0;
const PITCH_BEND_EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy)]
pub(super) struct ModulationSources {
    pub(super) amp_envelope: f32,
    pub(super) secondary_envelope: f32,
    pub(super) lfo: f32,
    pub(super) velocity: f32,
    pub(super) aftertouch: f32,
    pub(super) mod_wheel: f32,
    pub(super) brightness: f32,
    /// Hybrid effort/energy bus (ADR-0014), fixed internal wiring: consumed
    /// directly by the nonlinear stages, not user-routable. `effort` is player
    /// effort from `ExpressionStream` (drives the M8 physical driver); `energy` is
    /// the measured resonator-energy follower (drives M4–M6 nonlinearities).
    pub(super) effort: f32,
    pub(super) energy: f32,
    /// Note-state drive gate (M11 P3): 1 while the note is held, releasing to 0 over
    /// a short time after note-off. A self-sustaining driver (reed/bow) scales its
    /// drive by this so note-off lets the resonator ring out at its natural decay
    /// rather than being held by continued driving.
    pub(super) drive_gate: f32,
}

/// Drive-gate release time after note-off (M11 P3). Short, so the bow/breath force
/// lets go quickly and the *resonator's* own ring-out is the tail — not a slow
/// fade of the driver.
const DRIVE_GATE_RELEASE_SECONDS: f32 = 0.03;

fn drive_gate_release_step(sample_rate: f32) -> f32 {
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    1.0 / (DRIVE_GATE_RELEASE_SECONDS * sample_rate)
}

#[derive(Debug)]
pub(super) struct ModulationState {
    pub(super) config: ModulationConfig,
    pub(super) resonator_modulation_active: bool,
    pub(super) expression: VoiceExpression,
    pub(super) velocity: f32,
    pub(super) aftertouch: f32,
    pub(super) mod_wheel: f32,
    pub(super) brightness: f32,
    pub(super) amp_envelope: Adsr,
    pub(super) amp_state: AdsrState,
    pub(super) secondary_envelope: Adsr,
    pub(super) secondary_state: AdsrState,
    pub(super) lfo_phase: f32,
    pub(super) lfo_hold: f32,
    pub(super) pitch_bend_semitones: SmoothedParam,
    pub(super) applied_pitch_bend_semitones: f32,
    pub(super) waveguide_loop_gain: SmoothedParam,
    pub(super) applied_waveguide_loop_gain: f32,
    /// Measured-energy half of the effort/energy bus (ADR-0014): a per-sample
    /// follower fed the resonator output, read into the next sample's sources.
    energy_follower: EnergyFollower,
    energy: f32,
    /// Note-state drive gate envelope (1 held → 0 after note-off), read into
    /// `ModulationSources::drive_gate`.
    drive_gate: f32,
}

impl ModulationState {
    pub(super) fn new(sample_rate: f32) -> Self {
        let config = ModulationConfig::default();
        Self {
            config,
            resonator_modulation_active: false,
            expression: VoiceExpression::default(),
            velocity: 0.0,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
            amp_envelope: config.amp_envelope.into(),
            amp_state: AdsrState::default(),
            secondary_envelope: config.secondary_envelope.into(),
            secondary_state: AdsrState::default(),
            lfo_phase: 0.0,
            lfo_hold: 0.0,
            pitch_bend_semitones: pitch_bend_param(sample_rate, 0.0),
            applied_pitch_bend_semitones: 0.0,
            waveguide_loop_gain: waveguide_loop_gain_param(
                sample_rate,
                WAVEGUIDE_LOOP_GAIN.default,
            ),
            applied_waveguide_loop_gain: WAVEGUIDE_LOOP_GAIN.default,
            energy_follower: EnergyFollower::new(sample_rate),
            energy: 0.0,
            drive_gate: 1.0,
        }
    }

    pub(super) fn trigger(
        &mut self,
        midi_note: u8,
        expression: VoiceExpression,
        config: ModulationConfig,
        pitch_bend: f32,
    ) {
        let expression = expression.sanitized();
        self.config = config;
        self.resonator_modulation_active = modulation_targets_resonators(config);
        self.expression = expression;
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.amp_envelope = config.amp_envelope.into();
        self.secondary_envelope = config.secondary_envelope.into();
        self.amp_state.reset();
        self.amp_state.note_on();
        self.secondary_state.reset();
        self.secondary_state.note_on();
        self.lfo_phase = 0.0;
        self.lfo_hold = sample_and_hold_value(midi_note);
        self.pitch_bend_semitones.reset(pitch_bend);
        self.applied_pitch_bend_semitones = pitch_bend;
        self.energy_follower.reset();
        self.energy = 0.0;
        self.drive_gate = 1.0;
    }

    pub(super) fn clear(&mut self, loop_gain: f32) {
        self.expression = VoiceExpression::default();
        self.velocity = 0.0;
        self.aftertouch = 0.0;
        self.mod_wheel = 0.0;
        self.brightness = 0.0;
        self.resonator_modulation_active = false;
        self.amp_state.reset();
        self.secondary_state.reset();
        self.pitch_bend_semitones.reset(0.0);
        self.applied_pitch_bend_semitones = 0.0;
        self.reset_waveguide_loop_gain(loop_gain);
        self.energy_follower.reset();
        self.energy = 0.0;
        self.drive_gate = 1.0;
    }

    /// Fold the resonator output into the measured-energy follower. Called by the
    /// voice loop after `process_sample`; the followed RMS is read into the next
    /// sample's `ModulationSources::energy`.
    pub(super) fn observe_energy(&mut self, sample: f32) {
        self.energy = self.energy_follower.observe(sample);
    }

    pub(super) fn static_sources(&self) -> ModulationSources {
        ModulationSources {
            amp_envelope: 1.0,
            secondary_envelope: 0.0,
            lfo: 0.0,
            velocity: self.velocity,
            aftertouch: self.aftertouch,
            mod_wheel: self.mod_wheel,
            brightness: self.brightness,
            effort: self.effort(),
            energy: self.energy,
            drive_gate: 1.0,
        }
    }

    #[cfg(test)]
    pub(super) fn set_pitch_bend(&mut self, pitch_bend_semitones: f32) {
        let mut expression = self.expression;
        expression.stream.pitch_bend = pitch_bend_semitones;
        self.set_expression(expression);
    }

    pub(super) fn set_expression(&mut self, expression: VoiceExpression) {
        let expression = expression.sanitized();
        if self.expression.stream.gate && !expression.stream.gate {
            self.amp_state.note_off();
            self.secondary_state.note_off();
        }
        self.pitch_bend_semitones
            .set_target(expression.stream.pitch_bend);
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.expression = expression;
    }

    #[cfg(test)]
    pub(super) fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        self.waveguide_loop_gain
            .set_target(WAVEGUIDE_LOOP_GAIN.clamp(loop_gain));
    }

    pub(super) fn reset_waveguide_loop_gain(&mut self, loop_gain: f32) {
        let loop_gain = WAVEGUIDE_LOOP_GAIN.clamp(loop_gain);
        self.applied_waveguide_loop_gain = loop_gain;
        self.waveguide_loop_gain.reset(loop_gain);
    }

    pub(super) fn next_pitch_bend_change(&mut self) -> Option<f32> {
        let pitch_bend = self.pitch_bend_semitones.next_sample();
        if (pitch_bend - self.applied_pitch_bend_semitones).abs() <= PITCH_BEND_EPSILON {
            return None;
        }

        self.applied_pitch_bend_semitones = pitch_bend;
        Some(pitch_bend)
    }

    pub(super) fn next_waveguide_loop_gain_change(&mut self) -> Option<f32> {
        let loop_gain = self.waveguide_loop_gain.next_sample();
        if (loop_gain - self.applied_waveguide_loop_gain).abs() <= PARAMETER_EPSILON {
            return None;
        }

        self.applied_waveguide_loop_gain = loop_gain;
        Some(loop_gain)
    }

    pub(super) fn next_sources(&mut self, sample_rate: f32) -> ModulationSources {
        self.apply_expression_values();
        let amp_envelope = self.amp_state.next_sample(self.amp_envelope, sample_rate);
        let secondary_envelope = self
            .secondary_state
            .next_sample(self.secondary_envelope, sample_rate);
        let lfo = self.next_lfo_sample(sample_rate);
        // Drive gate: instant on while the note is held (so a note-on drives
        // immediately), a short linear release to 0 after note-off so the bow/reed
        // lets go and the resonator rings out at its natural decay.
        let gate_target = if self.expression.stream.gate {
            1.0
        } else {
            0.0
        };
        self.drive_gate = if gate_target >= self.drive_gate {
            gate_target
        } else {
            (self.drive_gate - drive_gate_release_step(sample_rate)).max(0.0)
        };

        ModulationSources {
            amp_envelope,
            secondary_envelope,
            lfo,
            velocity: self.velocity,
            aftertouch: self.aftertouch,
            mod_wheel: self.mod_wheel,
            brightness: self.brightness,
            effort: self.effort(),
            energy: self.energy,
            drive_gate: self.drive_gate,
        }
    }

    fn next_lfo_sample(&mut self, sample_rate: f32) -> f32 {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        self.apply_expression_values();
        let lfo_rate_mod = self.modulation_sum(
            ModulationDestination::LfoRate,
            ModulationSources {
                amp_envelope: self.amp_state.value(),
                secondary_envelope: self.secondary_state.value(),
                lfo: 0.0,
                velocity: self.velocity,
                aftertouch: self.aftertouch,
                mod_wheel: self.mod_wheel,
                brightness: self.brightness,
                effort: self.effort(),
                energy: self.energy,
                drive_gate: self.drive_gate,
            },
        );
        let base_rate_hz = finite_clamp(self.config.lfo.rate_hz, 0.01, 100.0, 1.0);
        let rate_hz = finite_clamp(
            base_rate_hz * (1.0 + lfo_rate_mod).clamp(0.01, 16.0),
            0.01,
            100.0,
            base_rate_hz,
        );
        let phase = finite_clamp(self.lfo_phase, 0.0, 1.0, 0.0);
        self.lfo_phase = (phase + rate_hz / sample_rate).fract();

        match self.config.lfo.shape {
            crate::LfoShape::Sine => (std::f32::consts::TAU * self.lfo_phase).sin(),
            crate::LfoShape::Triangle => 4.0 * (self.lfo_phase - 0.5).abs() - 1.0,
            crate::LfoShape::Saw => self.lfo_phase * 2.0 - 1.0,
            crate::LfoShape::Square => {
                if self.lfo_phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            crate::LfoShape::SampleAndHold => self.lfo_hold,
        }
    }

    pub(super) fn modulation_sum(
        &self,
        destination: ModulationDestination,
        sources: ModulationSources,
    ) -> f32 {
        modulation_sum_from(self.config, destination, sources)
    }

    /// Player effort for the effort/energy bus (ADR-0014): the dominant of
    /// velocity (attack force) and pressure/breath (sustained force). Provisional
    /// blend — M8 (force-dependent driver) owns its refinement.
    fn effort(&self) -> f32 {
        finite_clamp(self.velocity.max(self.aftertouch), 0.0, 1.0, 0.0)
    }

    fn apply_expression_values(&mut self) {
        let expression = self.expression.sanitized();
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.expression = expression;
    }

    pub(super) fn config(&self) -> ModulationConfig {
        self.config
    }

    pub(super) fn resonator_modulation_active(&self) -> bool {
        self.resonator_modulation_active
    }

    pub(super) fn applied_pitch_bend_semitones(&self) -> f32 {
        self.applied_pitch_bend_semitones
    }

    pub(super) fn is_amp_idle(&self) -> bool {
        self.amp_state.phase() == EnvelopePhase::Idle
    }
}

fn source_value(source: ModulationSource, values: ModulationSources) -> f32 {
    match source {
        ModulationSource::SecondaryEnvelope => values.secondary_envelope,
        ModulationSource::Lfo => values.lfo,
        ModulationSource::Velocity => values.velocity,
        ModulationSource::Aftertouch => values.aftertouch,
        ModulationSource::ModWheel => values.mod_wheel,
        ModulationSource::Brightness => values.brightness,
    }
}

fn sample_and_hold_value(seed: u8) -> f32 {
    let mut value = u32::from(seed)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    value ^= value >> 16;
    (value as f32 / u32::MAX as f32) * 2.0 - 1.0
}

pub(super) fn modulation_sum_from(
    modulation: ModulationConfig,
    destination: ModulationDestination,
    sources: ModulationSources,
) -> f32 {
    let sum = modulation
        .slots
        .iter()
        .filter(|slot| slot.enabled && slot.destination == destination)
        .map(|slot| source_value(slot.source, sources) * slot.amount)
        .sum::<f32>();
    finite_clamp(sum, -1.0, 1.0, 0.0)
}

fn modulation_targets_resonators(modulation: ModulationConfig) -> bool {
    modulation.slots.iter().any(|slot| {
        slot.enabled
            && matches!(
                slot.destination,
                ModulationDestination::ResonatorADamping
                    | ModulationDestination::ResonatorBDamping
                    | ModulationDestination::ResonatorAPosition
                    | ModulationDestination::ResonatorBPosition
            )
    })
}

fn pitch_bend_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(-96.0, 96.0, 0.0, PITCH_BEND_SMOOTH_MS, PITCH_BEND_EPSILON)
}

fn waveguide_loop_gain_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(
        WAVEGUIDE_LOOP_GAIN.min,
        WAVEGUIDE_LOOP_GAIN.max,
        WAVEGUIDE_LOOP_GAIN.default,
        PARAMETER_SMOOTH_MS,
        PARAMETER_EPSILON,
    )
}

fn pitch_bend_param(sample_rate: f32, semitones: f32) -> SmoothedParam {
    SmoothedParam::with_initial(pitch_bend_spec(), sample_rate, semitones)
}

fn waveguide_loop_gain_param(sample_rate: f32, loop_gain: f32) -> SmoothedParam {
    SmoothedParam::with_initial(waveguide_loop_gain_spec(), sample_rate, loop_gain)
}

pub(super) fn sanitize_pitch_bend(semitones: f32) -> f32 {
    pitch_bend_spec().sanitize(semitones)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voice::VoiceExpression;

    #[test]
    fn effort_is_dominant_of_velocity_and_pressure() {
        let mut state = ModulationState::new(48_000.0);

        // Velocity dominates a lighter pressure.
        state.set_expression(VoiceExpression::with_controls(0.7, 0.0, 0.3, 0.0, 0.0));
        let effort = state.next_sources(48_000.0).effort;
        assert!((effort - 0.7).abs() < 1.0e-6, "effort={effort}");

        // A pressure swell above the velocity raises effort to the pressure.
        state.set_expression(VoiceExpression::with_controls(0.2, 0.0, 0.9, 0.0, 0.0));
        let effort = state.next_sources(48_000.0).effort;
        assert!((effort - 0.9).abs() < 1.0e-6, "effort={effort}");
    }

    #[test]
    fn drive_gate_holds_then_releases_on_note_off() {
        let sample_rate = 48_000.0;
        let mut state = ModulationState::new(sample_rate);

        // Held: the drive gate stays fully open.
        let held = VoiceExpression::with_controls(0.8, 0.0, 0.0, 0.0, 0.0);
        state.set_expression(held);
        for _ in 0..8 {
            assert_eq!(state.next_sources(sample_rate).drive_gate, 1.0);
        }

        // Note-off: the gate releases monotonically to 0 within the release time
        // (DRIVE_GATE_RELEASE_SECONDS ≈ 1440 samples at 48 kHz).
        let mut released = held;
        released.stream.gate = false;
        state.set_expression(released);
        let mut last = 1.0;
        for _ in 0..2_000 {
            let gate = state.next_sources(sample_rate).drive_gate;
            assert!(
                gate <= last,
                "drive gate should not rise on release: {gate} > {last}"
            );
            last = gate;
        }
        assert_eq!(last, 0.0, "drive gate should release fully to 0");
    }

    #[test]
    fn energy_source_tracks_observed_signal_rms() {
        let mut state = ModulationState::new(48_000.0);

        // At rest the energy bus reads zero.
        assert!(state.next_sources(48_000.0).energy.abs() < 1.0e-6);

        // Feed a +/-0.5 square wave (RMS == 0.5) through the follower.
        let amplitude = 0.5_f32;
        for index in 0..8_000 {
            let sample = if index % 2 == 0 {
                amplitude
            } else {
                -amplitude
            };
            state.observe_energy(sample);
        }

        let energy = state.next_sources(48_000.0).energy;
        assert!((energy - amplitude).abs() < 0.01, "energy={energy}");
    }
}
