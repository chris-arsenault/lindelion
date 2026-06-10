use lindelion_dsp_utils::{filters::BiquadCoefficients, math, soft_saturate};

use crate::{
    DISPERSION_DEFAULT, LOOP_FILTER_CUTOFF_DEFAULT_HZ, LOOP_FILTER_RESONANCE_DEFAULT,
    LOOP_GAIN_DEFAULT, LOWEST_STRING_FREQUENCY_HZ, PICKUP_POSITION_DEFAULT,
    STRIKE_POSITION_DEFAULT,
    body::{BodyFamily, ReducedBody, StringBodyMode},
    bow, core, dispersion,
    driver::BowContactDrive,
    traveling::{BoundaryFilters, BoundarySamples, BoundarySide, TravelingWavePair},
};

/// String output blend. The String output is the body's radiated motion summed with
/// a tap of the string at the pickup position. The pickup tap carries the bulk of
/// the level, so the String sits at the same loudness as the Modal/Tube/Mesh
/// resonators (the body's direct radiation alone is ~10x quieter than the old pickup
/// EQ) and `pickup_position` stays a material control; the body radiation adds the
/// body's own voice on top. Crucially, the two-way bridge loading colours *both*
/// terms — it acts inside the loop the pickup reads — so the body's faster decay of
/// near-mode partials survives even in the pickup tap, not just the radiated term.
/// `STRING_OUTPUT_TRIM` then matches the summed level to the pre-M7 String, keeping
/// the same peak headroom (loud plucks do not clip any more than before).
const STRING_PICKUP_MIX: f32 = 1.0;
const STRING_BODY_MIX: f32 = 1.0;
const STRING_OUTPUT_TRIM: f32 = 0.85;
/// A bowed acoustic string is heard through bridge/body radiation only: the
/// pickup tap is a useful direct-string/electric observation for plucks, but
/// during active bowing it exposes the contact discontinuity as a dry signal a
/// violin body would not radiate directly, so it is muted while the bow driver
/// is engaged. The body's modal admittance is calibrated as *coloration on top
/// of the pickup* (`BODY_GAIN_SCALE`), roughly 1/10 of instrument level, so the
/// radiated path needs its own absolute calibration when it carries the whole
/// bowed voice. This weight replaces the pluck-path balance crossfade while the
/// bow is engaged and is calibrated so the default bowed violin sustains at
/// instrument level (guarded by the bowed-audibility test). If no body is
/// enabled the pickup tap remains the fallback, so valid bow configurations
/// never go silent.
const BOW_BODY_RADIATION_WEIGHT: f32 = 20.0;

/// Energy-dependent source↔body balance (M9). Depth does not replace the direct
/// string with a separate resonator; it changes how much of the already-coupled
/// system is heard from the pickup/source tap versus the radiated soundboard.
/// Depth 0 is pickup-forward with a light body contribution. Depth 1 is body-
/// forward but keeps part of the source tap so the note stays string-like. A
/// smaller measured-energy term adds extra soundboard bloom on hard strikes.
const STRING_BODY_BALANCE_GAIN: f32 = 18.0;
const STRING_PICKUP_BALANCE_DUCK: f32 = 0.68;
const STRING_BODY_BLOOM_GAIN: f32 = 3.5;
const STRING_PICKUP_BLOOM_DUCK: f32 = 0.08;
/// Measured-energy (RMS) that maps to the bright (body) end of the balance crossfade
/// (`e = 1`); the linear, clamped `energy/REF` keeps soft/medium dynamics on the warm
/// pickup tap and reserves the blooming body-radiation end for loud playing. M11 P8:
/// calibrated to the measured per-voice energy bus (a full-velocity String pluck peaks
/// near RMS 0.010), so soft→warm/loud→bright spans the real dynamic range; the old 0.3
/// left the crossfade pinned at its base position for all real playing (inaudible).
const STRING_BALANCE_ENERGY_REF: f32 = 0.012;

/// Half-width of the finite bow ribbon as a fraction of the speaking length.
const BOW_CONTACT_HALF_WIDTH: f32 = 0.012;

