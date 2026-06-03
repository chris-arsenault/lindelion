use lindelion_dsp_utils::{delay::FirstOrderAllpass, math};

use crate::{DISPERSION, core, model::String1dParams};

/// Number of cascaded first-order allpass sections in the stiffness dispersion
/// filter. A stiff string disperses high partials forward (sharp), and a single
/// first-order section barely bends the audible partials; cascading several with a
/// shared coefficient builds the smooth, frequency-rising stretch of a real wound
/// string (Bensa/Välimäki/Rauhala waveguide-dispersion approach).
const DISPERSION_STAGES: usize = 8;
/// Allpass coefficient magnitude `|a|` at full stiffness. Larger pulls the
/// group-delay transition down into the played partials, so more of the stretch
/// lands where it is audible; kept below 1 for stability.
const DISPERSION_COEFFICIENT_AT_MAX: f32 = 0.45;
/// One-way delays (samples) bounding the stiffness availability. Below
/// `MIN_LOOP_SAMPLES` the loop is too short to hold the cascade's own delay in
/// tune, so dispersion bypasses entirely; it ramps to full stiffness by
/// `FULL_LOOP_SAMPLES`. Physically, short high strings disperse little.
const MIN_LOOP_SAMPLES: f32 = 48.0;
const FULL_LOOP_SAMPLES: f32 = 170.0;
/// Cap on the cascade's fundamental group delay that the loop length compensates,
/// so the played pitch stays put. Raised from the old 2-section value for the
/// longer cascade; high notes (short loops) clamp here and disperse a little less.
const MAX_DELAY_COMPENSATION_SAMPLES: f32 = 64.0;
/// Empirical scale on the cascade's one-way phase-delay compensation so the
/// fundamental stays in tune. Calibrated against the measured played pitch across
/// the dispersion range (the simple `stages × phase_delay` over-compensates given
/// how `delay_tuning` folds the extra delay across the round trip).
const DELAY_COMPENSATION_FACTOR: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DispersionProfile {
    /// Shared coefficient of every cascade section (negative: low partials are
    /// delayed more, so the loop-length compensation leaves the high partials on a
    /// shorter loop — sharp, i.e. stiff).
    coefficient: f32,
    pub delay_compensation_samples: f32,
}

impl DispersionProfile {
    fn bypass() -> Self {
        Self {
            coefficient: 0.0,
            delay_compensation_samples: 0.0,
        }
    }

    fn is_enabled(self) -> bool {
        self.coefficient != 0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WaveguideDispersion {
    stages: [FirstOrderAllpass; DISPERSION_STAGES],
}

impl WaveguideDispersion {
    pub(super) fn new() -> Self {
        Self {
            stages: [FirstOrderAllpass::default(); DISPERSION_STAGES],
        }
    }

    pub(super) fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }

    pub(super) fn process_sample(&mut self, input: f32, profile: DispersionProfile) -> f32 {
        let input = math::snap_to_zero(input);
        if !profile.is_enabled() {
            return input;
        }

        let mut sample = input;
        for stage in &mut self.stages {
            stage.set_coefficient(profile.coefficient);
            sample = stage.process(sample);
        }
        math::snap_to_zero(sample)
    }
}

pub(super) fn dispersion_profile(sample_rate: f32, params: String1dParams) -> DispersionProfile {
    let amount = DISPERSION.clamp(params.dispersion);
    if amount <= f32::EPSILON {
        return DispersionProfile::bypass();
    }

    let sample_rate = core::sanitize_sample_rate(sample_rate);
    let frequency_hz = core::sanitize_frequency(sample_rate, params.frequency_hz);
    // The cascade's compensation must fit inside the one-way delay line, else the
    // loop can't stay in tune. Short loops (high notes) therefore disperse less —
    // physically a thin, short string. Scale the stiffness so the compensation
    // stays within a fraction of the one-way delay.
    let one_way_delay = sample_rate / (2.0 * frequency_hz);
    let headroom = ((one_way_delay - MIN_LOOP_SAMPLES) / (FULL_LOOP_SAMPLES - MIN_LOOP_SAMPLES))
        .clamp(0.0, 1.0);
    let coefficient = -DISPERSION_COEFFICIENT_AT_MAX * amount * headroom;
    if coefficient.abs() <= f32::EPSILON {
        return DispersionProfile::bypass();
    }
    let omega = std::f32::consts::TAU * frequency_hz / sample_rate;
    // The cascade's phase delay at the fundamental is what the loop length must
    // subtract to keep the played pitch put; `delay_tuning` folds this one-way value
    // across the round trip. Phase (not group) delay sets the resonance condition.
    let delay_compensation_samples = (DELAY_COMPENSATION_FACTOR
        * DISPERSION_STAGES as f32
        * first_order_phase_delay_samples(coefficient, omega))
    .clamp(0.0, MAX_DELAY_COMPENSATION_SAMPLES);

    DispersionProfile {
        coefficient,
        delay_compensation_samples,
    }
}

/// Phase delay (samples) of one first-order allpass `(a + z⁻¹)/(1 + a z⁻¹)` at
/// `omega`. This — not the group delay — sets where the loop resonates, so it is
/// what the loop length compensates to keep the fundamental in tune.
fn first_order_phase_delay_samples(coefficient: f32, omega: f32) -> f32 {
    let a = math::finite_clamp(coefficient, -0.95, 0.95, 0.0);
    let omega = math::finite_clamp(omega, 0.0, std::f32::consts::PI, 0.0);
    if omega < 1.0e-5 {
        // Limit at DC: (1 − a)/(1 + a).
        return math::finite_clamp(
            (1.0 - a) / (1.0 + a),
            0.0,
            MAX_DELAY_COMPENSATION_SAMPLES,
            0.0,
        );
    }
    let sin = omega.sin();
    let cos = omega.cos();
    let phase = (-sin).atan2(a + cos) - (-a * sin).atan2(1.0 + a * cos);
    math::finite_clamp(-phase / omega, 0.0, MAX_DELAY_COMPENSATION_SAMPLES, 0.0)
}
