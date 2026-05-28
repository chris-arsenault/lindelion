use lindelion_dsp_utils::{filters::BiquadCoefficients, math, phase::principal_angle_f32};

use super::WaveguideParams;
use crate::dsp::constants::{
    DSP_FALLBACK_SAMPLE_RATE, STRIKE_POSITION, WAVEGUIDE_LOOP_FILTER_Q, WAVEGUIDE_LOOP_GAIN,
    WAVEGUIDE_PICKUP_POSITION,
};

const WAVEGUIDE_DECAY_MIN_SECONDS: f32 = 0.02;
const WAVEGUIDE_DECAY_MAX_SECONDS: f32 = 2.5;
const FILTER_PEAK_SCAN_POINTS: usize = 96;
const GROUP_DELAY_PROBE_RADIANS: f32 = 0.001;
const MAX_FILTER_DELAY_COMPENSATION_SAMPLES: f32 = 8.0;
const EXCITATION_WIDTH_FRACTION: f32 = 0.035;

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
    let frequency_hz = tuned_frequency(sample_rate, params.frequency_hz);
    let period_samples = sample_rate / frequency_hz;
    let decay_seconds = decay_seconds_from_loop_gain(params.loop_gain);
    let decay_gain = gain_for_t60(period_samples, sample_rate, decay_seconds);
    let filter_peak = measured_filter_peak(coefficients);
    let passive_filter_gain = 1.0 / filter_peak.max(1.0);
    let loop_gain = math::finite_clamp(decay_gain * passive_filter_gain, 0.0, 0.999, 0.0);

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
    let min_frequency = sample_rate / (delay_capacity.max(1) as f32 * cycle_divisor);
    let frequency_hz = math::finite_clamp(
        frequency_hz,
        min_frequency,
        sample_rate * 0.45,
        min_frequency,
    );
    let delay_samples = (sample_rate / (frequency_hz * cycle_divisor) - delay_offset_samples)
        .clamp(1.0, delay_capacity as f32 - 3.0);
    let integer_delay = delay_samples.floor();

    DelayTuning {
        integer_delay,
        fractional_delay: delay_samples - integer_delay,
    }
}

pub(super) fn waveguide_geometry(strike_position: f32, pickup_position: f32) -> WaveguideGeometry {
    let strike_position = STRIKE_POSITION.clamp(strike_position);
    let pickup_position = WAVEGUIDE_PICKUP_POSITION.clamp(pickup_position);
    let half_width = EXCITATION_WIDTH_FRACTION * 0.5;

    WaveguideGeometry {
        pickup_position,
        excitation_taps: [
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
        ],
    }
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

fn tuned_frequency(sample_rate: f32, frequency_hz: f32) -> f32 {
    math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.45, 220.0)
}

fn decay_seconds_from_loop_gain(loop_gain: f32) -> f32 {
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

fn filter_group_delay_samples(
    coefficients: BiquadCoefficients,
    sample_rate: f32,
    frequency_hz: f32,
) -> f32 {
    let omega = std::f32::consts::TAU * tuned_frequency(sample_rate, frequency_hz) / sample_rate;
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
