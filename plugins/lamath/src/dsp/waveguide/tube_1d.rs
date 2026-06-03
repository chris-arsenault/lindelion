use lindelion_dsp_utils::{
    delay::FirstOrderAllpass,
    filters::{Biquad, BiquadCoefficients},
    math, soft_saturate,
};

use super::{
    WaveguideParams, WaveguideStyle,
    body::WaveguideBody,
    core,
    traveling::{BoundaryFilters, BoundarySide, PickupSamples, TravelingWavePair},
};
use crate::dsp::constants::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY};

const MOUTH_REFLECTION: f32 = -0.36;
const MIN_END_REFLECTION_MAGNITUDE: f32 = 0.08;

/// Measured-energy (RMS) at which bore steepening reaches its target depth; the
/// squared, normalized term `(energy/REF)^2` keeps soft/medium bores mellow and
/// concentrates the brassy brightening on loud playing. M11 P8: calibrated to the
/// measured per-voice energy bus — a full-velocity Tube note peaks near RMS 0.004, so
/// this REF puts loud playing at ≈0.6 drive (and sustained reed/breath driving into the
/// cuivré bloom); the old 0.15 left a loud note at ≈0.07% drive (no brass at all).
const STEEPEN_ENERGY_REF: f32 = 0.005;
/// Clamp on the normalized squared energy term (the steepening depth at peak
/// energy). 1.0 is the strong cuivré bloom chosen for M5.
const STEEPEN_MAX_ENERGY: f32 = 1.0;
/// Maximum dispersion-allpass coefficient at full steepening. The coefficient is
/// driven by the instantaneous wave amplitude (so it varies within a cycle —
/// the nonlinearity that generates upper harmonics) scaled by measured energy.
const STEEPEN_MAX_COEFF: f32 = 0.9;
/// Maps the instantaneous wave amplitude to the [0,1] amplitude factor: the
/// high-pressure crests get the most dispersion, sharpening the wavefronts.
const STEEPEN_AMPLITUDE_SENS: f32 = 10.0;
/// Bell radiation: the high-frequency content the bore transmits (radiates) out
/// the bell rather than reflecting back. The steepening harmonics live here, so
/// radiating them (energy-gated) is what the listener hears as brass.
const RADIATION_CUTOFF_HZ: f32 = 500.0;
const RADIATION_GAIN: f32 = 2.5;

/// Squared, normalized energy term in `[0, STEEPEN_MAX_ENERGY]` setting how much
/// the bore steepens at the current playing energy.
fn steepening_energy(energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / STEEPEN_ENERGY_REF;
    math::finite_clamp(normalized * normalized, 0.0, STEEPEN_MAX_ENERGY, 0.0)
}

/// Per-sample-invariant bore operators derived from the style-normalized
/// `WaveguideParams`. Cached behind a params dirty-check so the heavy derivations
/// (loop damping incl. the filter-peak scan, bore profile, geometry, phase-delay
/// compensated delay tuning) run at control rate, not per sample. Candidate
/// extraction (ADR-0003): single consumer today.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PreparedTubeModel {
    profile: TubeBoreProfile,
    geometry: core::WaveguideGeometry,
    one_way_delay: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tube1d {
    sample_rate: f32,
    waves: TravelingWavePair,
    boundary_filters: BoundaryFilters,
    body: WaveguideBody,
    prepared: Option<(WaveguideParams, PreparedTubeModel)>,
    // Per-sample smoothing of the continuous physical inputs so a control-rate
    // jump de-zippers; frequency and positions stay un-smoothed to keep tuning
    // and excitation timing exact.
    loop_gain: core::ScalarSmoother,
    loop_filter_cutoff: core::ScalarSmoother,
    loop_filter_resonance: core::ScalarSmoother,
    boundary_reflection: core::ScalarSmoother,
    // Reed-driven bore: glide the played frequency so a note change slides the delay length
    // (a wind portamento) instead of snapping it, which clicks. Snaps on the first note (the
    // smoother initializes to its first target), so a tongued note after a gap gets at most a
    // brief attack scoop while a slurred note glides cleanly.
    frequency: core::ScalarSmoother,
    // Measured resonator energy (M2 bus) driving finite-amplitude bore steepening;
    // set per host sample, constant across the 2x oversampled sub-samples. 0.0 => inert.
    steepening_drive: f32,
    // Amplitude-dependent dispersion stage applied once per round trip at the
    // mouth: unity magnitude (loop-stable, sustain preserved), nonlinear
    // amplitude-driven coefficient generates the brassy upper harmonics.
    steepening_allpass: FirstOrderAllpass,
    // Bell radiation: highpass on the bell-incident wave, radiating the
    // steepening harmonics to the output (energy-gated) instead of the model
    // discarding them as loop loss.
    radiation_highpass: Biquad,
    // The bore's returning wave at the mouth (driven end), cached each sample so a
    // physical driver (M8 reed) can read the actual input-end traveling wave it
    // couples to — the energy-bearing feedback a self-oscillating reed needs.
    mouth_incident: f32,
    #[cfg(test)]
    recompute_count: u32,
}

