use lindelion_dsp_utils::{
    delay::{DelayLine, FirstOrderAllpass},
    filters::{Biquad, BiquadCoefficients, OnePoleLowpass},
    math, soft_saturate,
};

use super::{
    BOUNDARY_REFLECTION_DEFAULT, DEFAULT_BIQUAD_Q, LOOP_FILTER_CUTOFF_DEFAULT_HZ,
    LOOP_FILTER_RESONANCE_DEFAULT, LOOP_GAIN_DEFAULT, LOWEST_TUBE_FREQUENCY_HZ,
    PICKUP_POSITION_DEFAULT, TUBE_BOUNDARY,
    body::{TubeBody, TubeBodySample},
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
const REGISTER_VENT_TURBULENCE_LOW_CUTOFF_HZ: f32 = 2_000.0;
const REGISTER_VENT_TURBULENCE_HIGH_CUTOFF_HZ: f32 = 2_700.0;
const REGISTER_VENT_TURBULENCE_GAIN: f32 = 0.0015;
const REED_PHASE_ONE_WAY_FACTOR: f32 = 0.5;
const WARM_BORE_DELAY_EXTRA_SAMPLES: f32 = 1.7;
const WARM_BORE_DELAY_FULL_CUTOFF_HZ: f32 = 1_300.0;
const WARM_BORE_DELAY_CLEAR_CUTOFF_HZ: f32 = 3_000.0;
const CLARINET_CONTOUR_Q: f32 = 10.0;
const CLARINET_CONTOUR_H2_DB: f32 = -18.0;
const CLARINET_CONTOUR_H3_DB: f32 = -6.0;
const CLARINET_CONTOUR_H4_DB: f32 = -16.0;
const CLARINET_CONTOUR_H5_DB: f32 = -18.0;
const CLARINET_CONTOUR_H6_DB: f32 = -10.0;
const CLARINET_CONTOUR_H7_DB: f32 = -12.0;
const CLARINET_CONTOUR_UPPER_HZ: f32 = 3_100.0;
const CLARINET_CONTOUR_UPPER_Q: f32 = 1.15;
const CLARINET_CONTOUR_UPPER_DB: f32 = 8.0;
const REGISTER_MODE_RATIO_MIN: f32 = 1.0;
const REGISTER_MODE_RATIO_MAX: f32 = 4.0;
const REGISTER_VENT_ADMITTANCE_MAX: f32 = 4.0;
const REGISTER_VENT_POSITION_DEFAULT: f32 = 1.0 / 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReedTubeSwitches {
    pub reed_enabled: bool,
    pub bell_enabled: bool,
    pub bore_steepening_enabled: bool,
    pub body_enabled: bool,
    pub clarinet_contour_enabled: bool,
}

impl Default for ReedTubeSwitches {
    fn default() -> Self {
        Self {
            reed_enabled: true,
            bell_enabled: true,
            bore_steepening_enabled: true,
            body_enabled: true,
            clarinet_contour_enabled: false,
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
    pub bell_radiation_shape: f32,
    pub body_formant: f32,
    pub body_formant_shift: f32,
    /// Ratio between sounding frequency and bore fundamental. A register-keyed clarinet note
    /// speaks on the third bore mode, so `3.0` keeps the long low-register bore while the sounding
    /// frequency stays high.
    pub register_mode_ratio: f32,
    /// Effective side-hole admittance for the register vent. `0.0` is closed; larger values leak
    /// pressure at `register_vent_position`, suppressing the fundamental and encouraging the third
    /// mode.
    pub register_vent_admittance: f32,
    /// Normalized bore position of the register vent, measured from the mouthpiece.
    pub register_vent_position: f32,
    /// Mix amount for the closed-open body projection. `1.0` is the physical odd-mode
    /// projection `0.5 * (x[n] - x[n - T/2])`, which rejects even harmonics at the radiating body
    /// input; lower values leak direct pickup pressure into the body path.
    pub body_odd_mode_projection: f32,
    /// Strength of the low-register upper odd body/radiation mode bank. These tracked h9/h11/h13
    /// modes fill the clarinet tail above the primary h3/h5/h7 body resonances.
    pub body_upper_odd_modes: f32,
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
            bell_radiation_shape: 0.0,
            body_formant: 0.0,
            body_formant_shift: 0.0,
            register_mode_ratio: 1.0,
            register_vent_admittance: 0.0,
            register_vent_position: REGISTER_VENT_POSITION_DEFAULT,
            body_odd_mode_projection: BODY_ODD_MODE_PROJECTION_DEFAULT,
            body_upper_odd_modes: 1.0,
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
            bell_radiation_shape: unit(self.bell_radiation_shape, fallback.bell_radiation_shape),
            body_formant: unit(self.body_formant, fallback.body_formant),
            body_formant_shift: math::finite_clamp(
                self.body_formant_shift,
                -2.0,
                2.0,
                fallback.body_formant_shift,
            ),
            register_mode_ratio: math::finite_clamp(
                self.register_mode_ratio,
                REGISTER_MODE_RATIO_MIN,
                REGISTER_MODE_RATIO_MAX,
                fallback.register_mode_ratio,
            ),
            register_vent_admittance: math::finite_clamp(
                self.register_vent_admittance,
                0.0,
                REGISTER_VENT_ADMITTANCE_MAX,
                fallback.register_vent_admittance,
            ),
            register_vent_position: math::finite_clamp(
                self.register_vent_position,
                0.05,
                0.95,
                fallback.register_vent_position,
            ),
            body_odd_mode_projection: unit(
                self.body_odd_mode_projection,
                fallback.body_odd_mode_projection,
            ),
            body_upper_odd_modes: unit(self.body_upper_odd_modes, fallback.body_upper_odd_modes),
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReedTubeTaps {
    pub mouth_wave: f32,
    pub mouth_incident: f32,
    pub mouth_filtered: f32,
    pub mouth_reflection: f32,
    pub bell_incident: f32,
    pub bell_reflection: f32,
    pub bell_pressure: f32,
    pub pickup_left: f32,
    pub pickup_right: f32,
    pub pickup_pressure: f32,
    pub pickup_flow: f32,
    pub pickup_sample: f32,
    pub body_input: f32,
    pub body_output: f32,
    pub body_reaction_flow: f32,
    pub bell_radiated: f32,
    pub register_vent_flow: f32,
    pub register_vent_output: f32,
    pub body_bell_sum: f32,
    pub main_output: f32,
    pub final_output: f32,
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
    register_vent_turbulence_highpass: Biquad,
    register_vent_turbulence_lowpass: Biquad,
    register_vent_flow: f32,
    register_vent_noise: u32,
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
            register_vent_turbulence_highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                REGISTER_VENT_TURBULENCE_LOW_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            register_vent_turbulence_lowpass: Biquad::new(BiquadCoefficients::lowpass(
                sample_rate,
                REGISTER_VENT_TURBULENCE_HIGH_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            register_vent_flow: 0.0,
            register_vent_noise: 0x4F1B_BCDC,
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
        self.register_vent_turbulence_highpass.reset();
        self.register_vent_turbulence_lowpass.reset();
        self.register_vent_flow = 0.0;
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
        let body = if params.switches.body_enabled {
            self.body
                .process_sample(body_input, prepared.geometry.pickup_position, params)
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
        if pressure == 0.0 {
            return;
        }

        // Two equal-impedance bore sections plus a resistive shunt to atmosphere:
        // p = 2(a+b)/(2+Y). The through-going waves are already in the delay lines, so inject only
        // the correction from the open side hole, common to both outgoing directions. The shunt flow
        // itself radiates locally from the register key, but it is output-only: it does not feed back
        // into the bore or reed.
        let junction_pressure = 2.0 * pressure / (2.0 + admittance);
        self.register_vent_flow = math::snap_to_zero(admittance * junction_pressure);
        let correction = -pressure * admittance / (2.0 + admittance);
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
        let projection = math::finite_clamp(params.body_odd_mode_projection, 0.0, 1.0, 1.0);
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
        let turbulence = self.register_vent_turbulence_sample();
        math::snap_to_zero(bright_flow * REGISTER_VENT_RADIATION_GAIN + turbulence)
    }

    fn register_vent_turbulence_sample(&mut self) -> f32 {
        let flow_drive = math::finite_clamp(self.register_vent_flow.abs(), 0.0, 1.0, 0.0).sqrt();
        if flow_drive <= f32::EPSILON {
            return 0.0;
        }

        let noise = self.next_register_vent_noise() * flow_drive * REGISTER_VENT_TURBULENCE_GAIN;
        let bright_noise = self.register_vent_turbulence_highpass.process(noise);
        math::snap_to_zero(self.register_vent_turbulence_lowpass.process(bright_noise))
    }

    fn next_register_vent_noise(&mut self) -> f32 {
        let mut x = self.register_vent_noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.register_vent_noise = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn set_clarinet_contour(&mut self, frequency_hz: f32) {
        let frequency_hz = math::finite_clamp(frequency_hz, 20.0, self.sample_rate * 0.20, 220.0);
        self.contour_h2.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            2.0,
            CLARINET_CONTOUR_H2_DB,
        ));
        self.contour_h3.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            3.0,
            CLARINET_CONTOUR_H3_DB,
        ));
        self.contour_h4.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            4.0,
            CLARINET_CONTOUR_H4_DB,
        ));
        self.contour_h5.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            5.0,
            CLARINET_CONTOUR_H5_DB,
        ));
        self.contour_h6.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            6.0,
            CLARINET_CONTOUR_H6_DB,
        ));
        self.contour_h7.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            7.0,
            CLARINET_CONTOUR_H7_DB,
        ));
        self.contour_upper
            .set_coefficients(BiquadCoefficients::peaking(
                self.sample_rate,
                CLARINET_CONTOUR_UPPER_HZ,
                CLARINET_CONTOUR_UPPER_Q,
                CLARINET_CONTOUR_UPPER_DB,
            ));
    }

    fn clarinet_contour_sample(&mut self, sample: f32) -> f32 {
        let sample = self.contour_h2.process(sample);
        let sample = self.contour_h3.process(sample);
        let sample = self.contour_h4.process(sample);
        let sample = self.contour_h5.process(sample);
        let sample = self.contour_h6.process(sample);
        let sample = self.contour_h7.process(sample);
        math::snap_to_zero(self.contour_upper.process(sample))
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

fn bore_frequency_hz(params: ReedTubeParams) -> f32 {
    let ratio = math::finite_clamp(
        params.register_mode_ratio,
        REGISTER_MODE_RATIO_MIN,
        REGISTER_MODE_RATIO_MAX,
        1.0,
    );
    math::finite_or(params.frequency_hz / ratio, params.frequency_hz).max(1.0)
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

fn harmonic_cut(
    sample_rate: f32,
    frequency_hz: f32,
    harmonic: f32,
    gain_db: f32,
) -> BiquadCoefficients {
    let cutoff_hz = math::finite_clamp(
        frequency_hz * harmonic,
        20.0,
        sample_rate * 0.45,
        frequency_hz,
    );
    BiquadCoefficients::peaking(sample_rate, cutoff_hz, CLARINET_CONTOUR_Q, gain_db)
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
