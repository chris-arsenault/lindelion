use lindelion_dsp_utils::{
    delay::FirstOrderAllpass,
    filters::{Biquad, BiquadCoefficients},
    math, soft_saturate,
};

use super::{
    BOUNDARY_REFLECTION_DEFAULT, DEFAULT_BIQUAD_Q, LOOP_FILTER_CUTOFF_DEFAULT_HZ,
    LOOP_FILTER_RESONANCE_DEFAULT, LOOP_GAIN_DEFAULT, LOWEST_TUBE_FREQUENCY_HZ,
    PICKUP_POSITION_DEFAULT, TUBE_BOUNDARY,
    body::TubeBody,
    core,
    traveling::{BoundaryFilters, BoundarySide, PickupSamples, TravelingWavePair},
};

const MOUTH_REFLECTION: f32 = -0.36;
const MIN_END_REFLECTION_MAGNITUDE: f32 = 0.08;
const STEEPEN_ENERGY_REF: f32 = 0.005;
const STEEPEN_MAX_ENERGY: f32 = 1.0;
const STEEPEN_MAX_COEFF: f32 = 0.9;
const STEEPEN_AMPLITUDE_SENS: f32 = 10.0;
const RADIATION_CUTOFF_HZ: f32 = 500.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReedTubeSwitches {
    pub reed_enabled: bool,
    pub bell_enabled: bool,
    pub bore_steepening_enabled: bool,
    pub body_enabled: bool,
}

impl Default for ReedTubeSwitches {
    fn default() -> Self {
        Self {
            reed_enabled: true,
            bell_enabled: true,
            bore_steepening_enabled: true,
            body_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReedTubeParams {
    pub frequency_hz: f32,
    pub loop_filter_cutoff_hz: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub boundary_reflection: f32,
    pub pickup_position: f32,
    pub bell_radiation: f32,
    pub switches: ReedTubeSwitches,
}

impl Default for ReedTubeParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: LOOP_FILTER_CUTOFF_DEFAULT_HZ,
            loop_filter_resonance: LOOP_FILTER_RESONANCE_DEFAULT,
            loop_gain: LOOP_GAIN_DEFAULT,
            loop_nonlinearity: 0.0,
            boundary_reflection: BOUNDARY_REFLECTION_DEFAULT,
            pickup_position: PICKUP_POSITION_DEFAULT,
            bell_radiation: 1.0,
            switches: ReedTubeSwitches::default(),
        }
    }
}

