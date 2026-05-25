use std::{error::Error, fmt};

use lindelion_dsp_utils::{
    analysis,
    math::{finite_clamp, finite_or},
};
use lindelion_onset_detect::{SliceMarker, slice_regions_from_markers};
use lindelion_pitch_detect::{PitchContour, median_voiced_pitch};
use serde::{Deserialize, Serialize};

use crate::{
    PitchShiftFrameAnalysis, PitchShiftSliceSummary, PitchShiftSourceCache, SourceCacheKey,
    VoicingKind, VoicingSegment, spectral::analyze_spectral_frame,
};

pub const DEFAULT_FRAME_SIZE: usize = 2048;
pub const DEFAULT_ENVELOPE_POINTS: usize = 64;
pub const DEFAULT_ENVELOPE_SMOOTHING_HARMONICS: f32 = 1.0;
pub const DEFAULT_MIN_VOICED_CONFIDENCE: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftAnalysisConfig {
    pub frame_size: usize,
    pub envelope_points: usize,
    pub envelope_smoothing_harmonics: f32,
    pub min_voiced_confidence: f32,
}

impl Default for PitchShiftAnalysisConfig {
    fn default() -> Self {
        Self {
            frame_size: DEFAULT_FRAME_SIZE,
            envelope_points: DEFAULT_ENVELOPE_POINTS,
            envelope_smoothing_harmonics: DEFAULT_ENVELOPE_SMOOTHING_HARMONICS,
            min_voiced_confidence: DEFAULT_MIN_VOICED_CONFIDENCE,
        }
    }
}

impl PitchShiftAnalysisConfig {
    pub fn sanitized(self) -> Self {
        Self {
            frame_size: self.frame_size.clamp(256, 8192).next_power_of_two(),
            envelope_points: self.envelope_points.clamp(8, 256),
            envelope_smoothing_harmonics: finite_clamp(
                self.envelope_smoothing_harmonics,
                0.25,
                4.0,
                DEFAULT_ENVELOPE_SMOOTHING_HARMONICS,
            ),
            min_voiced_confidence: finite_clamp(
                self.min_voiced_confidence,
                0.0,
                1.0,
                DEFAULT_MIN_VOICED_CONFIDENCE,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchShiftAnalysisError {
    EmptySource,
    InvalidSampleRate,
    EmptyPitchContour,
}

impl fmt::Display for PitchShiftAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySource => write!(formatter, "pitch-shift analysis source is empty"),
            Self::InvalidSampleRate => {
                write!(formatter, "pitch-shift analysis sample rate is invalid")
            }
            Self::EmptyPitchContour => {
                write!(formatter, "pitch-shift analysis requires an F0 contour")
            }
        }
    }
}

impl Error for PitchShiftAnalysisError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchShiftAnalyzer {
    config: PitchShiftAnalysisConfig,
}

impl Default for PitchShiftAnalyzer {
    fn default() -> Self {
        Self::new(PitchShiftAnalysisConfig::default())
    }
}

impl PitchShiftAnalyzer {
    pub fn new(config: PitchShiftAnalysisConfig) -> Self {
        Self {
            config: config.sanitized(),
        }
    }

    pub const fn config(&self) -> PitchShiftAnalysisConfig {
        self.config
    }

