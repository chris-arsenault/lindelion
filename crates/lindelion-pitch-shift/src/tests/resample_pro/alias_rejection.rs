use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, estimate_f0_autocorrelation, fitted_sine_rms_error, folded_frequency_hz,
        high_frequency_artifact_ratio, rms, windowed_dft_magnitude_at, zero_crossing_period_jitter,
    },
    db_to_gain,
};

use super::{render_resample_pro, render_resample_pro_with_ratios, sine_wave, steady_region};
use crate::PitchShiftRatios;

const SAMPLE_RATE: u32 = 48_000;
const SWEEP_SEGMENT_LEN: usize = 16_384;
const BREATH_TONES_HZ: [f32; 8] = [
    13_500.0, 14_700.0, 16_100.0, 17_300.0, 18_500.0, 19_900.0, 21_100.0, 22_300.0,
];

#[test]
fn resample_pro_pitch_up_sine_sweep_rejects_folded_alias_band() {
    let pitch_ratio = 2.0;
    let frequencies_hz = [3_000.0, 15_000.0, 18_000.0, 21_000.0];
    let audio = sine_sweep_segments(&frequencies_hz, SAMPLE_RATE as f32, SWEEP_SEGMENT_LEN);
    let rendered = render_resample_pro_with_ratios(
        &audio,
        SAMPLE_RATE,
        440.0,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: Some(pitch_ratio),
        },
    );

    assert_all_finite(&rendered);
    let legal_segment = source_aligned_segment(&rendered, 0, SWEEP_SEGMENT_LEN);
    let legal_target = windowed_dft_magnitude_at(
        legal_segment,
        SAMPLE_RATE as f32,
        frequencies_hz[0] * pitch_ratio,
    );
    assert!(
        legal_target > 0.02,
        "legal shifted sweep tone should remain measurable; legal_target={legal_target}"
    );

    for (segment_index, frequency_hz) in frequencies_hz.iter().copied().enumerate().skip(1) {
        assert!(frequency_hz > legal_pitch_up_source_bandwidth_hz(pitch_ratio));
        let alias_hz = folded_frequency_hz(frequency_hz * pitch_ratio, SAMPLE_RATE as f32);
        let segment = source_aligned_segment(&rendered, segment_index, SWEEP_SEGMENT_LEN);
        let alias = windowed_dft_magnitude_at(segment, SAMPLE_RATE as f32, alias_hz);

        assert!(
            alias <= legal_target * db_to_gain(-55.0),
            "Resample Pro pitch-up sweep should reject folded aliases; frequency_hz={frequency_hz}, alias_hz={alias_hz}, alias={alias}, legal_target={legal_target}"
        );
    }
}

#[test]
fn resample_pro_bright_breath_noise_does_not_fold_into_inband_artifacts() {
    let pitch_ratio = 2.0;
    let audio = bright_breath_noise(SAMPLE_RATE as f32, SAMPLE_RATE as usize);
    let source_level = rms(steady_region(&audio, 4_096));
    let rendered = render_resample_pro_with_ratios(
        &audio,
        SAMPLE_RATE,
        440.0,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: Some(pitch_ratio),
        },
    );
    let rendered_level = rms(steady_region(&rendered, 8_192));

    assert_all_finite(&rendered);
    assert!(
        rendered_level <= source_level * db_to_gain(-38.0),
        "bright breath components above legal bandwidth should not fold into audible in-band output; source_level={source_level}, rendered_level={rendered_level}"
    );

    for frequency_hz in BREATH_TONES_HZ {
        let alias_hz = folded_frequency_hz(frequency_hz * pitch_ratio, SAMPLE_RATE as f32);
        let alias = windowed_dft_magnitude_at(
            steady_region(&rendered, 8_192),
            SAMPLE_RATE as f32,
            alias_hz,
        );
        assert!(
            alias <= source_level * db_to_gain(-48.0),
            "bright breath tone should not fold into alias; frequency_hz={frequency_hz}, alias_hz={alias_hz}, alias={alias}, source_level={source_level}"
        );
    }
}

