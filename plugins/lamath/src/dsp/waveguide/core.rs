use lindelion_dsp_utils::{filters::BiquadCoefficients, math, phase::principal_angle_f32};

use super::WaveguideParams;
use crate::dsp::constants::{
    DSP_FALLBACK_SAMPLE_RATE, STRIKE_POSITION, WAVEGUIDE_LOOP_FILTER_Q, WAVEGUIDE_LOOP_GAIN,
    WAVEGUIDE_PICKUP_POSITION,
};

const WAVEGUIDE_DECAY_MIN_SECONDS: f32 = 0.02;
/// Upper bound of the `loop_gain → T60` map (M11 P2). Raised from 2.5 s so a
/// held high-loop-gain string/bore can ring with a real, long tail; the loop
/// stays bounded by the `stability_limit` clamp in `loop_damping` regardless of
/// this cap. Indefinite-while-held sustain comes from the P3 bow/reed driver,
/// not from this ceiling.
const WAVEGUIDE_DECAY_MAX_SECONDS: f32 = 10.0;
/// Smoothing time for the continuous physical inputs (loop gain/cutoff/etc.), so
/// a control-rate parameter jump becomes a short per-sample ramp rather than a
/// zipper step. Kept short enough to feel immediate while still gliding.
const INPUT_SMOOTHING_SECONDS: f32 = 0.008;
/// Relative tolerance at which a smoother snaps exactly onto its target, so a
/// settled input stops moving and the prepared-model cache can stay warm. Set
/// above the f32 one-pole stall floor (per-sample increments underflow once the
/// remaining delta is a few ulp of the state) so the ramp always reaches target;
/// the residual it absorbs is inaudible (≈2e-4 on gain, ≈0.9 Hz on a 9 kHz cutoff).
const INPUT_SMOOTHING_SNAP_TOLERANCE: f32 = 1.0e-4;
const FILTER_PEAK_SCAN_POINTS: usize = 96;
const GROUP_DELAY_PROBE_RADIANS: f32 = 0.001;
const MAX_FILTER_DELAY_COMPENSATION_SAMPLES: f32 = 8.0;
const EXCITATION_WIDTH_FRACTION: f32 = 0.035;
/// Excitation-window width fraction at full strike-position spread (M9 strum). A
/// quarter of the string spreads the contact widely, combing out the high partials
/// so a strum reads darker than a tight pick at the same level.
const STRUM_WIDTH_FRACTION: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DelayTuning {
    pub integer_delay: f32,
    pub fractional_delay: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LoopDamping {
    pub coefficients: BiquadCoefficients,
    pub loop_gain: f32,
    pub filter_delay_samples: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PositionTap {
    pub position: f32,
    pub gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct WaveguideGeometry {
    pub pickup_position: f32,
    pub excitation_taps: [PositionTap; 3],
}

/// One-pole smoother for a single continuous physical input. Initialized
/// converged — the first `next` after construction or `reset` snaps to its
/// target, so a steady input is inert (preserving exact equivalence) and only a
/// genuine change ramps. Snaps onto the target within a relative tolerance so a
/// settled value stops moving and the prepared-model cache stays warm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ScalarSmoother {
    state: f32,
    coefficient: f32,
    initialized: bool,
}

impl ScalarSmoother {
    pub(super) fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let coefficient =
            math::finite_clamp(1.0 / (INPUT_SMOOTHING_SECONDS * sample_rate), 0.0, 1.0, 1.0);
        Self {
            state: 0.0,
            coefficient,
            initialized: false,
        }
    }

    pub(super) fn reset(&mut self) {
        self.initialized = false;
        self.state = 0.0;
    }

    pub(super) fn next(&mut self, target: f32) -> f32 {
        let target = math::finite_or(target, self.state);
        if !self.initialized {
            self.state = target;
            self.initialized = true;
            return self.state;
        }

        self.state += self.coefficient * (target - self.state);
        if (self.state - target).abs() <= INPUT_SMOOTHING_SNAP_TOLERANCE * (1.0 + target.abs()) {
            self.state = target;
        }
        self.state
    }

    #[cfg(test)]
    pub(super) fn current(&self) -> f32 {
        self.state
    }
}

pub(super) fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DSP_FALLBACK_SAMPLE_RATE
    }
}