impl Tube1d {
    pub(super) fn new(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            waves: TravelingWavePair::new(sample_rate, lowest_frequency_hz, 4.0),
            boundary_filters: BoundaryFilters::new(),
            body: WaveguideBody::new(sample_rate),
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

    /// The bore's returning wave at the mouth (the M8 driver feedback seam): the
    /// energy-bearing input-end wave a reed couples to. Cached from the previous
    /// `process_sample`; 0.0 before the first sample.
    pub(super) fn driven_feedback(&self) -> f32 {
        self.mouth_incident
    }

    /// Set the measured-energy drive for finite-amplitude bore steepening (M2
    /// energy bus). Called once per host sample by the resonator engine; defaults
    /// to 0.0 so callers that never set it render the linear bore unchanged.
    pub(super) fn set_steepening_drive(&mut self, drive: f32) {
        self.steepening_drive = math::finite_or(drive, 0.0).max(0.0);
    }

    /// Effort-referenced bore brightness (ADR-0032 item B). A real bore brightens with **blowing
    /// pressure**; the reed-driven Tube therefore drives the finite-amplitude bore steepening from
    /// the player effort, not the measured-energy bus. Measurement showed the energy proxy is
    /// nearly blowing-pressure-invariant (the beating-reed limit-cycle amplitude saturates ~5 dB),
    /// so energy-gated steepening could not produce brightness-with-effort. Mapping effort through
    /// the energy reference (`drive = REF·effort`) makes `steepening_energy == effort²` — soft
    /// playing stays mellow, hard blowing blooms into the cuivré, reusing the existing physical
    /// steepening + bell-radiation machinery. Set once per host sample by the reed wind path.
    pub(super) fn set_brightness_effort(&mut self, effort: f32) {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        self.steepening_drive = STEEPEN_ENERGY_REF * effort;
    }

    pub(super) fn reset(&mut self) {
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

    pub(super) fn process_sample(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        let params = self.smoothed_params(WaveguideParams {
            style: WaveguideStyle::Tube,
            ..params
        });
        // `excitation_spread` (M9) only shapes injection, so strip it from the
        // prepared-model cache key: a per-sample spread change must not bust the
        // heavy bore derivations (Tube caches on the whole `WaveguideParams`).
        let excitation_spread = math::finite_clamp(params.excitation_spread, 0.0, 1.0, 0.0);
        // `source_body_balance` is String-only; strip it too so it never enters the
        // Tube cache key.
        let prepared = self.prepared_model(WaveguideParams {
            excitation_spread: 0.0,
            source_body_balance: 0.0,
            bell_radiation: 1.0,
            ..params
        });
        let profile = prepared.profile;
        let one_way_delay = prepared.one_way_delay;

        let boundary = self.waves.boundary_samples(one_way_delay);
        // Expose the bore's returning wave at the mouth for the M8 driver feedback.
        self.mouth_incident = boundary.left;
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);
        let mouth_reflection =
            self.reflected_sample(BoundarySide::Left, boundary.left, profile, params);
        let end_reflection =
            self.reflected_sample(BoundarySide::Right, boundary.right, profile, params);

        self.waves.push(end_reflection, mouth_reflection);
        // Strike-position spread (M9): widen the injection window for a strum, using
        // the cached narrow taps when spread is 0 (the pre-M9 fast path).
        let excitation_taps = if excitation_spread > 0.0 {
            core::excitation_taps(
                params.position_of_strike,
                core::excitation_half_width(excitation_spread),
            )
        } else {
            prepared.geometry.excitation_taps
        };
        self.waves.add_symmetric_excitation(
            one_way_delay,
            excitation_taps,
            math::snap_to_zero(excitation) * profile.excitation_coupling,
        );

        let body = self
            .body
            .process_sample(profile.pickup_sample(pickup), params);

        // Bell radiation: the bore transmits (radiates) its high-frequency content
        // out the bell rather than reflecting it back. The in-loop steepening's
        // harmonics ride the bell-incident wave, so radiate them to the output —
        // energy-gated so a soft bore is unchanged and a loud bore turns brassy.
        let radiated = self.radiation_highpass.process(boundary.right)
            * RADIATION_GAIN
            * steepening_energy(self.steepening_drive)
            * params.bell_radiation;

        math::snap_to_zero(body + radiated)
    }

    /// Wind-driven bore: a reed (or lip) valve **terminates the mouth**. `mouth_wave` is
    /// the reed's scattered outgoing wave (computed from this bore's `driven_feedback`),
    /// so it replaces the bore's passive mouth reflection entirely — the reed *is* the
    /// boundary. The mouth loss filter stays in the loop (the delay tuning compensates its
    /// phase) and the energy-driven steepening still brightens loud playing, but the fixed
    /// `−0.36` reflection and the strike-position injection are gone: one consistent mouth
    /// junction, so the loop locks to the bore's tuned fundamental instead of fighting two
    /// terminations (ADR-0032).
    pub(super) fn process_sample_wind(&mut self, mouth_wave: f32, params: WaveguideParams) -> f32 {
        let params = self.smoothed_params(WaveguideParams {
            style: WaveguideStyle::Tube,
            ..params
        });
        let prepared = self.prepared_model(WaveguideParams {
            excitation_spread: 0.0,
            source_body_balance: 0.0,
            bell_radiation: 1.0,
            ..params
        });
        let profile = prepared.profile;
        let one_way_delay = prepared.one_way_delay;

        let boundary = self.waves.boundary_samples(one_way_delay);
        // Expose the bore's returning wave at the mouth for the reed's next-sample junction.
        self.mouth_incident = boundary.left;
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, prepared.geometry.pickup_position);

        // The reed's scattered wave is the outgoing mouth wave: filter it through the mouth
        // loss (kept in the loop so the tuning holds) and the energy-driven brassy steepening,
        // but apply *no* passive reflection (the reed already reflected the bore's returning
        // wave) and *no* static saturation — the reed is the loop nonlinearity, and stacking
        // the static `loop_nonlinearity` saturation on top destabilises it into period-doubling.
        // Brassiness on loud playing comes from the energy-driven steepening (the cuivré bloom).
        let filtered = self
            .boundary_filters
            .process(BoundarySide::Left, mouth_wave);
        let mouth_reflection = math::snap_to_zero(self.apply_steepening(filtered));
        let end_reflection =
            self.reflected_sample(BoundarySide::Right, boundary.right, profile, params);

        // No strike-position excitation: the reed is the mouth source.
        self.waves.push(end_reflection, mouth_reflection);

        let body = self
            .body
            .process_sample(profile.pickup_sample(pickup), params);
        let radiated = self.radiation_highpass.process(boundary.right)
            * RADIATION_GAIN
            * steepening_energy(self.steepening_drive)
            * params.bell_radiation;

        math::snap_to_zero(body + radiated)
    }

