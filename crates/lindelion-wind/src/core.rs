#[cfg(test)]
use lindelion_dsp_utils::phase::principal_angle_f32;
use lindelion_dsp_utils::{filters::BiquadCoefficients, math};

use super::{
    DSP_FALLBACK_SAMPLE_RATE, LOOP_FILTER_CUTOFF_DEFAULT_HZ, LOOP_FILTER_Q, LOOP_GAIN,
    PICKUP_POSITION, ResonanceQ,
};

const DECAY_MIN_SECONDS: f32 = 0.02;
const DECAY_MAX_SECONDS: f32 = 10.0;
const INPUT_SMOOTHING_SECONDS: f32 = 0.008;
const INPUT_SMOOTHING_SNAP_TOLERANCE: f32 = 1.0e-4;
const FILTER_PEAK_SCAN_POINTS: usize = 96;
#[cfg(test)]
const GROUP_DELAY_PROBE_RADIANS: f32 = 0.001;
const MAX_FILTER_DELAY_COMPENSATION_SAMPLES: f32 = 8.0;
const BORE_HF_LOSS_CONTROL_SCALE: f32 = 0.30;
const BORE_HF_LOSS_BRIGHT_SCALE: f32 = 0.60;
const BORE_HF_LOSS_REGISTER_FLOOR_MULTIPLE: f32 = 2.75;
const BORE_HF_LOSS_MIN_CUTOFF_HZ: f32 = 650.0;
const BORE_HF_LOSS_MAX_CUTOFF_HZ: f32 = 8_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayTuning {
    pub integer_delay: f32,
    pub fractional_delay: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopDamping {
    pub coefficients: BiquadCoefficients,
    pub loop_gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveguideGeometry {
    pub pickup_position: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalarSmoother {
    state: f32,
    coefficient: f32,
    initialized: bool,
}

impl ScalarSmoother {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let coefficient =
            math::finite_clamp(1.0 / (INPUT_SMOOTHING_SECONDS * sample_rate), 0.0, 1.0, 1.0);
        Self {
            state: 0.0,
            coefficient,
            initialized: false,
        }
    }

    pub fn reset(&mut self) {
        self.initialized = false;
        self.state = 0.0;
    }

    pub fn next(&mut self, target: f32) -> f32 {
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
}

pub fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DSP_FALLBACK_SAMPLE_RATE
    }
}

pub fn loop_damping(
    sample_rate: f32,
    frequency_hz: f32,
    loop_filter_cutoff: f32,
    loop_filter_resonance: f32,
    loop_gain: f32,
) -> LoopDamping {
    let sample_rate = sanitize_sample_rate(sample_rate);
    let coefficients =
        loop_filter_coefficients(sample_rate, loop_filter_cutoff, loop_filter_resonance);
    let frequency_hz = sanitize_frequency(sample_rate, frequency_hz);
    let period_samples = sample_rate / frequency_hz;
    let decay_seconds = decay_seconds_from_loop_gain(loop_gain);
    let fundamental_gain = gain_for_t60(period_samples, sample_rate, decay_seconds);
    let fundamental_omega = std::f32::consts::TAU * frequency_hz / sample_rate;
    let fundamental_magnitude = biquad_magnitude_at(coefficients, fundamental_omega).max(1.0e-4);
    let filter_peak = measured_filter_peak(coefficients);
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
    }
}

pub fn bore_hf_loss_cutoff_hz(
    sample_rate: f32,
    frequency_hz: f32,
    requested_cutoff_hz: f32,
) -> f32 {
    let sample_rate = sanitize_sample_rate(sample_rate);
    let frequency_hz = sanitize_frequency(sample_rate, frequency_hz);
    let requested_cutoff_hz = math::finite_clamp(
        requested_cutoff_hz,
        20.0,
        sample_rate * 0.45,
        LOOP_FILTER_CUTOFF_DEFAULT_HZ,
    );
    let bright_lift = ((requested_cutoff_hz / LOOP_FILTER_CUTOFF_DEFAULT_HZ) - 1.0).max(0.0) / 2.0;
    let control_scale = BORE_HF_LOSS_CONTROL_SCALE
        + (BORE_HF_LOSS_BRIGHT_SCALE - BORE_HF_LOSS_CONTROL_SCALE) * bright_lift.clamp(0.0, 1.0);
    let control_cutoff = requested_cutoff_hz * control_scale;
    let register_floor = frequency_hz * BORE_HF_LOSS_REGISTER_FLOOR_MULTIPLE;
    let max_cutoff = BORE_HF_LOSS_MAX_CUTOFF_HZ.min(sample_rate * 0.45);

    math::finite_clamp(
        control_cutoff.max(register_floor),
        BORE_HF_LOSS_MIN_CUTOFF_HZ,
        max_cutoff,
        LOOP_FILTER_CUTOFF_DEFAULT_HZ * BORE_HF_LOSS_CONTROL_SCALE,
    )
}