pub(super) fn loop_damping(sample_rate: f32, params: WaveguideParams) -> LoopDamping {
    let sample_rate = sanitize_sample_rate(sample_rate);
    let coefficients = loop_filter_coefficients(
        sample_rate,
        params.loop_filter_cutoff,
        params.loop_filter_resonance,
    );
    let filter_delay_samples =
        filter_group_delay_samples(coefficients, sample_rate, params.frequency_hz);
    let frequency_hz = sanitize_frequency(sample_rate, params.frequency_hz);
    let period_samples = sample_rate / frequency_hz;
    let decay_seconds = decay_seconds_from_loop_gain(params.loop_gain);
    let fundamental_gain = gain_for_t60(period_samples, sample_rate, decay_seconds);
    // Calibrate the loop gain so the played pitch decays in the requested T60,
    // dividing out the loop filter's own attenuation at the fundamental. The
    // filter's roll-off then gives higher partials an explicit, calibrated
    // frequency-dependent T60(f) — they decay measurably faster than the
    // fundamental — instead of the fundamental and partials sharing one
    // untargeted decay time that the loop lowpass only approximates.
    let fundamental_omega = std::f32::consts::TAU * frequency_hz / sample_rate;
    let fundamental_magnitude = biquad_magnitude_at(coefficients, fundamental_omega).max(1.0e-4);
    let filter_peak = measured_filter_peak(coefficients);
    // Keep the worst-case round-trip magnitude below unity across every partial.
    let stability_limit = 0.999 / filter_peak.max(1.0);
    let loop_gain = math::finite_clamp(
        fundamental_gain / fundamental_magnitude,
        0.0,
        stability_limit,
        0.0,
    );

    LoopDamping {
        coefficients,
        loop_gain,
        filter_delay_samples,
    }
}

pub(super) fn endpoint_reflection_gain(loop_gain: f32) -> f32 {
    math::finite_clamp(loop_gain, 0.0, 0.999, 0.0).sqrt()
}

pub(super) fn max_delay_samples(
    sample_rate: f32,
    lowest_frequency_hz: f32,
    cycle_divisor: f32,
) -> usize {
    let cycle_divisor = cycle_divisor.max(1.0);
    (sample_rate / (lowest_frequency_hz.max(1.0) * cycle_divisor)).ceil() as usize + 8
}

pub(super) fn delay_tuning(
    sample_rate: f32,
    delay_capacity: usize,
    frequency_hz: f32,
    cycle_divisor: f32,
    delay_offset_samples: f32,
) -> DelayTuning {
    let cycle_divisor = cycle_divisor.max(1.0);
    let frequency_hz = sanitize_frequency(sample_rate, frequency_hz);
    let delay_samples = (sample_rate / (frequency_hz * cycle_divisor) - delay_offset_samples)
        .clamp(1.0, delay_capacity as f32 - 3.0);
    let integer_delay = delay_samples.floor();

    DelayTuning {
        integer_delay,
        fractional_delay: delay_samples - integer_delay,
    }
}

pub(super) fn waveguide_geometry(strike_position: f32, pickup_position: f32) -> WaveguideGeometry {
    let pickup_position = WAVEGUIDE_PICKUP_POSITION.clamp(pickup_position);
    WaveguideGeometry {
        pickup_position,
        // The cached taps are the pre-M9 narrow contact (spread 0); the contact
        // stage widens them at injection (M9) via `excitation_taps`.
        excitation_taps: excitation_taps(strike_position, EXCITATION_WIDTH_FRACTION * 0.5),
    }
}