/// Mean-square wave amplitude at which tension modulation reaches full drive.
/// Tension rise is quadratic in vibration amplitude (mean-square string slope),
/// and by equipartition the time-averaged square of the boundary samples tracks
/// the string's stored energy, so the drive is *linear* in this estimator.
/// Calibrated so a full-velocity pluck peaks near the M11 P8 target (≈0.6–0.8
/// drive, ≈25–30 cents) and decays back to nominal as the note rings down: a
/// full shaped pluck measures ≈0.24 mean-square at the boundaries.
const STRING_TENSION_WAVE_ENERGY_REF: f32 = 0.30;
/// Smoothing time of the string-energy estimator: a few fundamental periods, so
/// the tension drive follows the note envelope rather than individual waves.
const STRING_ENERGY_SMOOTHING_SECONDS: f32 = 0.05;
/// Fractional one-way-delay shortening at full drive. `2^(40/1200) - 1 ≈ 0.0234`
/// gives ≈ +40 cents of transient pitch-sharpening at a hard pluck's peak energy.
const STRING_TENSION_DEPTH: f32 = 0.0234;
/// Clamp on the normalized squared drive, capping the peak sharpening near +40
/// cents and keeping the modulated delay bounded well within the wave buffer.
const STRING_TENSION_MAX_DRIVE: f32 = 1.0;