    pub fn analyze(
        &self,
        audio: &[f32],
        sample_rate: u32,
        pitch_contour: &PitchContour,
        markers: &[SliceMarker],
    ) -> Result<PitchShiftSourceCache, PitchShiftAnalysisError> {
        if audio.is_empty() {
            return Err(PitchShiftAnalysisError::EmptySource);
        }
        if sample_rate == 0 {
            return Err(PitchShiftAnalysisError::InvalidSampleRate);
        }
        if pitch_contour.frames.is_empty() {
            return Err(PitchShiftAnalysisError::EmptyPitchContour);
        }

        let config = self.config;
        let frames = analyze_frames(audio, sample_rate, pitch_contour, config);
        let key = SourceCacheKey::from_inputs(audio, sample_rate, pitch_contour, markers, config);
        Ok(PitchShiftSourceCache {
            key,
            sample_rate,
            source_len_samples: audio.len(),
            config,
            epoch_samples: voiced_epoch_samples(audio, sample_rate, &frames),
            voicing_segments: voicing_segments_from_frames(&frames, audio.len()),
            slice_summaries: summarize_slices(markers, audio.len(), pitch_contour, &frames),
            frames,
        })
    }
}

fn analyze_frames(
    audio: &[f32],
    sample_rate: u32,
    pitch_contour: &PitchContour,
    config: PitchShiftAnalysisConfig,
) -> Vec<PitchShiftFrameAnalysis> {
    pitch_contour
        .frames
        .iter()
        .map(|frame| {
            let f0_hz = frame.f0_hz.filter(|f0| f0.is_finite() && *f0 > 0.0);
            let voiced = frame.voiced && frame.confidence >= config.min_voiced_confidence;
            let spectral = analyze_spectral_frame(
                audio,
                sample_rate,
                frame.source_sample_position,
                voiced.then_some(f0_hz).flatten(),
                config,
            );
            PitchShiftFrameAnalysis {
                frame_index: frame.frame_index,
                center_sample: frame.source_sample_position.min(audio.len() - 1),
                start_sample: spectral.start_sample,
                end_sample: spectral.end_sample,
                f0_hz: voiced.then_some(f0_hz).flatten(),
                confidence: finite_clamp(frame.confidence, 0.0, 1.0, 0.0),
                voiced,
                rms: finite_or(frame.rms, 0.0).max(0.0),
                harmonic_magnitudes: spectral.harmonic_magnitudes,
                spectral_envelope: spectral.envelope,
                residual: spectral.residual,
            }
        })
        .collect()
}

fn voicing_segments_from_frames(
    frames: &[PitchShiftFrameAnalysis],
    source_len: usize,
) -> Vec<VoicingSegment> {
    let mut segments = Vec::new();
    let mut start = 0;
    while start < frames.len() {
        let kind = frame_kind(&frames[start]);
        let mut end = start + 1;
        while end < frames.len() && frame_kind(&frames[end]) == kind {
            end += 1;
        }
        segments.push(segment_from_window(kind, &frames[start..end], source_len));
        start = end;
    }
    segments
}

fn voiced_epoch_samples(
    audio: &[f32],
    sample_rate: u32,
    frames: &[PitchShiftFrameAnalysis],
) -> Vec<usize> {
    let Some(first_voiced) = frames.iter().find(|frame| frame.f0_hz.is_some()) else {
        return Vec::new();
    };
    let fallback_period = period_samples(sample_rate, first_voiced.f0_hz.unwrap());
    if fallback_period <= 0.0 {
        return Vec::new();
    }

    let mut expected = first_voiced.center_sample as f32;
    while expected > fallback_period {
        expected -= fallback_period;
    }

    let mut epochs: Vec<usize> = Vec::new();
    let mut guard = 0usize;
    while expected < audio.len() as f32 && guard < audio.len().saturating_mul(2).max(1) {
        guard += 1;
        let period = local_period_samples(frames, sample_rate, expected as usize)
            .unwrap_or(fallback_period)
            .clamp(8.0, sample_rate as f32 / 20.0);
        let search_radius = (period * 0.4).ceil().max(2.0) as usize;
        let candidate = best_positive_zero_crossing(audio, expected, search_radius)
            .unwrap_or_else(|| expected.round().clamp(0.0, audio.len() as f32 - 1.0) as usize);
        let min_gap = (period * 0.45).round().max(1.0) as usize;
        if epochs
            .last()
            .copied()
            .is_none_or(|last| candidate > last.saturating_add(min_gap))
        {
            epochs.push(candidate);
            expected = candidate as f32 + period;
        } else {
            expected += period;
        }
    }
    epochs
}

fn local_period_samples(
    frames: &[PitchShiftFrameAnalysis],
    sample_rate: u32,
    position_samples: usize,
) -> Option<f32> {
    let frame = frame_at_position(frames, position_samples)?;
    frame.f0_hz.map(|f0| period_samples(sample_rate, f0))
}

fn period_samples(sample_rate: u32, f0_hz: f32) -> f32 {
    if f0_hz > 0.0 && f0_hz.is_finite() {
        sample_rate as f32 / f0_hz
    } else {
        0.0
    }
}

fn frame_at_position(
    frames: &[PitchShiftFrameAnalysis],
    position_samples: usize,
) -> Option<&PitchShiftFrameAnalysis> {
    if frames.is_empty() {
        return None;
    }
    let index = frames.partition_point(|frame| frame.center_sample <= position_samples);
    Some(&frames[index.saturating_sub(1).min(frames.len() - 1)])
}

fn best_positive_zero_crossing(
    audio: &[f32],
    expected: f32,
    search_radius: usize,
) -> Option<usize> {
    if audio.len() < 2 {
        return None;
    }
    let center = expected.round().clamp(1.0, audio.len() as f32 - 1.0) as usize;
    let start = center.saturating_sub(search_radius).max(1);
    let end = center
        .saturating_add(search_radius)
        .min(audio.len().saturating_sub(1));
    (start..=end)
        .filter(|index| audio[index - 1] <= 0.0 && audio[*index] > 0.0)
        .max_by(|left, right| {
            zero_crossing_slope(audio, *left).total_cmp(&zero_crossing_slope(audio, *right))
        })
}

fn zero_crossing_slope(audio: &[f32], index: usize) -> f32 {
    audio[index] - audio[index.saturating_sub(1)]
}

fn segment_from_window(
    kind: VoicingKind,
    frames: &[PitchShiftFrameAnalysis],
    source_len: usize,
) -> VoicingSegment {
    let start_sample = frames.first().map(|frame| frame.center_sample).unwrap_or(0);
    let end_sample = frames
        .last()
        .map(|frame| frame.center_sample.saturating_add(1).min(source_len))
        .unwrap_or(source_len);
    VoicingSegment {
        kind,
        start_sample,
        end_sample,
        frame_count: frames.len(),
        median_f0_hz: analysis::median_finite_positive(
            frames.iter().filter_map(|frame| frame.f0_hz),
        ),
        mean_confidence: mean(frames.iter().map(|frame| frame.confidence)),
        mean_rms: mean(frames.iter().map(|frame| frame.rms)),
    }
}

fn summarize_slices(
    markers: &[SliceMarker],
    audio_len: usize,
    pitch_contour: &PitchContour,
    frames: &[PitchShiftFrameAnalysis],
) -> Vec<PitchShiftSliceSummary> {
    slice_regions_from_markers(markers, audio_len)
        .into_iter()
        .map(|region| {
            let contour_frames =
                pitch_contour.frames_in_range(region.start_sample, region.end_sample);
            let analysis_frames = frames_in_range(frames, region.start_sample, region.end_sample);
            let voiced_count = analysis_frames.iter().filter(|frame| frame.voiced).count();
            let frame_count = analysis_frames.len().max(1);
            PitchShiftSliceSummary {
                slice_index: region.index,
                start_sample: region.start_sample,
                end_sample: region.end_sample,
                detected_f0_hz: median_voiced_pitch(contour_frames),
                mean_confidence: mean(
                    analysis_frames
                        .iter()
                        .filter(|frame| frame.voiced)
                        .map(|frame| frame.confidence),
                ),
                voiced_ratio: voiced_count as f32 / frame_count as f32,
                mean_residual_energy: mean(
                    analysis_frames
                        .iter()
                        .map(|frame| frame.residual.residual_energy),
                ),
                mean_aperiodic_ratio: mean(
                    analysis_frames
                        .iter()
                        .map(|frame| frame.residual.aperiodic_ratio),
                ),
            }
        })
        .collect()
}

fn frames_in_range(
    frames: &[PitchShiftFrameAnalysis],
    start_sample: usize,
    end_sample: usize,
) -> &[PitchShiftFrameAnalysis] {
    let first = frames.partition_point(|frame| frame.center_sample < start_sample);
    let last = frames.partition_point(|frame| frame.center_sample < end_sample);
    &frames[first..last]
}

fn frame_kind(frame: &PitchShiftFrameAnalysis) -> VoicingKind {
    if frame.voiced {
        VoicingKind::Voiced
    } else {
        VoicingKind::Unvoiced
    }
}

fn mean(values: impl IntoIterator<Item = f32>) -> f32 {
    let (sum, count) = values
        .into_iter()
        .filter(|value| value.is_finite())
        .fold((0.0, 0usize), |(sum, count), value| {
            (sum + value, count + 1)
        });
    if count == 0 { 0.0 } else { sum / count as f32 }
}