/// The three-tap excitation window centred on the strike position with a
/// triangular `0.25 / 0.5 / 0.25` gain profile, spanning `±half_width` (clamped
/// in-bounds). Pulled out of `waveguide_geometry` so the M9 contact stage can
/// rebuild it at a spread-driven width at injection without re-running the cached
/// prepared model.
pub(super) fn excitation_taps(strike_position: f32, half_width: f32) -> [PositionTap; 3] {
    let strike_position = STRIKE_POSITION.clamp(strike_position);
    let half_width = math::finite_or(half_width, 0.0).max(0.0);
    [
        PositionTap {
            position: STRIKE_POSITION.clamp(strike_position - half_width),
            gain: 0.25,
        },
        PositionTap {
            position: strike_position,
            gain: 0.5,
        },
        PositionTap {
            position: STRIKE_POSITION.clamp(strike_position + half_width),
            gain: 0.25,
        },
    ]
}

/// Half-width of the excitation window for a normalised strike-position spread
/// `0..1` (M9). Spread `0` returns the pre-M9 narrow pick half-width; spread `1`
/// returns the wide-strum half-width, combing out the high partials.
pub(super) fn excitation_half_width(spread: f32) -> f32 {
    let spread = math::finite_clamp(spread, 0.0, 1.0, 0.0);
    0.5 * (EXCITATION_WIDTH_FRACTION + spread * (STRUM_WIDTH_FRACTION - EXCITATION_WIDTH_FRACTION))
}

pub(super) fn position_delay_samples(loop_delay_samples: f32, position: f32) -> f32 {
    let loop_delay_samples = math::finite_or(loop_delay_samples, 0.0).max(0.0);
    let position = math::finite_clamp(position, 0.0, 1.0, 0.5);
    (loop_delay_samples * position).clamp(0.0, loop_delay_samples)
}

#[cfg(test)]
pub(super) fn complementary_position_delay_samples(loop_delay_samples: f32, position: f32) -> f32 {
    position_delay_samples(loop_delay_samples, 1.0 - position)
}

fn loop_filter_coefficients(
    sample_rate: f32,
    loop_filter_cutoff: f32,
    loop_filter_resonance: f32,
) -> BiquadCoefficients {
    let q = WAVEGUIDE_LOOP_FILTER_Q.q_for_resonance(loop_filter_resonance);
    BiquadCoefficients::lowpass(sample_rate, loop_filter_cutoff, q)
}

/// Shared frequency sanitizer used for both the delay length and the filter-delay
/// compensation, so they always agree on the frequency the string is tuned to.
pub(super) fn sanitize_frequency(sample_rate: f32, frequency_hz: f32) -> f32 {
    math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.45, 220.0)
}

pub(super) fn decay_seconds_from_loop_gain(loop_gain: f32) -> f32 {
    let normalized = WAVEGUIDE_LOOP_GAIN.clamp(loop_gain) / WAVEGUIDE_LOOP_GAIN.max;
    if normalized <= 0.0 {
        return 0.0;
    }

    let range = WAVEGUIDE_DECAY_MAX_SECONDS / WAVEGUIDE_DECAY_MIN_SECONDS;
    WAVEGUIDE_DECAY_MIN_SECONDS * range.powf(normalized * normalized)
}

fn gain_for_t60(period_samples: f32, sample_rate: f32, decay_seconds: f32) -> f32 {
    if decay_seconds <= 0.0 || !period_samples.is_finite() {
        return 0.0;
    }

    0.001_f32.powf(period_samples / (decay_seconds * sample_rate))
}

fn measured_filter_peak(coefficients: BiquadCoefficients) -> f32 {
    let mut peak = 0.0_f32;
    for index in 0..=FILTER_PEAK_SCAN_POINTS {
        let omega = std::f32::consts::PI * index as f32 / FILTER_PEAK_SCAN_POINTS as f32;
        peak = peak.max(biquad_magnitude_at(coefficients, omega));
    }
    math::finite_clamp(peak, 0.0, 32.0, 1.0)
}