/// Material operators derived from non-note identity controls. Normal played
/// pitch moves the waveguide's current physical length, so it is intentionally
/// kept out of this cache key.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PreparedStringModel {
    loop_material: core::LoopMaterial,
    geometry: core::WaveguideGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CurrentStringOperators {
    dispersion_profile: dispersion::DispersionProfile,
    one_way_delay: f32,
    reflection_gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringModel {
    sample_rate: f32,
    waves: TravelingWavePair,
    terminations: BoundaryFilters,
    left_dispersion: dispersion::WaveguideDispersion,
    right_dispersion: dispersion::WaveguideDispersion,
    // Reduced modal body coupled two-way at the bridge (M7): its admittance loads
    // the bridge reflection (so partials near body modes decay faster) and its
    // radiated motion is the string output, replacing the heuristic pickup EQ.
    body: ReducedBody,
    body_mode: StringBodyMode,
    /// Test hook scaling the two-way bridge loading (1.0 normal, 0.0 = body still
    /// radiates but does not load the loop) to isolate the two-way effect from the
    /// body's output coloration.
    body_coupling_scale: f32,
    prepared: Option<(StringMaterialParams, PreparedStringModel)>,
    // Per-sample smoothing of continuous physical inputs so a control-rate jump
    // de-zippers without replacing the persistent waveguide.
    loop_gain: core::ScalarSmoother,
    loop_filter_cutoff: core::ScalarSmoother,
    loop_filter_resonance: core::ScalarSmoother,
    dispersion: core::ScalarSmoother,
    // Current speaking length/tuning of the persistent string, represented as
    // uncompensated one-way delay. Note events move this state toward a target;
    // they do not rebuild the waveguide.
    tuning_delay: core::ScalarSmoother,
    // Smoothed mean-square of the boundary wave samples: the string's own stored
    // energy, driving tension modulation. Internal physical state (replaces the
    // old output-RMS energy bus, which was pluck-calibrated and detuned any
    // sustained driver); updated every (oversampled) sample.
    string_energy: f32,
    string_energy_coefficient: f32,
    // Strike-position spread (M9 contact stage), `0..1`; set each (oversampled)
    // sample from `StringModelParams::excitation_spread`. 0.0 => the narrow pre-M9
    // contact (cached taps); positive widens the injection toward a strum.
    excitation_spread: f32,
    // Source↔body balance depth (M9), `0..1`; set from `source_body_balance`. 0.0 =>
    // the pre-M9 fixed pickup/body blend. Kept out of `String1dParams` (the cache
    // key) since it only weights the output.
    balance_depth: f32,
    // Normalised measured-energy target for the balance crossfade, set per host
    // sample; smoothed per sample so the energy-dependent mix does not zipper.
    balance_energy_target: f32,
    balance_energy: core::ScalarSmoother,
    // The string's returning wave at the bridge (driven end), cached each sample so
    // a physical driver (M8) can read the input-end wave it couples to.
    bridge_incident: f32,
    // Bowed-contact branch memory for the Friedlander/MSW hysteresis rule (see
    // `bow`): whether the contact stuck last sample and the slide direction.
    // `bow_force` is the probe/diagnostic value of the last solved contact force.
    bow_force: f32,
    bow_sticking: bool,
    bow_slip_direction: f32,
    // FIFO delaying the upstream incoming-wave reads to the contact (see
    // `bow::BOW_READ_ADVANCE_SAMPLES`): per-rail ring of the advanced reads.
    bow_incoming_left: [f32; bow::BOW_READ_ADVANCE_SAMPLES],
    bow_incoming_right: [f32; bow::BOW_READ_ADVANCE_SAMPLES],
    bow_incoming_index: usize,
    // Rosin-noise source for the sliding friction (see `bow::BOW_SLIP_NOISE_*`):
    // a deterministic xorshift32 state and a one-pole lowpass, seeded at
    // construction/reset so renders and tests are reproducible.
    bow_noise_state: u32,
    bow_noise_lp: f32,
    bow_noise_coefficient: f32,
    // Slow mean of the contact force for the torsional low-frequency relief
    // (see `bow::BOW_TORSION_RELIEF_SECONDS`).
    bow_torsion_mean_force: f32,
    bow_torsion_coefficient: f32,
    // Intonation servo state (see `bow::BOW_TUNE_SERVO_SECONDS`): capture
    // intervals measure the sounding period; the delay correction trims the
    // tuning state toward the played target like a player's finger.
    bow_samples_since_capture: f32,
    bow_last_capture_interval: f32,
    bow_tune_target: f32,
    bow_tune_correction: f32,
    bow_tune_coefficient: f32,
    bow_tune_release_coefficient: f32,
    #[cfg(test)]
    recompute_count: u32,
}

/// Deterministic xorshift seed for the rosin-noise source.
const BOW_NOISE_SEED: u32 = 0x9E37_79B9;

impl StringModel {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            waves: TravelingWavePair::new(sample_rate, LOWEST_STRING_FREQUENCY_HZ, 2.0),
            terminations: BoundaryFilters::new(),
            left_dispersion: dispersion::WaveguideDispersion::new(),
            right_dispersion: dispersion::WaveguideDispersion::new(),
            body: ReducedBody::new(sample_rate, BodyFamily::Guitar),
            body_mode: StringBodyMode::Guitar,
            body_coupling_scale: 1.0,
            prepared: None,
            loop_gain: core::ScalarSmoother::new(sample_rate),
            loop_filter_cutoff: core::ScalarSmoother::new(sample_rate),
            loop_filter_resonance: core::ScalarSmoother::new(sample_rate),
            dispersion: core::ScalarSmoother::new(sample_rate),
            tuning_delay: core::ScalarSmoother::new(sample_rate),
            string_energy: 0.0,
            string_energy_coefficient: math::finite_clamp(
                1.0 / (STRING_ENERGY_SMOOTHING_SECONDS * sample_rate),
                0.0,
                1.0,
                1.0,
            ),
            excitation_spread: 0.0,
            balance_depth: 0.0,
            balance_energy_target: 0.0,
            balance_energy: core::ScalarSmoother::new(sample_rate),
            bridge_incident: 0.0,
            bow_force: 0.0,
            bow_sticking: false,
            bow_slip_direction: 0.0,
            bow_incoming_left: [0.0; bow::BOW_READ_ADVANCE_SAMPLES],
            bow_incoming_right: [0.0; bow::BOW_READ_ADVANCE_SAMPLES],
            bow_incoming_index: 0,
            bow_noise_state: BOW_NOISE_SEED,
            bow_noise_lp: 0.0,
            bow_noise_coefficient: math::finite_clamp(
                1.0 - (-std::f32::consts::TAU * bow::BOW_SLIP_NOISE_BANDWIDTH_HZ / sample_rate)
                    .exp(),
                0.0,
                1.0,
                1.0,
            ),
            bow_torsion_mean_force: 0.0,
            bow_torsion_coefficient: math::finite_clamp(
                1.0 / (bow::BOW_TORSION_RELIEF_SECONDS * sample_rate),
                0.0,
                1.0,
                1.0,
            ),
            bow_samples_since_capture: 0.0,
            bow_last_capture_interval: 0.0,
            bow_tune_target: 1.0,
            bow_tune_correction: 1.0,
            bow_tune_coefficient: math::finite_clamp(
                1.0 / (bow::BOW_TUNE_SERVO_SECONDS * sample_rate),
                0.0,
                1.0,
                1.0,
            ),
            bow_tune_release_coefficient: math::finite_clamp(
                1.0 / (bow::BOW_TUNE_RELEASE_SECONDS * sample_rate),
                0.0,
                1.0,
                1.0,
            ),
            #[cfg(test)]
            recompute_count: 0,
        }
    }

    /// The string's returning wave at the bridge (the M8 driver feedback seam).
    /// Cached from the previous `process_sample`; 0.0 before the first sample.
    pub fn driven_feedback(&self) -> f32 {
        self.bridge_incident
    }

    /// Set the measured-energy drive for the source↔body balance (M9 energy bus),
    /// normalised against the balance reference. Called once per host sample by the
    /// resonator engine alongside `set_tension_drive`; defaults to 0.0 (full-body end
    /// of the crossfade), which is inert when the balance depth is 0.
    pub fn set_balance_drive(&mut self, drive: f32) {
        let energy = math::finite_or(drive, 0.0).max(0.0);
        self.balance_energy_target =
            math::finite_clamp(energy / STRING_BALANCE_ENERGY_REF, 0.0, 1.0, 0.0);
    }

    pub fn reset(&mut self) {
        self.waves.clear();
        self.terminations.reset();
        self.left_dispersion.reset();
        self.right_dispersion.reset();
        self.body.reset();
        self.prepared = None;
        self.loop_gain.reset();
        self.loop_filter_cutoff.reset();
        self.loop_filter_resonance.reset();
        self.dispersion.reset();
        self.tuning_delay.reset();
        self.string_energy = 0.0;
        self.excitation_spread = 0.0;
        self.balance_depth = 0.0;
        self.balance_energy_target = 0.0;
        self.balance_energy.reset();
        self.bridge_incident = 0.0;
        self.clear_bow_contact_state();
        self.bow_tune_target = 1.0;
        self.bow_tune_correction = 1.0;
        self.body_mode = StringBodyMode::Guitar;
    }

    /// Next rosin-noise sample: deterministic xorshift32 white noise through a
    /// one-pole lowpass (`bow::BOW_SLIP_NOISE_BANDWIDTH_HZ`), roughly ±0.5.
    fn next_bow_noise(&mut self) -> f32 {
        let mut state = self.bow_noise_state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.bow_noise_state = state;
        let white = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        self.bow_noise_lp += self.bow_noise_coefficient * (white - self.bow_noise_lp);
        self.bow_noise_lp
    }

    /// Production entry point for the shared two-rail string model.
    pub fn process(&mut self, excitation: f32, params: StringModelParams) -> f32 {
        self.process_with_bow_contact(excitation, params, None)
    }

    pub fn process_with_bow_contact(
        &mut self,
        excitation: f32,
        params: StringModelParams,
        bow_contact: Option<BowContactDrive>,
    ) -> f32 {
        self.process_with_bow_contact_and_probe(excitation, params, bow_contact, None)
    }

    pub fn process_with_bow_contact_probe(
        &mut self,
        excitation: f32,
        params: StringModelParams,
        bow_contact: Option<BowContactDrive>,
    ) -> (f32, StringModelProbe) {
        let mut probe = StringModelProbe::default();
        let output = self.process_with_bow_contact_and_probe(
            excitation,
            params,
            bow_contact,
            Some(&mut probe),
        );
        (output, probe)
    }

    fn process_with_bow_contact_and_probe(
        &mut self,
        excitation: f32,
        params: StringModelParams,
        bow_contact: Option<BowContactDrive>,
        probe: Option<&mut StringModelProbe>,
    ) -> f32 {
        // `excitation_spread` (M9 contact stage) is consumed only at injection, so it
        // is stashed here rather than carried in `String1dParams` (the prepared-model
        // cache key) — a per-sample spread change never busts the heavy derivations.
        self.excitation_spread = math::finite_clamp(params.excitation_spread, 0.0, 1.0, 0.0);
        self.balance_depth = if params.switches.source_body_balance_enabled {
            math::finite_clamp(params.source_body_balance, 0.0, 1.0, 0.0)
        } else {
            0.0
        };
        self.process_sample_with_bow_contact(
            excitation,
            params.string_params(),
            params,
            bow_contact,
            probe,
        )
    }

    #[cfg(test)]
    fn process_sample(
        &mut self,
        excitation: f32,
        params: String1dParams,
        model_params: StringModelParams,
    ) -> f32 {
        self.process_sample_with_bow_contact(excitation, params, model_params, None, None)
    }

    /// Two-way bridge coupling (passive wave-digital termination): the body admittance loads the
    /// bridge reflection (|R| <= 1, dips at body modes, so those partials lose energy and decay
    /// faster — the loop loading a post-EQ cannot reproduce), and the body radiates the absorbed
    /// motion = output.
    fn coupled_bridge_reflection(
        &mut self,
        left_reflection: f32,
        model_params: StringModelParams,
    ) -> (f32, f32) {
        match model_params.body_mode.family() {
            Some(family) => {
                if self.body_mode != model_params.body_mode {
                    self.body.configure(family);
                    self.body_mode = model_params.body_mode;
                }
                let (body_reflected, radiated) = self.body.bridge(left_reflection);
                let coupling = if model_params.switches.body_contact_enabled {
                    self.body_coupling_scale
                } else {
                    0.0
                };
                (
                    math::snap_to_zero(
                        left_reflection + coupling * (body_reflected - left_reflection),
                    ),
                    radiated,
                )
            }
            None => {
                self.body_mode = StringBodyMode::Disabled;
                (left_reflection, 0.0)
            }
        }
    }

    /// String-energy estimator: smoothed mean square of the boundary samples. The waves passing
    /// the endpoints sample the whole string each round trip, so this tracks stored energy
    /// without an O(length) scan.
    fn track_string_energy(&mut self, boundary: BoundarySamples) {
        let boundary_square = boundary.left * boundary.left + boundary.right * boundary.right;
        self.string_energy = math::snap_to_zero(
            self.string_energy
                + self.string_energy_coefficient * (boundary_square - self.string_energy),
        );
    }

    fn process_sample_with_bow_contact(
        &mut self,
        excitation: f32,
        params: String1dParams,
        model_params: StringModelParams,
        bow_contact: Option<BowContactDrive>,
        probe: Option<&mut StringModelProbe>,
    ) -> f32 {
        let params = self.smoothed_params(params);
        let prepared = self.prepared_model(params.material_params());
        let current_frequency_hz = self.current_tuning_frequency(params.frequency_hz);
        let current_params = params.with_frequency(current_frequency_hz);
        let operators = self.current_operators(current_params, prepared);
        // Energy-dependent tension modulation: hard playing raises string tension,
        // shortening the effective delay and sharpening pitch transiently; as the
        // string's stored energy decays the delay returns to nominal (the
        // "bloom"). The drive reads the previous sample's energy state, one
        // sample behind the waves it modulates — negligible against the 50 ms
        // estimator smoothing.
        let tension_drive = if model_params.switches.tension_modulation_enabled {
            math::finite_clamp(
                self.string_energy / STRING_TENSION_WAVE_ENERGY_REF,
                0.0,
                STRING_TENSION_MAX_DRIVE,
                0.0,
            )
        } else {
            0.0
        };
        // The bowed-intonation servo trims the speaking length like a player's
        // finger (neutral 1.0 whenever the bow is not engaged).
        let one_way_delay = tension_modulated_delay(operators.one_way_delay, tension_drive)
            * math::finite_clamp(
                self.bow_tune_correction,
                bow::BOW_TUNE_FACTOR_MIN,
                bow::BOW_TUNE_FACTOR_MAX,
                1.0,
            );

        let boundary = self.waves.boundary_samples(one_way_delay);
        // Expose the string's returning wave at the bridge for the M8 driver feedback.
        self.bridge_incident = boundary.left;
        self.track_string_energy(boundary);

        let left_reflection = self.reflected_sample(
            boundary.left,
            operators.reflection_gain,
            BoundarySide::Left,
            current_params,
            operators.dispersion_profile,
        );
        let right_reflection = self.reflected_sample(
            boundary.right,
            operators.reflection_gain,
            BoundarySide::Right,
            current_params,
            operators.dispersion_profile,
        );

        let (coupled_left, radiated) =
            self.coupled_bridge_reflection(left_reflection, model_params);

        // Pickup tap of the string at the pickup position (read before the loop is
        // advanced, mirroring the pre-M7 timing). The loop it samples is already
        // loaded two-way by the body, so this tap carries the body's decay colour.
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);
        let pickup_tap = pickup.average();
        let bow_active = bow_contact.is_some();
        let expected_period = self.sample_rate / current_frequency_hz.max(1.0);
        let bow_correction = bow_contact
            .and_then(|drive| self.bow_contact_excitation(one_way_delay, expected_period, drive));
        let bow_wave_correction = bow_correction
            .as_ref()
            .map(|contact| contact.wave_correction)
            .unwrap_or(0.0);
        if bow_contact.is_none() {
            self.clear_bow_contact_state();
            self.relax_bow_tuning();
        }

        self.waves.push(right_reflection, coupled_left);
        self.apply_bow_correction(one_way_delay, bow_correction);
        // Strike-position spread (M9): a wide strum injects over a broader region
        // than a tight pick. Spread 0 uses the cached narrow taps (the pre-M9 fast
        // path); a positive spread rebuilds the wider window from the (cheap)
        // strike + half-width, leaving the prepared model untouched.
        let excitation_taps = if self.excitation_spread > 0.0 {
            core::excitation_taps(
                params.strike_position,
                core::excitation_half_width(self.excitation_spread),
            )
        } else {
            prepared.geometry.excitation_taps
        };
        self.waves.add_symmetric_excitation(
            one_way_delay,
            excitation_taps,
            math::snap_to_zero(excitation),
        );

        let (pickup_weight, body_weight) =
            self.output_weights(bow_active && model_params.body_mode.family().is_some());
        let weighted_pickup = STRING_OUTPUT_TRIM * pickup_weight * pickup_tap;
        let weighted_body = STRING_OUTPUT_TRIM * body_weight * radiated;
        let output = weighted_pickup + weighted_body;
        let output = math::snap_to_zero(output);
        if let Some(probe) = probe {
            *probe = StringModelProbe {
                pickup_tap,
                body_radiated: radiated,
                weighted_pickup,
                weighted_body,
                pickup_weight,
                body_weight,
                output,
                bow_force: self.bow_force,
                bow_wave_correction,
                current_frequency_hz,
                one_way_delay_samples: one_way_delay,
            };
        }
        output
    }

    /// Pickup/body output weights for this sample.
    ///
    /// Pluck path — source/body balance (M9): depth moves the listening point
    /// from a pickup-forward source tap toward the radiated soundboard output.
    /// Measured bridge energy adds transient bloom, but the static depth term
    /// carries the tail and repeated-strike body memory so the control remains
    /// audible after the attack.
    ///
    /// Bowed path: the radiated body carries the whole voice at its own
    /// calibration (see `BOW_BODY_RADIATION_WEIGHT`); the pluck-path balance
    /// crossfade does not apply while the bow is engaged.
    fn output_weights(&mut self, bow_radiated: bool) -> (f32, f32) {
        let energy = self.balance_energy.next(self.balance_energy_target);
        if bow_radiated {
            return (0.0, BOW_BODY_RADIATION_WEIGHT);
        }
        if self.balance_depth > 0.0 {
            let bloom = self.balance_depth * energy;
            let pickup_weight = STRING_PICKUP_MIX
                * (1.0
                    - STRING_PICKUP_BALANCE_DUCK * self.balance_depth
                    - STRING_PICKUP_BLOOM_DUCK * bloom);
            let body_weight = STRING_BODY_MIX
                * (1.0
                    + STRING_BODY_BALANCE_GAIN * self.balance_depth
                    + STRING_BODY_BLOOM_GAIN * bloom);
            (pickup_weight, body_weight)
        } else {
            (STRING_PICKUP_MIX, STRING_BODY_MIX)
        }
    }

    pub fn set_body_coupling_scale(&mut self, scale: f32) {
        self.body_coupling_scale = scale;
    }
}

mod bow_contact;
mod operators;
mod params;

use operators::tension_modulated_delay;
pub(crate) use params::String1dParams;
use params::StringMaterialParams;
pub use params::{StringModelParams, StringModelProbe, StringModelSwitches};

#[cfg(test)]
mod tests;
