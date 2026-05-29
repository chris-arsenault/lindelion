//! Additive sinusoidal / spectral-peak resynthesis (McAulay–Quatieri style): each
//! cached spectral peak is reconstructed as an oscillator whose phase is anchored
//! to its measured phase intercept at an absolute sample position, summed across
//! peaks. This is **not** a phase vocoder — there is no STFT/ISTFT or inter-frame
//! principal-argument phase accumulation here. The genuine phase vocoder (with
//! instantaneous-frequency phase advance and Laroche–Dolson peak locking) lives in
//! the `resample_pro_*` modules.

use crate::{
    PitchShiftFrameAnalysis, PitchShiftSourceCache,
    synthesis::PitchShiftRenderConfig,
    synthesis_support::{frame_pair_at_position, spectral_envelope_formant_gain},
};

const HARMONIC_SCAFFOLD_FLOOR_RATIO: f64 = 0.03;
const HARMONIC_SCAFFOLD_PROXIMITY_RATIO: f64 = 0.2;

pub fn spectral_peak_model_sample(
    cache: &PitchShiftSourceCache,
    frame_position: f64,
    phase_position: f64,
    config: PitchShiftRenderConfig,
) -> Option<f32> {
    let (left, right, mix) = frame_pair_at_position(&cache.frames, frame_position as usize);
    let left_sample = spectral_peak_frame_sample(cache, left, phase_position, config);
    if std::ptr::eq(left, right) {
        return left_sample;
    }
    let right_sample = spectral_peak_frame_sample(cache, right, phase_position, config);
    match (left_sample, right_sample) {
        (Some(left), Some(right)) => Some(left + (right - left) * mix),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn spectral_peak_frame_sample(
    cache: &PitchShiftSourceCache,
    frame: &PitchShiftFrameAnalysis,
    absolute_position: f64,
    config: PitchShiftRenderConfig,
) -> Option<f32> {
    if frame.spectral_peaks.is_empty() {
        return None;
    }
    let ratios = config.ratios.sanitized();
    let sample_rate = cache.sample_rate as f64;
    let nyquist = sample_rate * 0.49;
    let pitch_ratio = ratios.pitch_ratio as f64;
    let mut sample = 0.0_f64;
    let mut magnitude_energy_sum = 0.0_f64;
    for peak in &frame.spectral_peaks {
        let source_frequency = peak.frequency_hz as f64;
        let target_frequency = source_frequency * pitch_ratio;
        if target_frequency <= 0.0 || target_frequency >= nyquist {
            continue;
        }
        let magnitude = peak.magnitude as f64
            * spectral_envelope_formant_gain(&frame.spectral_envelope, source_frequency, ratios);
        let source_phase_intercept = peak.phase_radians as f64
            - std::f64::consts::TAU * source_frequency * frame.start_sample as f64 / sample_rate;
        let phase = source_phase_intercept
            + std::f64::consts::TAU * target_frequency * absolute_position / sample_rate;
        sample += phase.cos() * magnitude;
        magnitude_energy_sum += magnitude * magnitude;
    }
    add_harmonic_scaffold(
        frame,
        absolute_position,
        sample_rate,
        config,
        &mut sample,
        &mut magnitude_energy_sum,
    );
    (magnitude_energy_sum > f64::EPSILON).then_some(
        (sample / magnitude_energy_sum.sqrt()
            * frame.rms as f64
            * std::f64::consts::SQRT_2
            * config.harmonic_level as f64) as f32,
    )
}

fn add_harmonic_scaffold(
    frame: &PitchShiftFrameAnalysis,
    absolute_position: f64,
    sample_rate: f64,
    config: PitchShiftRenderConfig,
    sample: &mut f64,
    magnitude_energy_sum: &mut f64,
) {
    if frame.spectral_peaks.len() < 4 {
        return;
    }
    let ratios = config.ratios.sanitized();
    let Some(source_f0) = frame.f0_hz.filter(|f0| frame.voiced && *f0 > 0.0) else {
        return;
    };
    let target_f0 = source_f0 as f64 * ratios.pitch_ratio as f64;
    let nyquist = sample_rate * 0.49;
    if target_f0 <= 0.0 || target_f0 >= nyquist {
        return;
    }
    let harmonic_count = ((nyquist / target_f0) as usize).min(config.max_harmonics);
    let peak_magnitude = frame.spectral_envelope.peak_magnitude() as f64;
    let magnitude_floor = peak_magnitude * HARMONIC_SCAFFOLD_FLOOR_RATIO;
    for harmonic in 1..=harmonic_count {
        let frequency = target_f0 * harmonic as f64;
        if has_nearby_shifted_peak(frame, frequency, ratios.pitch_ratio as f64, target_f0) {
            continue;
        }
        let magnitude = frame
            .spectral_envelope
            .magnitude_at((frequency / ratios.effective_formant_ratio() as f64) as f32)
            .max(magnitude_floor as f32) as f64;
        let phase = std::f64::consts::TAU * frequency * absolute_position / sample_rate;
        *sample += phase.sin() * magnitude;
        *magnitude_energy_sum += magnitude * magnitude;
    }
}

fn has_nearby_shifted_peak(
    frame: &PitchShiftFrameAnalysis,
    frequency_hz: f64,
    pitch_ratio: f64,
    target_f0: f64,
) -> bool {
    let radius = (target_f0 * HARMONIC_SCAFFOLD_PROXIMITY_RATIO).max(8.0);
    frame
        .spectral_peaks
        .iter()
        .any(|peak| (peak.frequency_hz as f64 * pitch_ratio - frequency_hz).abs() <= radius)
}