pub fn endpoint_reflection_gain(loop_gain: f32) -> f32 {
    math::finite_clamp(loop_gain, 0.0, 0.999, 0.0).sqrt()
}

pub fn max_delay_samples(sample_rate: f32, lowest_frequency_hz: f32, cycle_divisor: f32) -> usize {
    let cycle_divisor = cycle_divisor.max(1.0);
    (sample_rate / (lowest_frequency_hz.max(1.0) * cycle_divisor)).ceil() as usize + 8
}

pub fn delay_tuning(
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

pub fn waveguide_geometry(pickup_position: f32) -> WaveguideGeometry {
    WaveguideGeometry {
        pickup_position: PICKUP_POSITION.clamp(pickup_position),
    }
}

pub fn position_delay_samples(loop_delay_samples: f32, position: f32) -> f32 {
    let loop_delay_samples = math::finite_or(loop_delay_samples, 0.0).max(0.0);
    let position = math::finite_clamp(position, 0.0, 1.0, 0.5);
    (loop_delay_samples * position).clamp(0.0, loop_delay_samples)
}

pub fn filter_phase_delay_samples(
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

pub fn sanitize_frequency(sample_rate: f32, frequency_hz: f32) -> f32 {
    math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.45, 220.0)
}

fn loop_filter_coefficients(
    sample_rate: f32,
    loop_filter_cutoff: f32,
    loop_filter_resonance: f32,
) -> BiquadCoefficients {
    let q = q_for_resonance(LOOP_FILTER_Q, loop_filter_resonance);
    BiquadCoefficients::lowpass(sample_rate, loop_filter_cutoff, q)
}

fn q_for_resonance(model: ResonanceQ, resonance: f32) -> f32 {
    model.q_for_resonance(resonance)
}

fn decay_seconds_from_loop_gain(loop_gain: f32) -> f32 {
    let normalized = LOOP_GAIN.clamp(loop_gain) / LOOP_GAIN.max;
    if normalized <= 0.0 {
        return 0.0;
    }
    let range = DECAY_MAX_SECONDS / DECAY_MIN_SECONDS;
    DECAY_MIN_SECONDS * range.powf(normalized * normalized)
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

#[cfg(test)]
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
    fn loop_damping_maps_higher_loop_gain_to_longer_decay() {
        let quiet = loop_damping(48_000.0, 220.0, 18_000.0, 0.0, 0.2);
        let sustained = loop_damping(48_000.0, 220.0, 18_000.0, 0.0, 0.98);

        assert!(quiet.loop_gain > 0.0);
        assert!(sustained.loop_gain > quiet.loop_gain);
        assert!(sustained.loop_gain < 1.0);
    }

    #[test]
    fn bore_hf_loss_cutoff_keeps_default_in_clarinet_band() {
        let dark = bore_hf_loss_cutoff_hz(48_000.0, 392.0, 1_200.0);
        let default = bore_hf_loss_cutoff_hz(48_000.0, 392.0, LOOP_FILTER_CUTOFF_DEFAULT_HZ);
        let bright = bore_hf_loss_cutoff_hz(48_000.0, 392.0, 14_000.0);

        assert!(dark < default, "dark={dark} default={default}");
        assert!(default > 1_150.0 && default < 1_400.0, "default={default}");
        assert!(bright > default * 4.0, "default={default} bright={bright}");
        assert!(bright <= BORE_HF_LOSS_MAX_CUTOFF_HZ);
    }

    #[test]
    fn loop_filter_delay_helpers_stay_finite() {
        let damping = loop_damping(48_000.0, 220.0, 900.0, 0.2, 0.97);
        let group = filter_group_delay_samples(damping.coefficients, 48_000.0, 220.0);
        let phase = filter_phase_delay_samples(damping.coefficients, 48_000.0, 220.0);

        assert!(group.is_finite());
        assert!(phase.is_finite());
    }
}