    /// Smooth the continuous physical inputs toward their targets, leaving
    /// frequency and the strike/pickup positions untouched so tuning and
    /// excitation timing track the requested values exactly.
    fn smoothed_params(&mut self, params: WaveguideParams) -> WaveguideParams {
        WaveguideParams {
            loop_gain: self.loop_gain.next(params.loop_gain),
            loop_filter_cutoff: self.loop_filter_cutoff.next(params.loop_filter_cutoff),
            loop_filter_resonance: self
                .loop_filter_resonance
                .next(params.loop_filter_resonance),
            boundary_reflection: self.boundary_reflection.next(params.boundary_reflection),
            frequency_hz: self.frequency.next(params.frequency_hz),
            ..params
        }
    }

    /// Return the cached bore operators, re-deriving them (and re-arming the
    /// boundary filter coefficients) only when the incoming params have moved.
    /// `params` must already be style-normalized to `Tube`.
    fn prepared_model(&mut self, params: WaveguideParams) -> PreparedTubeModel {
        if let Some((cached_params, prepared)) = self.prepared
            && cached_params == params
        {
            return prepared;
        }

        let damping = core::loop_damping(self.sample_rate, params);
        let profile = TubeBoreProfile::from_params(self.sample_rate, params, damping.loop_gain);
        let geometry = core::waveguide_geometry(params.position_of_strike, params.pickup_position);
        // The bore round trip passes through two distinct boundary filters once
        // each — the mouth lowpass (left) and the damping lowpass (right). Loop
        // resonance is set by accumulated phase, so compensate each filter's
        // phase delay at the played pitch (not group delay, which drifts as the
        // pitch nears the cutoff), split half onto each one-way leg.
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
        self.prepared = Some((params, prepared));
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
        params: WaveguideParams,
    ) -> f32 {
        let filtered = self.boundary_filters.process(side, input);
        let nonlinear = if side == BoundarySide::Left {
            // Static brassiness character (unchanged from pre-M5).
            let static_drive = math::finite_clamp(params.loop_nonlinearity, 0.0, 1.0, 0.0);
            let saturated = if static_drive > 0.0 {
                soft_saturate(filtered, static_drive)
            } else {
                filtered
            };
            // Energy-dependent finite-amplitude steepening: an amplitude-driven
            // dispersion allpass once per round trip (M5). Unity magnitude keeps
            // the loop stable; the amplitude-varying coefficient steepens the
            // high-pressure fronts into upper harmonics, so a loud bore turns brassy.
            self.apply_steepening(saturated)
        } else {
            filtered
        };
        let reflection = match side {
            BoundarySide::Left => profile.mouth_reflection,
            BoundarySide::Right => profile.end_reflection,
        };

        math::snap_to_zero(nonlinear * reflection)
    }