impl ReedTubeParams {
    pub fn sanitized(self) -> Self {
        let fallback = Self::default();
        Self {
            frequency_hz: math::finite_clamp(
                self.frequency_hz,
                1.0,
                22_000.0,
                fallback.frequency_hz,
            ),
            loop_filter_cutoff_hz: math::finite_clamp(
                self.loop_filter_cutoff_hz,
                20.0,
                22_000.0,
                fallback.loop_filter_cutoff_hz,
            ),
            loop_filter_resonance: unit(self.loop_filter_resonance, fallback.loop_filter_resonance),
            loop_gain: math::finite_clamp(self.loop_gain, 0.0, 0.999, fallback.loop_gain),
            loop_nonlinearity: unit(self.loop_nonlinearity, fallback.loop_nonlinearity),
            boundary_reflection: math::finite_clamp(
                self.boundary_reflection,
                -1.0,
                1.0,
                fallback.boundary_reflection,
            ),
            pickup_position: math::finite_clamp(
                self.pickup_position,
                0.001,
                0.999,
                fallback.pickup_position,
            ),
            bell_radiation: unit(self.bell_radiation, fallback.bell_radiation),
            switches: ReedTubeSwitches {
                reed_enabled: true,
                ..self.switches
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PreparedTubeModel {
    profile: TubeBoreProfile,
    geometry: core::WaveguideGeometry,
    one_way_delay: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReedTube {
    sample_rate: f32,
    waves: TravelingWavePair,
    boundary_filters: BoundaryFilters,
    body: TubeBody,
    prepared: Option<(ReedTubeParams, PreparedTubeModel)>,
    loop_gain: core::ScalarSmoother,
    loop_filter_cutoff: core::ScalarSmoother,
    loop_filter_resonance: core::ScalarSmoother,
    boundary_reflection: core::ScalarSmoother,
    frequency: core::ScalarSmoother,
    steepening_drive: f32,
    steepening_allpass: FirstOrderAllpass,
    radiation_highpass: Biquad,
    mouth_incident: f32,
}

impl ReedTube {
    pub fn new(sample_rate: f32) -> Self {
        Self::with_lowest_frequency(sample_rate, LOWEST_TUBE_FREQUENCY_HZ)
    }

    pub fn with_lowest_frequency(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            waves: TravelingWavePair::new(sample_rate, lowest_frequency_hz, 4.0),
            boundary_filters: BoundaryFilters::new(),
            body: TubeBody::new(sample_rate),
            prepared: None,
            loop_gain: core::ScalarSmoother::new(sample_rate),
            loop_filter_cutoff: core::ScalarSmoother::new(sample_rate),
            loop_filter_resonance: core::ScalarSmoother::new(sample_rate),
            boundary_reflection: core::ScalarSmoother::new(sample_rate),
            frequency: core::ScalarSmoother::new(sample_rate),
            steepening_drive: 0.0,
            steepening_allpass: FirstOrderAllpass::default(),
            radiation_highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                RADIATION_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            mouth_incident: 0.0,
        }
    }

    pub fn driven_feedback(&self) -> f32 {
        self.mouth_incident
    }

    pub fn set_brightness_effort(&mut self, effort: f32) {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        self.steepening_drive = STEEPEN_ENERGY_REF * effort;
    }

    pub fn reset(&mut self) {
        self.waves.clear();
        self.boundary_filters.reset();
        self.body.reset();
        self.prepared = None;
        self.loop_gain.reset();
        self.loop_filter_cutoff.reset();
        self.loop_filter_resonance.reset();
        self.boundary_reflection.reset();
        self.frequency.reset();
        self.steepening_drive = 0.0;
        self.steepening_allpass.reset();
        self.radiation_highpass.reset();
        self.mouth_incident = 0.0;
    }

    pub fn process_wind(&mut self, mouth_wave: f32, params: ReedTubeParams) -> f32 {
        let params = self.smoothed_params(params.sanitized());
        let prepared = self.prepared_model(params);
        let profile = prepared.profile;
        let one_way_delay = prepared.one_way_delay;

        let boundary = self.waves.boundary_samples(one_way_delay);
        self.mouth_incident = boundary.left;
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);
        let filtered = self
            .boundary_filters
            .process(BoundarySide::Left, mouth_wave);
        let mouth_reflection = math::snap_to_zero(self.apply_steepening(filtered, params));
        let end_reflection =
            self.reflected_sample(BoundarySide::Right, boundary.right, profile, params);

        self.waves.push(end_reflection, mouth_reflection);
        self.output_sample(boundary.right, end_reflection, pickup, profile, params)
    }

    fn smoothed_params(&mut self, params: ReedTubeParams) -> ReedTubeParams {
        ReedTubeParams {
            loop_gain: self.loop_gain.next(params.loop_gain),
            loop_filter_cutoff_hz: self.loop_filter_cutoff.next(params.loop_filter_cutoff_hz),
            loop_filter_resonance: self
                .loop_filter_resonance
                .next(params.loop_filter_resonance),
            boundary_reflection: self.boundary_reflection.next(params.boundary_reflection),
            frequency_hz: self.frequency.next(params.frequency_hz),
            ..params
        }
    }

    fn prepared_model(&mut self, params: ReedTubeParams) -> PreparedTubeModel {
        let cache_key = ReedTubeParams {
            bell_radiation: 1.0,
            switches: ReedTubeSwitches::default(),
            ..params
        };
        if let Some((cached_params, prepared)) = self.prepared
            && cached_params == cache_key
        {
            return prepared;
        }

        let damping = core::loop_damping(
            self.sample_rate,
            params.frequency_hz,
            params.loop_filter_cutoff_hz,
            params.loop_filter_resonance,
            params.loop_gain,
        );
        let profile = TubeBoreProfile::from_params(self.sample_rate, params, damping.loop_gain);
        let geometry = core::waveguide_geometry(params.pickup_position);
        let mouth_phase_delay = core::filter_phase_delay_samples(
            profile.mouth_loss,
            self.sample_rate,
            params.frequency_hz,
        );
        let damping_phase_delay = core::filter_phase_delay_samples(
            damping.coefficients,
            self.sample_rate,
            params.frequency_hz,
        );
        let tuning = core::delay_tuning(
            self.sample_rate,
            self.waves.capacity(),
            params.frequency_hz,
            4.0,
            1.0 + 0.5 * (mouth_phase_delay + damping_phase_delay),
        );
        let one_way_delay = tuning.integer_delay + tuning.fractional_delay;

        self.boundary_filters
            .set_coefficients(profile.mouth_loss, damping.coefficients);

        let prepared = PreparedTubeModel {
            profile,
            geometry,
            one_way_delay,
        };
        self.prepared = Some((cache_key, prepared));
        prepared
    }

    fn reflected_sample(
        &mut self,
        side: BoundarySide,
        input: f32,
        profile: TubeBoreProfile,
        params: ReedTubeParams,
    ) -> f32 {
        let filtered = self.boundary_filters.process(side, input);
        let nonlinear = if side == BoundarySide::Left {
            let static_drive = math::finite_clamp(params.loop_nonlinearity, 0.0, 1.0, 0.0);
            let saturated = if static_drive > 0.0 {
                soft_saturate(filtered, static_drive)
            } else {
                filtered
            };
            self.apply_steepening(saturated, params)
        } else {
            filtered
        };
        let reflection = match side {
            BoundarySide::Left => profile.mouth_reflection,
            BoundarySide::Right => profile.end_reflection,
        };
        math::snap_to_zero(nonlinear * reflection)
    }

    fn apply_steepening(&mut self, sample: f32, params: ReedTubeParams) -> f32 {
        if !params.switches.bore_steepening_enabled {
            return sample;
        }
        let energy = steepening_energy(self.steepening_drive);
        if energy <= f32::EPSILON {
            return sample;
        }
        let amplitude = (sample.abs() * STEEPEN_AMPLITUDE_SENS).tanh();
        let coefficient = math::finite_clamp(STEEPEN_MAX_COEFF * energy * amplitude, 0.0, 1.0, 0.0);
        self.steepening_allpass.set_coefficient(coefficient);
        self.steepening_allpass.process(sample)
    }

    fn output_sample(
        &mut self,
        bell_incident: f32,
        bell_reflected: f32,
        pickup: PickupSamples,
        profile: TubeBoreProfile,
        params: ReedTubeParams,
    ) -> f32 {
        let pickup_sample = profile.pickup_sample(pickup);
        let body = if params.switches.body_enabled {
            self.body.process_sample(pickup_sample, params)
        } else {
            pickup_sample * TUBE_BOUNDARY.output_gain(params.boundary_reflection)
        };
        let bell_gain = if params.switches.bell_enabled {
            params.bell_radiation
        } else {
            0.0
        };
        // Energy-conserving bell radiation. What radiates out the bell is the part of the
        // bell-incident wave NOT reflected back into the bore — i.e. the acoustic pressure at the
        // open end, `incident + reflected` (reflected = R·incident with R the loss-lowpass ×
        // end_reflection, ≈-0.75 at DC rolling to ≈0 at HF, so this sum is ≈0.25·incident at DC →
        // ≈incident at HF: lows stay in the bore, highs leave). It is bounded by the incident wave
        // (no gain > 1) and not double-counted (the reflected part is what we pushed back into the
        // loop). The far-field radiation highpass shapes the bell's HF-favouring efficiency. This
        // replaces the old `highpass(incident)·2.5·effort²` tap, which re-emitted the highs above
        // unity *on top of* reflecting them and squared the tone; brightness-with-effort must now
        // come from the source (the reed generating more harmonics), not this tap.
        let radiated = self
            .radiation_highpass
            .process(bell_incident + bell_reflected)
            * bell_gain;

        math::snap_to_zero(body + radiated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TubeBoreProfile {
    mouth_loss: BiquadCoefficients,
    mouth_reflection: f32,
    end_reflection: f32,
    pressure_mix: f32,
}

impl TubeBoreProfile {
    fn from_params(sample_rate: f32, params: ReedTubeParams, loop_gain: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let endpoint_loss = core::endpoint_reflection_gain(loop_gain);
        let end_reflection = bore_end_reflection(params.boundary_reflection) * endpoint_loss;
        let openness = (1.0 - TUBE_BOUNDARY.reflection(params.boundary_reflection)) * 0.5;
        let mouth_cutoff = math::finite_clamp(
            params.loop_filter_cutoff_hz * (0.75 + 0.35 * openness),
            160.0,
            sample_rate * 0.45,
            6_000.0,
        );

        Self {
            mouth_loss: BiquadCoefficients::lowpass(sample_rate, mouth_cutoff, DEFAULT_BIQUAD_Q),
            mouth_reflection: MOUTH_REFLECTION * endpoint_loss,
            end_reflection,
            pressure_mix: math::finite_clamp(0.30 + 0.60 * (1.0 - openness), 0.2, 0.95, 0.65),
        }
    }

    fn pickup_sample(self, pickup: PickupSamples) -> f32 {
        let pressure = pickup.average();
        let flow = (pickup.right - pickup.left) * 0.5;
        math::snap_to_zero(pressure * self.pressure_mix + flow * (1.0 - self.pressure_mix))
    }
}

fn bore_end_reflection(boundary_reflection: f32) -> f32 {
    let reflection = TUBE_BOUNDARY.reflection(boundary_reflection);
    if reflection.abs() < MIN_END_REFLECTION_MAGNITUDE {
        MIN_END_REFLECTION_MAGNITUDE.copysign(reflection)
    } else {
        reflection
    }
}

fn steepening_energy(energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / STEEPEN_ENERGY_REF;
    math::finite_clamp(normalized * normalized, 0.0, STEEPEN_MAX_ENERGY, 0.0)
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReedDriver, ReedParams};
    use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};

    #[test]
    fn reed_driven_tube_renders_finite_audible_audio() {
        let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
        let mut tube = ReedTube::new(48_000.0);
        let params = ReedTubeParams {
            frequency_hz: 220.0,
            ..ReedTubeParams::default()
        };
        let mut output = Vec::with_capacity(4096);

        for index in 0..4096 {
            let excitation = if index == 0 { 0.4 } else { 0.0 };
            tube.set_brightness_effort(0.8);
            let mouth = reed.process(excitation, 0.8, tube.driven_feedback(), 1.0);
            output.push(tube.process_wind(mouth, params));
        }

        assert_all_finite(&output);
        assert!(peak_abs(&output) > 0.001);
        assert!(rms(&output[1024..]) > 0.000_01);
    }

    #[test]
    fn bell_radiation_does_not_depend_on_effort() {
        // Energy-conservation invariant for the redesigned bell: it radiates the bell-incident
        // wave NOT reflected back into the bore (`incident + reflected`, bounded by the incident —
        // no gain > 1, no double-count), shaped by a fixed far-field HF filter. A real bell's
        // radiation *efficiency* is fixed; what rises with blowing is the harmonic content the
        // reed generates, which then radiates through this fixed bell. So the bell's relative
        // contribution must be (near) **effort-independent** — unlike the old tap, which gated the
        // whole radiation by `effort²` (silent-ish soft, ballooning loud → the square at ff). Guard
        // that the bell-on / bell-off level ratio is similar at soft and hard effort.
        let ratio_at = |effort: f32| {
            let render = |bell_enabled: bool| {
                let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
                let mut tube = ReedTube::new(48_000.0);
                let params = ReedTubeParams {
                    frequency_hz: 220.0,
                    switches: ReedTubeSwitches {
                        bell_enabled,
                        ..ReedTubeSwitches::default()
                    },
                    ..ReedTubeParams::default()
                };
                let mut out = Vec::with_capacity(8192);
                for index in 0..8192 {
                    let excitation = if index == 0 { 0.4 } else { 0.0 };
                    tube.set_brightness_effort(effort);
                    let mouth = reed.process(excitation, effort, tube.driven_feedback(), 1.0);
                    out.push(tube.process_wind(mouth, params));
                }
                out
            };
            let on = render(true);
            let off = render(false);
            assert_all_finite(&on);
            peak_abs(&on[2048..]) / peak_abs(&off[2048..]).max(1.0e-6)
        };
        let soft = ratio_at(0.45);
        let hard = ratio_at(1.0);
        // The old effort²-gated tap made this ratio swing wildly with effort; the fixed-efficiency
        // bell keeps it stable (within ~30%).
        assert!(
            (soft / hard).max(hard / soft) < 1.3,
            "bell contribution is effort-dependent (soft ratio {soft}, hard ratio {hard}) — should be fixed-efficiency"
        );
    }

    #[test]
    fn model_switches_do_not_make_output_non_finite() {
        let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
        let mut tube = ReedTube::new(48_000.0);
        let params = ReedTubeParams {
            switches: ReedTubeSwitches {
                bell_enabled: false,
                bore_steepening_enabled: false,
                body_enabled: false,
                ..ReedTubeSwitches::default()
            },
            ..ReedTubeParams::default()
        };

        for index in 0..512 {
            let excitation = if index == 0 { 0.3 } else { 0.0 };
            let mouth = reed.process(excitation, 0.7, tube.driven_feedback(), 1.0);
            assert!(tube.process_wind(mouth, params).is_finite());
        }
    }
}
