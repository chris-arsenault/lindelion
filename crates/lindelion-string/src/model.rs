use lindelion_dsp_utils::{filters::BiquadCoefficients, math, soft_saturate};

use crate::{
    DISPERSION_DEFAULT, LOOP_FILTER_CUTOFF_DEFAULT_HZ, LOOP_FILTER_RESONANCE_DEFAULT,
    LOOP_GAIN_DEFAULT, LOWEST_STRING_FREQUENCY_HZ, PICKUP_POSITION_DEFAULT,
    STRIKE_POSITION_DEFAULT,
    body::{BodyFamily, ReducedBody, StringBodyMode},
    core, dispersion,
    traveling::{BoundaryFilters, BoundarySide, TravelingWavePair},
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
const STRING_BODY_MIX: f32 = 3.0;
const STRING_OUTPUT_TRIM: f32 = 0.85;

/// Energy-dependent source↔body balance (M9). The fixed pickup/body weights above
/// are replaced — when the balance depth is non-zero — by an **equal-power** crossfade
/// between the direct pickup tap and a **level-matched** body radiation, steered by
/// measured energy. In this voicing the loop-damped pickup tap is the *warm/rounded*
/// sustain voice and the body radiation — carrying its presence formant — is the
/// *bright/blooming* one, so the perceptually correct mapping (M11 P8) is **soft →
/// pickup (warm), loud → body (bright)**: harder playing drives the body's radiating
/// resonances and blooms brighter. The crossfade position `p ∈ [0,1]` (`p = 1` all
/// pickup, `p = 0` all body) maps to gains `S·sin(pπ/2)` (pickup) and
/// `S·cos(pπ/2)·LEVEL_MATCH` (body). Equal power (`sin² + cos² = 1`) on the
/// *level-matched* signals holds output level, so the change is timbral, not gain.
///
/// The body radiation is ~15–20× quieter than the pickup tap (ADR-0021), so it is
/// scaled up by `LEVEL_MATCH` before the crossfade — otherwise leaning to the body
/// would just go quiet instead of warm. `BASE_POSITION = atan(LEVEL_MATCH·PICKUP/BODY)
/// /(π/2)` and `WEIGHT_SCALE = 1/sin(BASE_POSITION·π/2)` are chosen so that at
/// `p = BASE_POSITION` the gains are exactly the pre-M9 `(1.0, 3.0)` blend; depth `0`
/// collapses to that fixed blend (identity guard).
///
/// `LEVEL_MATCH` is a first-principles value; the exact body/pickup level ratio (and
/// thus the crossfade calibration) is an M11 calibration target (ADR-0021, deferred
/// cross-resonator level / coupling-strength work).
const STRING_BODY_LEVEL_MATCH: f32 = 15.0;
const STRING_BALANCE_BASE_POSITION: f32 = 0.874_3; // atan(15·1/3)/(π/2)
const STRING_BALANCE_WEIGHT_SCALE: f32 = 1.019_9; // 1/sin(BASE_POSITION·π/2)
/// How far full energy swings the crossfade position around the base (at full depth):
/// `p = BASE + (0.5 − e)·SPAN·depth`, clamped to `[0, 1]`. Sized so a soft note settles
/// onto the warm pickup tap while a loud note blooms into the bright body radiation.
const STRING_BALANCE_SPAN: f32 = 1.0;
/// Measured-energy (RMS) that maps to the bright (body) end of the balance crossfade
/// (`e = 1`); the linear, clamped `energy/REF` keeps soft/medium dynamics on the warm
/// pickup tap and reserves the blooming body-radiation end for loud playing. M11 P8:
/// calibrated to the measured per-voice energy bus (a full-velocity String pluck peaks
/// near RMS 0.010), so soft→warm/loud→bright spans the real dynamic range; the old 0.3
/// left the crossfade pinned at its base position for all real playing (inaudible).
const STRING_BALANCE_ENERGY_REF: f32 = 0.012;

/// Measured-energy (RMS) at which the tension bloom reaches its target depth; the
/// squared, normalized drive `(energy/REF)^2` keeps low/medium dynamics in tune
/// and concentrates the sharpening on hard hits. M11 P8: calibrated to the measured
/// per-voice energy bus — a full-velocity String pluck peaks near RMS 0.010, so this
/// REF puts a hard hit at ≈0.7 drive (≈28 cents, approaching the +40 the depth allows)
/// and lets sustained bowing saturate; the old 0.15 left a hard pluck at 0.4% drive
/// (inaudible). The effect was always designed to reach drive 1.0 — only the REF kept
/// it from getting there.
const STRING_TENSION_ENERGY_REF: f32 = 0.012;
/// Fractional one-way-delay shortening at full drive. `2^(40/1200) - 1 ≈ 0.0234`
/// gives ≈ +40 cents of transient pitch-sharpening at a hard pluck's peak energy.
const STRING_TENSION_DEPTH: f32 = 0.0234;
/// Clamp on the normalized squared drive, capping the peak sharpening near +40
/// cents and keeping the modulated delay bounded well within the wave buffer.
const STRING_TENSION_MAX_DRIVE: f32 = 1.0;

/// Shorten the effective one-way delay as a function of measured energy
/// (tension modulation; Bank/Sujbert, Tolonen/Välimäki). The delay only ever
/// shortens (`drive >= 0`) and never below `one_way_delay / (1 + DEPTH*MAX)`, so
/// it stays bounded and within the fixed traveling-wave capacity; at `drive == 0`
/// it returns the nominal delay (tuning unaffected).
fn tension_modulated_delay(one_way_delay: f32, energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / STRING_TENSION_ENERGY_REF;
    let drive = math::finite_clamp(normalized * normalized, 0.0, STRING_TENSION_MAX_DRIVE, 0.0);
    one_way_delay / (1.0 + STRING_TENSION_DEPTH * drive)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct String1dParams {
    pub(crate) frequency_hz: f32,
    pub(crate) loop_filter_cutoff: f32,
    pub(crate) loop_filter_resonance: f32,
    pub(crate) loop_gain: f32,
    pub(crate) loop_nonlinearity: f32,
    pub(crate) dispersion: f32,
    pub(crate) strike_position: f32,
    pub(crate) pickup_position: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringModelSwitches {
    pub body_contact_enabled: bool,
    pub tension_modulation_enabled: bool,
    pub source_body_balance_enabled: bool,
}

impl Default for StringModelSwitches {
    fn default() -> Self {
        Self {
            body_contact_enabled: true,
            tension_modulation_enabled: true,
            source_body_balance_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StringModelParams {
    pub frequency_hz: f32,
    pub loop_filter_cutoff_hz: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub dispersion: f32,
    pub strike_position: f32,
    pub pickup_position: f32,
    pub excitation_spread: f32,
    pub source_body_balance: f32,
    pub body_mode: StringBodyMode,
    pub switches: StringModelSwitches,
}

impl Default for StringModelParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: LOOP_FILTER_CUTOFF_DEFAULT_HZ,
            loop_filter_resonance: LOOP_FILTER_RESONANCE_DEFAULT,
            loop_gain: LOOP_GAIN_DEFAULT,
            loop_nonlinearity: 0.0,
            dispersion: DISPERSION_DEFAULT,
            strike_position: STRIKE_POSITION_DEFAULT,
            pickup_position: PICKUP_POSITION_DEFAULT,
            excitation_spread: 0.0,
            source_body_balance: 0.0,
            body_mode: StringBodyMode::default(),
            switches: StringModelSwitches::default(),
        }
    }
}

impl StringModelParams {
    fn string_params(self) -> String1dParams {
        String1dParams {
            frequency_hz: self.frequency_hz,
            loop_filter_cutoff: self.loop_filter_cutoff_hz,
            loop_filter_resonance: self.loop_filter_resonance,
            loop_gain: self.loop_gain,
            loop_nonlinearity: self.loop_nonlinearity,
            dispersion: self.dispersion,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
        }
    }
}

/// Per-sample-invariant string operators derived from `String1dParams`. Cached
/// behind a params dirty-check so the heavy derivations (loop damping incl. the
/// filter-peak scan, dispersion profile, geometry, delay tuning) run at control
/// rate, not per sample. Candidate extraction (ADR-0003): single consumer today.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PreparedStringModel {
    dispersion_profile: dispersion::DispersionProfile,
    geometry: core::WaveguideGeometry,
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
    prepared: Option<(String1dParams, PreparedStringModel)>,
    // Per-sample smoothing of the continuous physical inputs so a control-rate
    // jump de-zippers; frequency and positions stay un-smoothed to keep tuning
    // and excitation timing exact.
    loop_gain: core::ScalarSmoother,
    loop_filter_cutoff: core::ScalarSmoother,
    loop_filter_resonance: core::ScalarSmoother,
    dispersion: core::ScalarSmoother,
    // Measured resonator energy (M2 bus) driving tension modulation; set per host
    // sample, constant across the 2x oversampled sub-samples. 0.0 => inert.
    tension_drive: f32,
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
    #[cfg(test)]
    recompute_count: u32,
}

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
            tension_drive: 0.0,
            excitation_spread: 0.0,
            balance_depth: 0.0,
            balance_energy_target: 0.0,
            balance_energy: core::ScalarSmoother::new(sample_rate),
            bridge_incident: 0.0,
            #[cfg(test)]
            recompute_count: 0,
        }
    }

    /// The string's returning wave at the bridge (the M8 driver feedback seam).
    /// Cached from the previous `process_sample`; 0.0 before the first sample.
    pub fn driven_feedback(&self) -> f32 {
        self.bridge_incident
    }

    /// Set the measured-energy drive for tension modulation (M2 energy bus).
    /// Called once per host sample by the resonator engine; defaults to 0.0 so
    /// callers that never set it render the linear string unchanged.
    pub fn set_tension_drive(&mut self, drive: f32) {
        self.tension_drive = math::finite_or(drive, 0.0).max(0.0);
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
        self.tension_drive = 0.0;
        self.excitation_spread = 0.0;
        self.balance_depth = 0.0;
        self.balance_energy_target = 0.0;
        self.balance_energy.reset();
        self.bridge_incident = 0.0;
        self.body_mode = StringBodyMode::Guitar;
    }

    /// Production entry point for the shared two-rail string model.
    pub fn process(&mut self, excitation: f32, params: StringModelParams) -> f32 {
        // `excitation_spread` (M9 contact stage) is consumed only at injection, so it
        // is stashed here rather than carried in `String1dParams` (the prepared-model
        // cache key) — a per-sample spread change never busts the heavy derivations.
        self.excitation_spread = math::finite_clamp(params.excitation_spread, 0.0, 1.0, 0.0);
        self.balance_depth = if params.switches.source_body_balance_enabled {
            math::finite_clamp(params.source_body_balance, 0.0, 1.0, 0.0)
        } else {
            0.0
        };
        self.process_sample(excitation, params.string_params(), params)
    }

    fn process_sample(
        &mut self,
        excitation: f32,
        params: String1dParams,
        model_params: StringModelParams,
    ) -> f32 {
        let params = self.smoothed_params(params);
        let prepared = self.prepared_model(params);
        // Energy-dependent tension modulation: a hard pluck raises string tension,
        // shortening the effective delay and sharpening pitch transiently; as the
        // measured energy decays the delay returns to nominal (the "bloom").
        let tension_drive = if model_params.switches.tension_modulation_enabled {
            self.tension_drive
        } else {
            0.0
        };
        let one_way_delay = tension_modulated_delay(prepared.one_way_delay, tension_drive);

        let boundary = self.waves.boundary_samples(one_way_delay);
        // Expose the string's returning wave at the bridge for the M8 driver feedback.
        self.bridge_incident = boundary.left;

        let left_reflection = self.reflected_sample(
            boundary.left,
            prepared.reflection_gain,
            BoundarySide::Left,
            params,
            prepared.dispersion_profile,
        );
        let right_reflection = self.reflected_sample(
            boundary.right,
            prepared.reflection_gain,
            BoundarySide::Right,
            params,
            prepared.dispersion_profile,
        );

        // Two-way bridge coupling (passive wave-digital termination): the body
        // admittance loads the bridge reflection (|R| <= 1, dips at body modes, so
        // those partials lose energy and decay faster — the loop loading a post-EQ
        // cannot reproduce), and the body radiates the absorbed motion = output.
        let (coupled_left, radiated) = match model_params.body_mode.family() {
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
        };

        // Pickup tap of the string at the pickup position (read before the loop is
        // advanced, mirroring the pre-M7 timing). The loop it samples is already
        // loaded two-way by the body, so this tap carries the body's decay colour.
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);
        let pickup_tap = pickup.average();

        self.waves.push(right_reflection, coupled_left);
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

        // Energy-dependent source↔body balance (M9). At depth 0 the weights are the
        // pre-M9 fixed (pickup, body) blend (bit-exact identity); otherwise an
        // equal-power crossfade steered by the smoothed measured energy leans the
        // output to the warm, loop-damped pickup tap at low dynamics and the bright,
        // formant-bearing body radiation at high dynamics — so harder playing blooms
        // brighter (M11 P8 polarity fix), holding level (the change is timbral, not gain).
        let energy = self.balance_energy.next(self.balance_energy_target);
        let (pickup_weight, body_weight) = if self.balance_depth > 0.0 {
            let position = (STRING_BALANCE_BASE_POSITION
                + self.balance_depth * (0.5 - energy) * STRING_BALANCE_SPAN)
                .clamp(0.0, 1.0);
            let angle = position * std::f32::consts::FRAC_PI_2;
            (
                STRING_BALANCE_WEIGHT_SCALE * angle.sin(),
                STRING_BALANCE_WEIGHT_SCALE * angle.cos() * STRING_BODY_LEVEL_MATCH,
            )
        } else {
            (STRING_PICKUP_MIX, STRING_BODY_MIX)
        };
        let output = STRING_OUTPUT_TRIM * (pickup_weight * pickup_tap + body_weight * radiated);
        math::snap_to_zero(output)
    }

    pub fn set_body_coupling_scale(&mut self, scale: f32) {
        self.body_coupling_scale = scale;
    }

    /// Smooth the continuous physical inputs toward their targets, leaving
    /// frequency and the strike/pickup positions untouched so tuning and
    /// excitation timing track the requested values exactly.
    fn smoothed_params(&mut self, params: String1dParams) -> String1dParams {
        String1dParams {
            loop_gain: self.loop_gain.next(params.loop_gain),
            loop_filter_cutoff: self.loop_filter_cutoff.next(params.loop_filter_cutoff),
            loop_filter_resonance: self
                .loop_filter_resonance
                .next(params.loop_filter_resonance),
            dispersion: self.dispersion.next(params.dispersion),
            ..params
        }
    }

    /// Return the cached string operators, re-deriving them (and re-arming the
    /// termination filter coefficients) only when the incoming params have moved.
    fn prepared_model(&mut self, params: String1dParams) -> PreparedStringModel {
        if let Some((cached_params, prepared)) = self.prepared
            && cached_params == params
        {
            return prepared;
        }

        let damping = core::loop_damping(self.sample_rate, params);
        let dispersion_profile = dispersion::dispersion_profile(self.sample_rate, params);
        let geometry = core::waveguide_geometry(params.strike_position, params.pickup_position);
        let tuning = core::delay_tuning(
            self.sample_rate,
            self.waves.capacity(),
            params.frequency_hz,
            2.0,
            // The loop filter is applied at one termination only (once per round
            // trip), so it contributes half its group delay per one-way pass.
            1.0 + 0.5 * damping.filter_delay_samples
                + dispersion_profile.delay_compensation_samples,
        );
        let one_way_delay = tuning.integer_delay + tuning.fractional_delay;

        // Apply the loop filter at a single termination (once per round trip): the
        // gain compensation in `loop_damping` divides out one filter peak, so a
        // second pass would make the resonant round-trip gain exceed unity.
        self.terminations
            .set_coefficients(damping.coefficients, BiquadCoefficients::identity());

        let prepared = PreparedStringModel {
            dispersion_profile,
            geometry,
            one_way_delay,
            reflection_gain: core::endpoint_reflection_gain(damping.loop_gain),
        };
        self.prepared = Some((params, prepared));
        #[cfg(test)]
        {
            self.recompute_count += 1;
        }
        prepared
    }

    fn reflected_sample(
        &mut self,
        input: f32,
        reflection_gain: f32,
        side: BoundarySide,
        params: String1dParams,
        dispersion_profile: dispersion::DispersionProfile,
    ) -> f32 {
        let filtered = self.terminations.process(side, input);
        let loop_nonlinearity = math::finite_clamp(params.loop_nonlinearity, 0.0, 1.0, 0.0);
        let nonlinear = if loop_nonlinearity > 0.0 {
            soft_saturate(filtered, loop_nonlinearity)
        } else {
            filtered
        };
        let dispersed = match side {
            BoundarySide::Left => self
                .left_dispersion
                .process_sample(nonlinear, dispersion_profile),
            BoundarySide::Right => self
                .right_dispersion
                .process_sample(nonlinear, dispersion_profile),
        };
        math::snap_to_zero(-dispersed * reflection_gain)
    }
}

#[cfg(test)]
mod tests;
