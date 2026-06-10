use lindelion_dsp_utils::{
    delay::{DelayLine, FirstOrderAllpass},
    filters::{Biquad, BiquadCoefficients, OnePoleLowpass},
    math, soft_saturate,
};

use super::{
    DEFAULT_BIQUAD_Q, LOWEST_TUBE_FREQUENCY_HZ, TUBE_BOUNDARY,
    body::{TubeBody, TubeBodySample, register_body_blend},
    core,
    traveling::{BoundaryFilters, BoundarySide, PickupSamples, TravelingWavePair},
};

const MOUTH_REFLECTION: f32 = -0.36;
const MIN_END_REFLECTION_MAGNITUDE: f32 = 0.08;
const STEEPEN_ENERGY_REF: f32 = 0.005;
const STEEPEN_MAX_ENERGY: f32 = 1.0;
const STEEPEN_MAX_COEFF: f32 = 0.9;
const STEEPEN_AMPLITUDE_SENS: f32 = 10.0;
const RADIATION_CUTOFF_HZ: f32 = 1_850.0;
const GENTLE_RADIATION_CUTOFF_HZ: f32 = 1_850.0;
const BODY_ODD_MODE_PROJECTION_DEFAULT: f32 = 1.0;
const REGISTER_VENT_RADIATION_CUTOFF_HZ: f32 = 1_800.0;
const REGISTER_VENT_RADIATION_GAIN: f32 = 0.24;
const REED_PHASE_ONE_WAY_FACTOR: f32 = 0.5;
const WARM_BORE_DELAY_EXTRA_SAMPLES: f32 = 1.7;
const WARM_BORE_DELAY_FULL_CUTOFF_HZ: f32 = 1_300.0;
const WARM_BORE_DELAY_CLEAR_CUTOFF_HZ: f32 = 3_000.0;
const REGISTER_MODE_RATIO_MIN: f32 = 1.0;
const REGISTER_MODE_RATIO_MAX: f32 = 4.0;
const REGISTER_VENT_ADMITTANCE_MAX: f32 = 4.0;
const REGISTER_VENT_POSITION_DEFAULT: f32 = 1.0 / 3.0;
const REGISTER_VENT_MODE_CHOKE_Q: f32 = 2.0;

mod bore;
mod contour;
mod params;

use bore::{TubeBoreProfile, bore_frequency_hz, steepening_energy};