/// Phase delay (−phase / ω) of a biquad at `frequency_hz`, in samples.
///
/// Loop resonance is set by the *phase* the wave accumulates per round trip, so
/// a resonator tuned to a target pitch must compensate each loop filter's phase
/// delay — not its group delay, which only matches near DC and drifts as the
/// played pitch approaches the filter cutoff.
pub(super) fn filter_phase_delay_samples(
    coefficients: BiquadCoefficients,
    sample_rate: f32,
    frequency_hz: f32,
) -> f32 {
    let omega = std::f32::consts::TAU * sanitize_frequency(sample_rate, frequency_hz) / sample_rate;
    if omega <= f32::EPSILON {
        return 0.0;
    }
    let phase_delay = -biquad_phase_at(coefficients, omega) / omega;
    math::finite_clamp(phase_delay, 0.0, MAX_FILTER_DELAY_COMPENSATION_SAMPLES, 0.0)
}

fn filter_group_delay_samples(
    coefficients: BiquadCoefficients,
    sample_rate: f32,
    frequency_hz: f32,
) -> f32 {
    let omega = std::f32::consts::TAU * sanitize_frequency(sample_rate, frequency_hz) / sample_rate;
    let delta = GROUP_DELAY_PROBE_RADIANS;
    let low = (omega - delta).clamp(delta, std::f32::consts::PI - delta);
    let high = (omega + delta).clamp(delta, std::f32::consts::PI - delta);
    if high <= low {
        return 0.0;
    }

    let phase_low = biquad_phase_at(coefficients, low);
    let phase_high = biquad_phase_at(coefficients, high);
    let phase_delta = principal_angle_f32(phase_high - phase_low);
    let group_delay = -phase_delta / (high - low);
    math::finite_clamp(group_delay, 0.0, MAX_FILTER_DELAY_COMPENSATION_SAMPLES, 0.0)
}

fn biquad_magnitude_at(coefficients: BiquadCoefficients, omega: f32) -> f32 {
    let (sin1, cos1) = omega.sin_cos();
    let (sin2, cos2) = (2.0 * omega).sin_cos();
    let numerator_real = coefficients.b0 + coefficients.b1 * cos1 + coefficients.b2 * cos2;
    let numerator_imag = -(coefficients.b1 * sin1 + coefficients.b2 * sin2);
    let denominator_real = 1.0 + coefficients.a1 * cos1 + coefficients.a2 * cos2;
    let denominator_imag = -(coefficients.a1 * sin1 + coefficients.a2 * sin2);
    let numerator = numerator_real.mul_add(numerator_real, numerator_imag * numerator_imag);
    let denominator =
        denominator_real.mul_add(denominator_real, denominator_imag * denominator_imag);

    if denominator <= f32::EPSILON {
        return 32.0;
    }

    (numerator / denominator).sqrt()
}