#[test]
fn resample_pro_downshift_preserves_low_frequency_phase_continuity() {
    let source_hz = 220.0;
    let pitch_ratio = 0.5;
    let target_hz = source_hz * pitch_ratio;
    let audio = sine_wave(source_hz, SAMPLE_RATE, SAMPLE_RATE as usize);
    let rendered = render_resample_pro(&audio, SAMPLE_RATE, source_hz, pitch_ratio);
    let steady = steady_region(&rendered, 8_192);
    let f0 =
        estimate_f0_autocorrelation(steady, SAMPLE_RATE as f32, 80.0, 140.0).unwrap_or_default();
    let fitted_error = fitted_sine_rms_error(steady, SAMPLE_RATE as f32, target_hz);
    let relative_error = fitted_error / rms(steady).max(1.0e-9);
    let high_artifact_ratio = high_frequency_artifact_ratio(steady, SAMPLE_RATE as f32, target_hz);
    let zero_crossing_jitter = zero_crossing_period_jitter(steady);
    let adjacent_delta = max_adjacent_delta(steady);

    assert!(
        (f0 - target_hz).abs() < 2.0,
        "downshift should hit low target pitch; target_hz={target_hz}, f0={f0}"
    );
    assert!(
        relative_error <= db_to_gain(-40.0),
        "downshift low sine residual should stay below -40 dB; relative_error={relative_error}"
    );
    assert!(
        high_artifact_ratio <= db_to_gain(-60.0),
        "downshift should not add bitcrushed high-frequency texture; ratio={high_artifact_ratio}"
    );
    assert!(
        zero_crossing_jitter <= 0.01,
        "downshift zero-crossing period jitter should stay below 1%; jitter={zero_crossing_jitter}"
    );
    assert!(
        adjacent_delta <= 0.08,
        "downshift low-frequency output should not contain large adjacent-sample steps; adjacent_delta={adjacent_delta}"
    );
}

fn sine_sweep_segments(frequencies_hz: &[f32], sample_rate: f32, segment_len: usize) -> Vec<f32> {
    frequencies_hz
        .iter()
        .flat_map(|frequency_hz| {
            (0..segment_len).map(move |index| {
                let phase = std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate;
                phase.sin() * segment_fade(index, segment_len)
            })
        })
        .collect()
}

fn bright_breath_noise(sample_rate: f32, len: usize) -> Vec<f32> {
    let mut audio = vec![0.0; len];
    for (tone_index, frequency_hz) in BREATH_TONES_HZ.iter().copied().enumerate() {
        let phase_offset = tone_index as f32 * 1.927;
        for (index, sample) in audio.iter_mut().enumerate() {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate;
            *sample += (phase + phase_offset).sin() * 0.08;
        }
    }
    audio
        .into_iter()
        .enumerate()
        .map(|(index, sample)| sample * segment_fade(index, len))
        .collect()
}

fn segment_fade(index: usize, len: usize) -> f32 {
    let ramp_len = 1_024.min(len / 4).max(1);
    let fade_in = if index < ramp_len {
        raised_cosine(index as f32 / ramp_len as f32)
    } else {
        1.0
    };
    let samples_from_end = len.saturating_sub(index).saturating_sub(1);
    let fade_out = if samples_from_end < ramp_len {
        raised_cosine(samples_from_end as f32 / ramp_len as f32)
    } else {
        1.0
    };
    fade_in.min(fade_out)
}

fn raised_cosine(position: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::PI * position.clamp(0.0, 1.0)).cos()
}

fn source_aligned_segment(samples: &[f32], segment_index: usize, segment_len: usize) -> &[f32] {
    let start = segment_index * segment_len;
    let end = start + segment_len;
    let trim = 4_096.min(segment_len / 4);
    &samples[start + trim..end - trim]
}

fn legal_pitch_up_source_bandwidth_hz(pitch_ratio: f32) -> f32 {
    SAMPLE_RATE as f32 * 0.5 / pitch_ratio.max(f32::EPSILON)
}

fn max_adjacent_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}