    /// Energy-driven amplitude-dependent dispersion (finite-amplitude steepening).
    /// The allpass coefficient is the instantaneous wave amplitude (so it varies
    /// within each cycle — the nonlinearity) scaled by the measured-energy term,
    /// so loud crests disperse most and sharpen into upper harmonics, while the
    /// unity magnitude leaves the loop gain (and so the sustain) untouched.
    fn apply_steepening(&mut self, sample: f32) -> f32 {
        let energy = steepening_energy(self.steepening_drive);
        if energy <= f32::EPSILON {
            return sample;
        }
        let amplitude = (sample.abs() * STEEPEN_AMPLITUDE_SENS).tanh();
        let coefficient = math::finite_clamp(STEEPEN_MAX_COEFF * energy * amplitude, 0.0, 1.0, 0.0);
        self.steepening_allpass.set_coefficient(coefficient);
        self.steepening_allpass.process(sample)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TubeBoreProfile {
    mouth_loss: BiquadCoefficients,
    mouth_reflection: f32,
    end_reflection: f32,
    excitation_coupling: f32,
    pressure_mix: f32,
}

impl TubeBoreProfile {
    fn from_params(sample_rate: f32, params: WaveguideParams, loop_gain: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let endpoint_loss = core::endpoint_reflection_gain(loop_gain);
        let end_reflection = bore_end_reflection(params.boundary_reflection) * endpoint_loss;
        let openness = (1.0 - TUBE_BOUNDARY.reflection(params.boundary_reflection)) * 0.5;
        // The bore's wall/radiation losses roll off the high (odd) harmonics each round trip —
        // the difference between a warm, hollow wind tone and a bright buzzy square. The
        // reed-driven bore re-injects a harmonically rich pulse every cycle, so without strong
        // loop damping the high harmonics sit at full level (h3 ≈ h1) and it reads as a raw
        // square synth. This brings the effective mouth cutoff well down (the shipped 8 kHz
        // brightness control lands near ~2 kHz), giving the clarinet-like rolloff; the control
        // still sweeps relative brightness on top.
        let mouth_cutoff = math::finite_clamp(
            params.loop_filter_cutoff * (0.75 + 0.35 * openness),
            160.0,
            sample_rate * 0.45,
            6_000.0,
        );

        Self {
            mouth_loss: BiquadCoefficients::lowpass(sample_rate, mouth_cutoff, DEFAULT_BIQUAD_Q),
            mouth_reflection: MOUTH_REFLECTION * endpoint_loss,
            end_reflection,
            excitation_coupling: TUBE_BOUNDARY.excitation_coupling(params.boundary_reflection),
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

#[cfg(test)]
mod tests;
