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
const REED_PHASE_ONE_WAY_FACTOR: f32 = 0.5;
const WARM_BORE_DELAY_EXTRA_SAMPLES: f32 = 1.7;
const WARM_BORE_DELAY_FULL_CUTOFF_HZ: f32 = 1_300.0;
const WARM_BORE_DELAY_CLEAR_CUTOFF_HZ: f32 = 3_000.0;

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
    /// Phase delay (samples) the inertial reed aperture adds to the feedback loop at the
    /// playing frequency, supplied by the driving [`crate::ReedDriver`]. Folded into the
    /// bore-length tuning so the reed's loop phase is compensated like the mouth-loss and
    /// damping filters; `0.0` (instant aperture) leaves tuning unchanged.
    pub reed_phase_delay_samples: f32,
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
            reed_phase_delay_samples: 0.0,
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
            reed_phase_delay_samples: math::finite_clamp(
                self.reed_phase_delay_samples,
                0.0,
                24.0,
                fallback.reed_phase_delay_samples,
            ),
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
    #[cfg(test)]
    recompute_count: u32,
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
            #[cfg(test)]
            recompute_count: 0,
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

        let bore_cutoff_hz = core::bore_hf_loss_cutoff_hz(
            self.sample_rate,
            params.frequency_hz,
            params.loop_filter_cutoff_hz,
        );
        let damping = core::loop_damping(
            self.sample_rate,
            params.frequency_hz,
            bore_cutoff_hz,
            params.loop_filter_resonance,
            params.loop_gain,
        );
        let profile = TubeBoreProfile::from_params(
            self.sample_rate,
            params,
            damping.loop_gain,
            bore_cutoff_hz,
        );
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
            delay_offset_samples(
                bore_cutoff_hz,
                mouth_phase_delay,
                damping_phase_delay,
                params.reed_phase_delay_samples,
            ),
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
        #[cfg(test)]
        {
            self.recompute_count += 1;
        }
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
    fn from_params(
        sample_rate: f32,
        params: ReedTubeParams,
        loop_gain: f32,
        bore_cutoff_hz: f32,
    ) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let endpoint_loss = core::endpoint_reflection_gain(loop_gain);
        let end_reflection = bore_end_reflection(params.boundary_reflection) * endpoint_loss;
        let openness = (1.0 - TUBE_BOUNDARY.reflection(params.boundary_reflection)) * 0.5;
        let mouth_cutoff = math::finite_clamp(
            bore_cutoff_hz * (0.90 + 0.15 * openness),
            160.0,
            sample_rate * 0.45,
            1_900.0,
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

fn delay_offset_samples(
    bore_cutoff_hz: f32,
    mouth_phase_delay: f32,
    damping_phase_delay: f32,
    reed_phase_delay: f32,
) -> f32 {
    let warm_bore = ((WARM_BORE_DELAY_CLEAR_CUTOFF_HZ - bore_cutoff_hz)
        / (WARM_BORE_DELAY_CLEAR_CUTOFF_HZ - WARM_BORE_DELAY_FULL_CUTOFF_HZ))
        .clamp(0.0, 1.0);
    let phase_scale = 0.5 + 0.5 * warm_bore;
    // The mouth-loss and damping filters each sit in the loop once per round trip; their
    // phase delays convert to a one-way bore-length reduction by `phase_scale`, whose
    // `warm_bore` ramp (½→1) is an empirical bore-coloration adjustment for those filters.
    // The reed aperture also sits in the loop once per round trip, but its round-trip→one-way
    // conversion is the plain physical ½ (halving one-way delay shortens the round-trip period
    // by the reed's full phase delay) and must NOT ride the bore-coloration ramp, or warm
    // bores double the reed compensation and play sharp. `reed_phase_delay` already carries the
    // reed's nonlinear-coupling and effort factors (see `ReedDriver::aperture_phase_delay_samples`).
    1.0 + WARM_BORE_DELAY_EXTRA_SAMPLES * warm_bore
        + phase_scale * (mouth_phase_delay + damping_phase_delay)
        + REED_PHASE_ONE_WAY_FACTOR * reed_phase_delay
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests;
