const DEFAULT_TAPS: usize = 96;
const PITCH_SHIFT_TAPS: usize = 384;
const PITCH_SHIFT_TRANSITION_GUARD_RATIO: f64 = 0.12;
const MIN_TAPS: usize = 8;
const MIN_CUTOFF_RATIO: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResampleQuality {
    taps: usize,
    transition_guard_ratio: f64,
}

impl ResampleQuality {
    pub const fn standard() -> Self {
        Self {
            taps: DEFAULT_TAPS,
            transition_guard_ratio: 0.0,
        }
    }

    pub const fn pitch_shift() -> Self {
        Self {
            taps: PITCH_SHIFT_TAPS,
            transition_guard_ratio: PITCH_SHIFT_TRANSITION_GUARD_RATIO,
        }
    }

    pub fn new(taps: usize, transition_guard_ratio: f64) -> Self {
        Self {
            taps: sanitize_taps(taps),
            transition_guard_ratio: sanitize_transition_guard_ratio(transition_guard_ratio),
        }
    }

    pub fn taps(self) -> usize {
        sanitize_taps(self.taps)
    }

    pub fn transition_guard_ratio(self) -> f64 {
        sanitize_transition_guard_ratio(self.transition_guard_ratio)
    }

    pub fn cutoff_ratio_for_read_ratio(self, read_ratio: f64) -> f64 {
        let read_ratio = sanitize_read_ratio(read_ratio);
        let legal_cutoff = legal_shifted_bandwidth_cutoff_ratio(read_ratio);
        if read_ratio > 1.0 {
            (legal_cutoff * (1.0 - self.transition_guard_ratio())).clamp(MIN_CUTOFF_RATIO, 1.0)
        } else {
            legal_cutoff
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowedSincResampler {
    quality: ResampleQuality,
}

impl WindowedSincResampler {
    pub fn new(taps: usize) -> Self {
        Self::with_quality(ResampleQuality::new(taps, 0.0))
    }

    pub fn with_quality(quality: ResampleQuality) -> Self {
        Self { quality }
    }

    pub fn pitch_shift() -> Self {
        Self::with_quality(ResampleQuality::pitch_shift())
    }

    pub const fn default_taps() -> usize {
        DEFAULT_TAPS
    }

    pub fn taps(self) -> usize {
        self.quality.taps()
    }

    pub fn quality(self) -> ResampleQuality {
        self.quality
    }

    pub fn sample(self, samples: &[f32], position: f64, cutoff_ratio: f64) -> f32 {
        if samples.is_empty() || !position.is_finite() {
            return 0.0;
        }

        let cutoff_ratio = sanitize_cutoff_ratio(cutoff_ratio);
        let half_taps = self.taps() / 2;
        let center = position.floor() as isize;
        let start = center.saturating_sub(half_taps as isize - 1);
        let mut sample_sum = 0.0;
        let mut weight_sum = 0.0;

        for tap in 0..self.taps() {
            let sample_index = start.saturating_add(tap as isize);
            let Some(sample) = sample_at(samples, sample_index) else {
                continue;
            };
            let distance = position - sample_index as f64;
            let weight = lowpass_sinc(distance, cutoff_ratio)
                * blackman_harris_window(distance, half_taps as f64);
            sample_sum += sample as f64 * weight;
            weight_sum += weight;
        }

        if weight_sum.abs() > f64::EPSILON {
            (sample_sum / weight_sum) as f32
        } else {
            0.0
        }
    }

    pub fn render_to(self, input: &[f32], read_ratio: f64, output: &mut [f32]) {
        let read_ratio = sanitize_read_ratio(read_ratio);
        let cutoff_ratio = self.quality.cutoff_ratio_for_read_ratio(read_ratio);
        let mut position = 0.0;
        for sample in output {
            *sample = self.sample(input, position, cutoff_ratio);
            position += read_ratio;
        }
    }
}

impl Default for WindowedSincResampler {
    fn default() -> Self {
        Self::with_quality(ResampleQuality::standard())
    }
}

pub fn cutoff_ratio_for_read_ratio(read_ratio: f64) -> f64 {
    ResampleQuality::standard().cutoff_ratio_for_read_ratio(read_ratio)
}

pub fn legal_shifted_bandwidth_cutoff_ratio(read_ratio: f64) -> f64 {
    let read_ratio = sanitize_read_ratio(read_ratio);
    if read_ratio > 1.0 {
        (1.0 / read_ratio).clamp(MIN_CUTOFF_RATIO, 1.0)
    } else {
        1.0
    }
}

fn sanitize_taps(taps: usize) -> usize {
    let taps = taps.max(MIN_TAPS);
    if taps.is_multiple_of(2) {
        taps
    } else {
        taps + 1
    }
}

fn sanitize_read_ratio(read_ratio: f64) -> f64 {
    if read_ratio.is_finite() && read_ratio > 0.0 {
        read_ratio
    } else {
        1.0
    }
}

fn sanitize_cutoff_ratio(cutoff_ratio: f64) -> f64 {
    if cutoff_ratio.is_finite() {
        cutoff_ratio.clamp(MIN_CUTOFF_RATIO, 1.0)
    } else {
        1.0
    }
}

fn sanitize_transition_guard_ratio(transition_guard_ratio: f64) -> f64 {
    if transition_guard_ratio.is_finite() {
        transition_guard_ratio.clamp(0.0, 0.5)
    } else {
        0.0
    }
}

fn sample_at(samples: &[f32], index: isize) -> Option<f32> {
    usize::try_from(index)
        .ok()
        .and_then(|index| samples.get(index).copied())
}

fn lowpass_sinc(distance: f64, cutoff_ratio: f64) -> f64 {
    let cutoff = 0.5 * cutoff_ratio;
    2.0 * cutoff * normalized_sinc(2.0 * cutoff * distance)
}

fn normalized_sinc(x: f64) -> f64 {
    if x.abs() <= f64::EPSILON {
        1.0
    } else {
        let phase = std::f64::consts::PI * x;
        phase.sin() / phase
    }
}

fn blackman_harris_window(distance: f64, half_taps: f64) -> f64 {
    let normalized = (distance / half_taps).abs();
    if normalized >= 1.0 {
        0.0
    } else {
        0.35875
            + 0.48829 * (std::f64::consts::PI * normalized).cos()
            + 0.14128 * (std::f64::consts::TAU * normalized).cos()
            + 0.01168 * (3.0 * std::f64::consts::PI * normalized).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis::{
            dft_magnitude_at, folded_frequency_hz, rms, rms_difference, windowed_dft_magnitude_at,
            zero_crossing_period_jitter,
        },
        db_to_gain,
    };

    #[test]
    fn unity_ratio_is_transparent_away_from_edges() {
        let sample_rate = 48_000.0;
        let input = sine_wave(1_000.0, sample_rate, 4_096);
        let mut output = vec![0.0; input.len()];

        WindowedSincResampler::default().render_to(&input, 1.0, &mut output);

        let trim = WindowedSincResampler::default_taps();
        let error = rms_difference(
            &input[trim..input.len() - trim],
            &output[trim..output.len() - trim],
        ) / rms(&input[trim..input.len() - trim]).max(1.0e-9);
        assert!(error < 1.0e-6, "unity resample error={error}");
    }

    #[test]
    fn impulse_response_is_symmetric() {
        let mut input = vec![0.0; 257];
        let center = 128usize;
        input[center] = 1.0;
        let resampler = WindowedSincResampler::default();

        for offset in [0.125, 0.25, 0.5, 0.75] {
            let left = resampler.sample(&input, center as f64 - offset, 1.0);
            let right = resampler.sample(&input, center as f64 + offset, 1.0);
            assert!(
                (left - right).abs() < 1.0e-6,
                "offset={offset}, left={left}, right={right}"
            );
        }
    }

    #[test]
    fn pitch_up_read_ratio_rejects_unshiftable_high_frequency() {
        let sample_rate = 48_000.0;
        let input = sine_wave(16_000.0, sample_rate, 48_000);
        let mut output = vec![0.0; input.len() / 2];

        WindowedSincResampler::default().render_to(&input, 2.0, &mut output);

        let rejected = dft_magnitude_at(&output[256..output.len() - 256], sample_rate, 16_000.0);
        let input_level = rms(&input);
        assert!(
            rejected <= input_level * 0.001,
            "rejected={rejected}, input_level={input_level}"
        );
    }

    #[test]
    fn pitch_shift_quality_cutoff_tracks_legal_shifted_bandwidth() {
        let quality = ResampleQuality::pitch_shift();
        for read_ratio in [1.25, 1.5, 2.0] {
            let legal_cutoff = legal_shifted_bandwidth_cutoff_ratio(read_ratio);
            let guarded_cutoff = quality.cutoff_ratio_for_read_ratio(read_ratio);

            assert!(
                (legal_cutoff - 1.0 / read_ratio).abs() < 1.0e-12,
                "legal_cutoff={legal_cutoff}, read_ratio={read_ratio}"
            );
            assert!(
                guarded_cutoff < legal_cutoff,
                "guarded_cutoff={guarded_cutoff}, legal_cutoff={legal_cutoff}"
            );
            assert!(
                guarded_cutoff > legal_cutoff * 0.85,
                "guard should leave legal shifted bandwidth mostly intact; guarded_cutoff={guarded_cutoff}, legal_cutoff={legal_cutoff}"
            );
        }
        assert_eq!(quality.cutoff_ratio_for_read_ratio(0.5), 1.0);
    }

    #[test]
    fn pitch_shift_sine_sweep_rejects_folded_alias_band() {
        let sample_rate = 48_000.0;
        let read_ratio = 2.0;
        let segment_len = 8_192;
        let frequencies_hz = [3_000.0, 15_000.0, 18_000.0, 21_000.0];
        let input = sine_sweep_segments(&frequencies_hz, sample_rate, segment_len);
        let mut output = vec![0.0; input.len() / read_ratio as usize];

        WindowedSincResampler::pitch_shift().render_to(&input, read_ratio, &mut output);

        let legal_segment = resampled_segment(&output, 0, segment_len, read_ratio);
        let legal_target = windowed_dft_magnitude_at(
            legal_segment,
            sample_rate as f32,
            (frequencies_hz[0] * read_ratio) as f32,
        );
        for (segment_index, frequency_hz) in frequencies_hz.iter().copied().enumerate().skip(1) {
            assert!(
                frequency_hz > legal_shifted_bandwidth_hz(sample_rate, read_ratio),
                "test frequency should be outside legal pitch-up bandwidth"
            );
            let alias_hz =
                folded_frequency_hz((frequency_hz * read_ratio) as f32, sample_rate as f32);
            let segment = resampled_segment(&output, segment_index, segment_len, read_ratio);
            let alias = windowed_dft_magnitude_at(segment, sample_rate as f32, alias_hz);

            assert!(
                alias <= legal_target * db_to_gain(-72.0),
                "pitch-up sweep alias should be at least 72 dB below legal shifted tone; frequency_hz={frequency_hz}, alias_hz={alias_hz}, alias={alias}, legal_target={legal_target}"
            );
        }
    }

    #[test]
    fn downshift_preserves_low_frequency_continuity() {
        let sample_rate = 48_000.0;
        let input = sine_wave(220.0, sample_rate, 48_000);
        let mut output = vec![0.0; input.len() * 2];

        WindowedSincResampler::pitch_shift().render_to(&input, 0.5, &mut output);

        let trim = WindowedSincResampler::pitch_shift().taps() * 2;
        let steady = &output[trim..output.len() - trim];
        let target_hz = 110.0;
        let fitted_error =
            crate::analysis::fitted_sine_rms_error(steady, sample_rate, target_hz as f32);
        let relative_error = fitted_error / rms(steady).max(1.0e-9);
        let jitter = zero_crossing_period_jitter(steady);

        assert!(
            relative_error <= db_to_gain(-80.0),
            "downshift low-frequency sine residual should stay below -80 dB; relative_error={relative_error}"
        );
        assert!(
            jitter <= 0.001,
            "downshift zero-crossing jitter should stay below 0.1%; jitter={jitter}"
        );
    }

    fn legal_shifted_bandwidth_hz(sample_rate: f64, read_ratio: f64) -> f64 {
        sample_rate * 0.5 * legal_shifted_bandwidth_cutoff_ratio(read_ratio)
    }

    fn sine_sweep_segments(
        frequencies_hz: &[f64],
        sample_rate: f64,
        segment_len: usize,
    ) -> Vec<f32> {
        frequencies_hz
            .iter()
            .flat_map(|frequency_hz| {
                (0..segment_len).map(move |index| {
                    let phase = std::f64::consts::TAU * frequency_hz * index as f64 / sample_rate;
                    (phase.sin() * segment_fade(index, segment_len) as f64) as f32
                })
            })
            .collect()
    }

    fn segment_fade(index: usize, len: usize) -> f32 {
        let ramp_len = 512.min(len / 4).max(1);
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

    fn resampled_segment(
        output: &[f32],
        input_segment_index: usize,
        input_segment_len: usize,
        read_ratio: f64,
    ) -> &[f32] {
        let start = (input_segment_index as f64 * input_segment_len as f64 / read_ratio) as usize;
        let len = (input_segment_len as f64 / read_ratio) as usize;
        let trim = 512.min(len / 4);
        &output[start + trim..start + len - trim]
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
