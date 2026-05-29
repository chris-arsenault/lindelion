use lindelion_dsp_utils::{phase, window};
use realfft::RealFftPlanner;

use crate::{PitchShiftAnalysisConfig, ResampleProCache, ResampleProFrame};
use lindelion_onset_detect::SliceMarker;

/// STFT overlap factor: `analysis_hop = fft_size / OVERLAP_FACTOR`. 8 → 87.5 % overlap.
///
/// Chosen from the M4 overlap sweep re-run on the **real** fixture library. The synthetic
/// battery sat at the inter-partial measurement floor and showed only sub-audible differences
/// (which had pointed at 75 %); on real tonal/vocal material 87.5 % overlap lowers the
/// inter-partial phasiness floor by 8–15 dB vs 75 % (e.g. cello −118 → −129 dB, sung vocal
/// −59 → −68 dB) with no measured transient softening on the real transients tested (the
/// synthetic pure-impulse crest drop was an artifact of that fixture). The doubled frame count
/// is an offline-analysis cost only — the render path is setup-time, not the audio thread.
const OVERLAP_FACTOR: usize = 8;

pub(crate) fn analyze_resample_pro(
    audio: &[f32],
    sample_rate: u32,
    markers: &[SliceMarker],
    config: PitchShiftAnalysisConfig,
) -> ResampleProCache {
    let fft_size = config.frame_size.clamp(1024, 8192).next_power_of_two();
    let analysis_hop = (fft_size / OVERLAP_FACTOR).max(1);
    let synthesis_hop = analysis_hop;
    let window = (0..fft_size)
        .map(|index| window::sqrt_hann_f64(index, fft_size))
        .collect::<Vec<_>>();
    let frames = analyze_frames(audio, fft_size, analysis_hop, &window);
    let transient_samples = transient_samples(markers, audio.len());
    let transient_frames = transient_frames(&transient_samples, &frames);

    ResampleProCache {
        sample_rate,
        fft_size,
        analysis_hop,
        synthesis_hop,
        window,
        frames,
        transient_frames,
        transient_samples,
    }
}

fn analyze_frames(
    audio: &[f32],
    fft_size: usize,
    analysis_hop: usize,
    window: &[f64],
) -> Vec<ResampleProFrame> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let bin_count = fft_size / 2 + 1;
    let mut previous_phases: Option<Vec<f64>> = None;
    let mut frames = Vec::new();

    for (frame_index, start_sample) in frame_starts(audio.len(), analysis_hop).enumerate() {
        let mut input = fft.make_input_vec();
        for (index, sample) in input.iter_mut().enumerate() {
            let source = audio
                .get(start_sample + index)
                .copied()
                .filter(|sample| sample.is_finite())
                .unwrap_or(0.0) as f64;
            *sample = source * window[index];
        }

        let mut spectrum = fft.make_output_vec();
        if fft.process(&mut input, &mut spectrum).is_err() {
            continue;
        }

        let magnitudes = spectrum
            .iter()
            .map(|bin| bin.norm_sqr().sqrt())
            .collect::<Vec<_>>();
        let phases = spectrum
            .iter()
            .map(|bin| bin.im.atan2(bin.re))
            .collect::<Vec<_>>();
        let instantaneous_frequency_rad_per_sample =
            instantaneous_frequencies(&phases, previous_phases.as_deref(), fft_size, analysis_hop);
        let peak_owner_by_bin = peak_owners(&magnitudes);
        previous_phases = Some(phases.clone());

        debug_assert_eq!(magnitudes.len(), bin_count);
        frames.push(ResampleProFrame {
            frame_index,
            start_sample,
            center_sample: start_sample
                .saturating_add(fft_size / 2)
                .min(audio.len().saturating_sub(1)),
            magnitudes,
            phases,
            instantaneous_frequency_rad_per_sample,
            peak_owner_by_bin,
        });
    }

    frames
}

fn frame_starts(audio_len: usize, hop_size: usize) -> impl Iterator<Item = usize> {
    let mut next = 0usize;
    let hop_size = hop_size.max(1);
    std::iter::from_fn(move || {
        if audio_len == 0 || next >= audio_len {
            return None;
        }
        let current = next;
        next = next.saturating_add(hop_size);
        Some(current)
    })
}

fn instantaneous_frequencies(
    phases: &[f64],
    previous_phases: Option<&[f64]>,
    fft_size: usize,
    analysis_hop: usize,
) -> Vec<f64> {
    let hop = analysis_hop.max(1) as f64;
    phases
        .iter()
        .copied()
        .enumerate()
        .map(|(bin, current_phase)| {
            let expected_delta = std::f64::consts::TAU * bin as f64 * hop / fft_size as f64;
            let Some(previous_phase) = previous_phases.and_then(|values| values.get(bin).copied())
            else {
                return expected_delta / hop;
            };
            let delta = phase::principal_angle(current_phase - previous_phase - expected_delta);
            (expected_delta + delta) / hop
        })
        .collect()
}

fn peak_owners(magnitudes: &[f64]) -> Vec<u16> {
    let peaks = spectral_peak_bins(magnitudes);
    if peaks.is_empty() {
        return (0..magnitudes.len())
            .map(|bin| bin.min(u16::MAX as usize) as u16)
            .collect();
    }

    (0..magnitudes.len())
        .map(|bin| {
            peaks
                .iter()
                .copied()
                .min_by_key(|peak| peak.abs_diff(bin))
                .unwrap_or(bin)
                .min(u16::MAX as usize) as u16
        })
        .collect()
}

fn spectral_peak_bins(magnitudes: &[f64]) -> Vec<usize> {
    if magnitudes.len() < 3 {
        return Vec::new();
    }
    let peak_floor = magnitudes.iter().copied().fold(0.0, f64::max) * 0.01;
    (1..magnitudes.len() - 1)
        .filter(|index| {
            magnitudes[*index] > peak_floor
                && magnitudes[*index] >= magnitudes[index - 1]
                && magnitudes[*index] > magnitudes[index + 1]
        })
        .collect()
}

fn transient_samples(markers: &[SliceMarker], source_len: usize) -> Vec<usize> {
    let mut transient_samples = markers
        .iter()
        .map(|marker| marker.position_samples.min(source_len.saturating_sub(1)))
        .collect::<Vec<_>>();
    transient_samples.sort_unstable();
    transient_samples.dedup();
    transient_samples
}

fn transient_frames(transient_samples: &[usize], frames: &[ResampleProFrame]) -> Vec<usize> {
    if frames.is_empty() {
        return Vec::new();
    }

    let mut transient_frames = transient_samples
        .iter()
        .copied()
        .map(|position| nearest_frame_index(frames, position))
        .collect::<Vec<_>>();
    transient_frames.sort_unstable();
    transient_frames.dedup();
    transient_frames
}

fn nearest_frame_index(frames: &[ResampleProFrame], position_samples: usize) -> usize {
    let right = frames.partition_point(|frame| frame.center_sample <= position_samples);
    let left = right.saturating_sub(1).min(frames.len() - 1);
    let right = right.min(frames.len() - 1);
    let left_distance = frames[left].center_sample.abs_diff(position_samples);
    let right_distance = frames[right].center_sample.abs_diff(position_samples);
    if right_distance < left_distance {
        right
    } else {
        left
    }
}