fn biquad_phase_at(coefficients: BiquadCoefficients, omega: f32) -> f32 {
    let (sin1, cos1) = omega.sin_cos();
    let (sin2, cos2) = (2.0 * omega).sin_cos();
    let numerator_real = coefficients.b0 + coefficients.b1 * cos1 + coefficients.b2 * cos2;
    let numerator_imag = -(coefficients.b1 * sin1 + coefficients.b2 * sin2);
    let denominator_real = 1.0 + coefficients.a1 * cos1 + coefficients.a2 * cos2;
    let denominator_imag = -(coefficients.a1 * sin1 + coefficients.a2 * sin2);

    numerator_imag.atan2(numerator_real) - denominator_imag.atan2(denominator_real)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_tuning_splits_integer_and_fractional_delay() {
        let tuning = delay_tuning(48_000.0, 256, 277.18, 1.0, 1.0);

        assert_eq!(tuning.integer_delay, 172.0);
        assert!((tuning.fractional_delay - 0.173).abs() < 0.01);
    }

    #[test]
    fn half_cycle_tuning_uses_one_way_string_delay() {
        let tuning = delay_tuning(48_000.0, 128, 240.0, 2.0, 0.0);

        assert_eq!(tuning.integer_delay, 100.0);
        assert!(tuning.fractional_delay.abs() < 0.000_1);
    }

    #[test]
    fn invalid_sample_rate_uses_dsp_fallback() {
        assert_eq!(sanitize_sample_rate(f32::NAN), DSP_FALLBACK_SAMPLE_RATE);
    }

    #[test]
    fn waveguide_geometry_clamps_positions_and_normalizes_taps() {
        let geometry = waveguide_geometry(f32::NAN, 2.0);
        let tap_gain_sum = geometry
            .excitation_taps
            .iter()
            .map(|tap| tap.gain)
            .sum::<f32>();

        assert_eq!(geometry.pickup_position, WAVEGUIDE_PICKUP_POSITION.max);
        assert_eq!(
            geometry.excitation_taps[1].position,
            STRIKE_POSITION.default
        );
        assert!((tap_gain_sum - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn excitation_taps_span_finite_width_around_strike() {
        let geometry = waveguide_geometry(0.4, 0.8);

        assert!(geometry.excitation_taps[0].position < 0.4);
        assert_eq!(geometry.excitation_taps[1].position, 0.4);
        assert!(geometry.excitation_taps[2].position > 0.4);
    }

    #[test]
    fn position_delay_helpers_map_normalized_locations() {
        assert_eq!(position_delay_samples(100.0, 0.25), 25.0);
        assert_eq!(complementary_position_delay_samples(100.0, 0.25), 75.0);
        assert_eq!(position_delay_samples(f32::NAN, 0.25), 0.0);
    }

    #[test]
    fn raised_decay_cap_extends_max_t60_past_old_limit() {
        // M11 P2 step 1: the loop_gain → T60 map now tops out well past the old
        // 2.5 s ceiling, so a near-unity loop can ring for many seconds. The
        // maximum loop gain maps exactly to the cap.
        let max_t60 = decay_seconds_from_loop_gain(WAVEGUIDE_LOOP_GAIN.max);
        assert!(
            max_t60 > 2.5,
            "raised cap should extend max T60 past the old limit: {max_t60}"
        );
        assert!(
            (max_t60 - WAVEGUIDE_DECAY_MAX_SECONDS).abs() < 1.0e-3,
            "max loop gain should map to the cap: {max_t60}"
        );
    }

    #[test]
    fn loop_damping_maps_higher_loop_gain_to_longer_decay() {
        let quiet = loop_damping(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.2,
                ..WaveguideParams::default()
            },
        );
        let sustained = loop_damping(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.98,
                ..WaveguideParams::default()
            },
        );

        assert!(quiet.loop_gain > 0.0);
        assert!(sustained.loop_gain > quiet.loop_gain);
        assert!(sustained.loop_gain < 1.0);
    }

    #[test]
    fn loop_damping_compensates_resonant_filter_peak() {
        let flat = loop_damping(
            48_000.0,
            WaveguideParams {
                loop_filter_cutoff: 2_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.98,
                ..WaveguideParams::default()
            },
        );
        let resonant = loop_damping(
            48_000.0,
            WaveguideParams {
                loop_filter_cutoff: 2_000.0,
                loop_filter_resonance: 0.98,
                loop_gain: 0.98,
                ..WaveguideParams::default()
            },
        );

        assert!(resonant.loop_gain < flat.loop_gain);
        assert!(measured_filter_peak(resonant.coefficients) * resonant.loop_gain < 1.0);
    }

    #[test]
    fn loop_damping_reports_filter_delay_for_tuning_compensation() {
        let dark = loop_damping(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 900.0,
                ..WaveguideParams::default()
            },
        );
        let bright = loop_damping(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 18_000.0,
                ..WaveguideParams::default()
            },
        );

        assert!(dark.filter_delay_samples > 0.0);
        assert!(bright.filter_delay_samples < dark.filter_delay_samples);
    }

    #[test]
    fn zero_loop_gain_produces_zero_feedback() {
        let damping = loop_damping(
            48_000.0,
            WaveguideParams {
                loop_gain: 0.0,
                ..WaveguideParams::default()
            },
        );

        assert_eq!(damping.loop_gain, 0.0);
        assert_eq!(endpoint_reflection_gain(damping.loop_gain), 0.0);
    }
}