pub use params::{ReedTubeParams, ReedTubeSwitches, ReedTubeTaps};

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
    body_odd_mode_delay: DelayLine,
    prepared: Option<(ReedTubeParams, PreparedTubeModel)>,
    loop_gain: core::ScalarSmoother,
    loop_filter_cutoff: core::ScalarSmoother,
    loop_filter_resonance: core::ScalarSmoother,
    boundary_reflection: core::ScalarSmoother,
    frequency: core::ScalarSmoother,
    steepening_drive: f32,
    steepening_allpass: FirstOrderAllpass,
    radiation_highpass: Biquad,
    gentle_radiation_lowpass: OnePoleLowpass,
    register_vent_highpass: Biquad,
    register_vent_mode_choke_bandpass: Biquad,
    register_vent_flow: f32,
    register_radiation_source: Option<f32>,
    contour_h2: Biquad,
    contour_h3: Biquad,
    contour_h4: Biquad,
    contour_h5: Biquad,
    contour_h6: Biquad,
    contour_h7: Biquad,
    contour_upper: Biquad,
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
            body_odd_mode_delay: DelayLine::new(core::max_delay_samples(
                sample_rate,
                lowest_frequency_hz,
                2.0,
            )),
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
            gentle_radiation_lowpass: OnePoleLowpass::new(GENTLE_RADIATION_CUTOFF_HZ, sample_rate),
            register_vent_highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                REGISTER_VENT_RADIATION_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            register_vent_mode_choke_bandpass: Biquad::new(BiquadCoefficients::identity()),
            register_vent_flow: 0.0,
            register_radiation_source: None,
            contour_h2: Biquad::new(BiquadCoefficients::identity()),
            contour_h3: Biquad::new(BiquadCoefficients::identity()),
            contour_h4: Biquad::new(BiquadCoefficients::identity()),
            contour_h5: Biquad::new(BiquadCoefficients::identity()),
            contour_h6: Biquad::new(BiquadCoefficients::identity()),
            contour_h7: Biquad::new(BiquadCoefficients::identity()),
            contour_upper: Biquad::new(BiquadCoefficients::identity()),
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

    /// Source wave the register-mode body color radiates from on the next `process_wind` call,
    /// in place of the raw mouth wave — typically the reed's coherent (turbulence-free) output,
    /// so the open-hole lattice color does not re-emit shed jet noise. Consumed per sample;
    /// when unset the mouth wave is used.
    pub fn set_register_radiation_source(&mut self, sample: f32) {
        self.register_radiation_source = Some(math::finite_or(sample, 0.0));
    }

    pub fn reset(&mut self) {
        self.waves.clear();
        self.boundary_filters.reset();
        self.body.reset();
        self.body_odd_mode_delay.clear();
        self.prepared = None;
        self.loop_gain.reset();
        self.loop_filter_cutoff.reset();
        self.loop_filter_resonance.reset();
        self.boundary_reflection.reset();
        self.frequency.reset();
        self.steepening_drive = 0.0;
        self.steepening_allpass.reset();
        self.radiation_highpass.reset();
        self.gentle_radiation_lowpass.reset();
        self.register_vent_highpass.reset();
        self.register_vent_mode_choke_bandpass.reset();
        self.register_vent_flow = 0.0;
        self.register_radiation_source = None;
        self.contour_h2.reset();
        self.contour_h3.reset();
        self.contour_h4.reset();
        self.contour_h5.reset();
        self.contour_h6.reset();
        self.contour_h7.reset();
        self.contour_upper.reset();
        self.mouth_incident = 0.0;
    }

    pub fn process_wind(&mut self, mouth_wave: f32, params: ReedTubeParams) -> f32 {
        self.process_wind_inner(mouth_wave, params, None)
    }

    pub fn process_wind_with_taps(
        &mut self,
        mouth_wave: f32,
        params: ReedTubeParams,
        taps: &mut ReedTubeTaps,
    ) -> f32 {
        self.process_wind_inner(mouth_wave, params, Some(taps))
    }

    fn process_wind_inner(
        &mut self,
        mouth_wave: f32,
        params: ReedTubeParams,
        mut taps: Option<&mut ReedTubeTaps>,
    ) -> f32 {
        let params = self.smoothed_params(params.sanitized());
        let prepared = self.prepared_model(params);
        let profile = prepared.profile;
        let one_way_delay = prepared.one_way_delay;

        let boundary = self.waves.boundary_samples(one_way_delay);
        self.mouth_incident = boundary.left;
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);
        let pickup_sample = profile.pickup_sample(pickup);
        let body_input = if params.switches.body_enabled {
            self.closed_open_body_sample(pickup_sample, one_way_delay, params)
        } else {
            pickup_sample
        };
        let register_source = self.register_radiation_source.take().unwrap_or(mouth_wave);
        let body = if params.switches.body_enabled {
            self.body.process_sample(
                body_input,
                register_source,
                prepared.geometry.pickup_position,
                params,
            )
        } else {
            TubeBodySample {
                output: pickup_sample * TUBE_BOUNDARY.output_gain(params.boundary_reflection),
                reaction_flow: 0.0,
            }
        };
        let filtered = self
            .boundary_filters
            .process(BoundarySide::Left, mouth_wave);
        let mouth_reflection = math::snap_to_zero(self.apply_steepening(filtered, params));
        let end_reflection =
            self.reflected_sample(BoundarySide::Right, boundary.right, profile, params);
        if let Some(taps) = taps.as_mut() {
            **taps = ReedTubeTaps {
                mouth_wave,
                mouth_incident: boundary.left,
                mouth_filtered: filtered,
                mouth_reflection,
                bell_incident: boundary.right,
                bell_reflection: end_reflection,
                ..ReedTubeTaps::default()
            };
        }

        self.register_vent_flow = 0.0;
        self.apply_register_vent(one_way_delay, params);
        self.apply_body_reaction(
            one_way_delay,
            prepared.geometry.pickup_position,
            body.reaction_flow,
        );
        self.waves.push(end_reflection, mouth_reflection);
        self.output_sample(
            boundary.right,
            end_reflection,
            pickup,
            pickup_sample,
            body_input,
            body.output,
            body.reaction_flow,
            params,
            taps,
        )
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
            bell_radiation_shape: 0.0,
            body_formant: 0.0,
            body_formant_shift: 0.0,
            body_odd_mode_projection: 0.0,
            body_upper_odd_modes: 0.0,
            switches: ReedTubeSwitches::default(),
            ..params
        };
        if let Some((cached_params, prepared)) = self.prepared
            && cached_params == cache_key
        {
            return prepared;
        }

        let sounding_frequency_hz = params.frequency_hz;
        let bore_frequency_hz = bore_frequency_hz(params);
        let bore_cutoff_hz = core::bore_hf_loss_cutoff_hz(
            self.sample_rate,
            sounding_frequency_hz,
            params.loop_filter_cutoff_hz,
        );
        let damping = core::loop_damping(
            self.sample_rate,
            sounding_frequency_hz,
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
            sounding_frequency_hz,
        );
        let damping_phase_delay = core::filter_phase_delay_samples(
            damping.coefficients,
            self.sample_rate,
            sounding_frequency_hz,
        );
        let tuning = core::delay_tuning(
            self.sample_rate,
            self.waves.capacity(),
            bore_frequency_hz,
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
        self.register_vent_mode_choke_bandpass
            .set_coefficients(BiquadCoefficients::bandpass(
                self.sample_rate,
                sounding_frequency_hz,
                REGISTER_VENT_MODE_CHOKE_Q,
            ));
        self.set_clarinet_contour(sounding_frequency_hz);

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

    fn apply_register_vent(&mut self, one_way_delay: f32, params: ReedTubeParams) {
        let admittance = math::finite_clamp(
            params.register_vent_admittance,
            0.0,
            REGISTER_VENT_ADMITTANCE_MAX,
            0.0,
        );
        if admittance <= f32::EPSILON {
            return;
        }

        let position = math::finite_clamp(
            params.register_vent_position,
            0.05,
            0.95,
            REGISTER_VENT_POSITION_DEFAULT,
        );
        let junction = self.waves.junction_samples(one_way_delay, position);
        let pressure = math::snap_to_zero(junction.from_mouth + junction.from_bell);
        // The choke bandpass must run every sample for state continuity, even when the junction
        // pressure is momentarily zero.
        let mode_band_pressure = self.register_vent_mode_choke_bandpass.process(pressure);
        if pressure == 0.0 {
            return;
        }

        // Two equal-impedance bore sections plus a resistive shunt to atmosphere:
        // p = 2(a+b)/(2+Y). The through-going waves are already in the delay lines, so inject
        // only the correction from the open side hole, common to both outgoing directions. The
        // chimney anti-resonance chokes the shunt in a narrow band at the played register mode
        // (subtract the mode-band pressure from what the shunt responds to): the mode keeps its
        // oscillation margin — a frequency-flat shunt damps it ~4 dB per transit and the vented
        // attack takes >1 s — while the bore fundamental, whose hard resistive loading is what
        // makes the reed abandon the low register, sees exactly the legacy shunt. The shunt flow
        // itself radiates locally from the register key, but it is output-only: it does not feed
        // back into the bore or reed.
        let effective_pressure = pressure - mode_band_pressure;
        let junction_pressure = 2.0 * effective_pressure / (2.0 + admittance);
        self.register_vent_flow = math::snap_to_zero(admittance * junction_pressure);
        let correction = -effective_pressure * admittance / (2.0 + admittance);
        self.waves
            .add_junction_correction(one_way_delay, position, correction);
    }

    fn apply_body_reaction(&mut self, one_way_delay: f32, position: f32, reaction_flow: f32) {
        let reaction_flow = math::snap_to_zero(reaction_flow);
        if reaction_flow == 0.0 {
            return;
        }

        self.waves
            .add_junction_correction(one_way_delay, position, -0.5 * reaction_flow);
    }

    #[allow(clippy::too_many_arguments)] // output staging: each tap is a distinct signal
    fn output_sample(
        &mut self,
        bell_incident: f32,
        bell_reflected: f32,
        pickup: PickupSamples,
        pickup_sample: f32,
        body_input: f32,
        body: f32,
        body_reaction_flow: f32,
        params: ReedTubeParams,
        taps: Option<&mut ReedTubeTaps>,
    ) -> f32 {
        let pickup_pressure = pickup.average();
        let pickup_flow = (pickup.right - pickup.left) * 0.5;
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
        let bell_pressure = bell_incident + bell_reflected;
        let radiated = self.radiated_bell_sample(bell_pressure, params) * bell_gain;

        let register_vent = self.radiated_register_vent_sample();

        let body_bell_sum = body + radiated;
        let main_output = if params.switches.clarinet_contour_enabled {
            self.clarinet_contour_sample(body_bell_sum)
        } else {
            body_bell_sum
        };

        let final_output = math::snap_to_zero(main_output + register_vent);
        if let Some(taps) = taps {
            taps.bell_pressure = bell_pressure;
            taps.pickup_left = pickup.left;
            taps.pickup_right = pickup.right;
            taps.pickup_pressure = pickup_pressure;
            taps.pickup_flow = pickup_flow;
            taps.pickup_sample = pickup_sample;
            taps.body_input = body_input;
            taps.body_output = body;
            taps.body_reaction_flow = body_reaction_flow;
            taps.bell_radiated = radiated;
            taps.register_vent_flow = self.register_vent_flow;
            taps.register_vent_output = register_vent;
            taps.body_bell_sum = body_bell_sum;
            taps.main_output = main_output;
            taps.final_output = final_output;
        }
        final_output
    }

    fn closed_open_body_sample(
        &mut self,
        sample: f32,
        one_way_delay: f32,
        params: ReedTubeParams,
    ) -> f32 {
        let half_period_delay =
            (one_way_delay * 2.0).clamp(0.0, self.body_odd_mode_delay.capacity() as f32 - 3.0);
        let delayed = self.body_odd_mode_delay.read(half_period_delay);
        self.body_odd_mode_delay.push(sample);
        let odd_mode = (sample - delayed) * 0.5;
        // The projection delay is half the BORE round trip, so above the break (sounding pitch on
        // the third bore mode) it is no longer half the sounding period: the comb misaligns and
        // attenuates the sounding h3/h4/h5 band instead of rejecting evens. The open register
        // vent breaks the even-cancelling symmetry anyway, so the register-aware body bypasses
        // the projection entirely.
        let projection = math::finite_clamp(params.body_odd_mode_projection, 0.0, 1.0, 1.0)
            * (1.0 - register_body_blend(params));
        math::snap_to_zero(sample + (odd_mode - sample) * projection)
    }

    fn radiated_bell_sample(&mut self, bell_pressure: f32, params: ReedTubeParams) -> f32 {
        let current = self.radiation_highpass.process(bell_pressure);
        let gentle = bell_pressure - self.gentle_radiation_lowpass.process(bell_pressure);
        let shape = math::finite_clamp(params.bell_radiation_shape, 0.0, 1.0, 0.0);
        math::snap_to_zero(current + (gentle - current) * shape)
    }

    fn radiated_register_vent_sample(&mut self) -> f32 {
        let bright_flow = self.register_vent_highpass.process(self.register_vent_flow);
        math::snap_to_zero(bright_flow * REGISTER_VENT_RADIATION_GAIN)
    }
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

#[cfg(test)]
mod tests;
