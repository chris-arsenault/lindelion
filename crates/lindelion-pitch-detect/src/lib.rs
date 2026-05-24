use std::{error::Error, fmt};

use lindelion_dsp_utils::{
    analysis,
    math::{finite_clamp, finite_or},
};
use serde::{Deserialize, Serialize};

mod swiftf0;

pub use swiftf0::{
    SwiftF0Detector, SwiftF0StreamingPitchTracker, detect_pitch_contour, resample_to_swiftf0_rate,
};

pub const SWIFTF0_TARGET_SAMPLE_RATE: u32 = 16_000;
pub const SWIFTF0_HOP_SIZE: usize = 256;
pub const SWIFTF0_FRAME_SIZE: usize = 1024;
pub const SWIFTF0_CENTER_OFFSET_SAMPLES: f32 = 127.5;
pub const SWIFTF0_MODEL_FMIN_HZ: f32 = 46.875;
pub const SWIFTF0_MODEL_FMAX_HZ: f32 = 2_093.75;
pub const SWIFTF0_MODEL_BYTES: &[u8] = include_bytes!("../assets/swift_f0.onnx");

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchDetectionConfig {
    pub confidence_threshold: f32,
    pub fmin_hz: f32,
    pub fmax_hz: f32,
}

impl Default for PitchDetectionConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.5,
            fmin_hz: SWIFTF0_MODEL_FMIN_HZ,
            fmax_hz: SWIFTF0_MODEL_FMAX_HZ,
        }
    }
}

impl PitchDetectionConfig {
    pub fn sanitized(self) -> Self {
        let fmin_hz = finite_clamp(
            self.fmin_hz,
            SWIFTF0_MODEL_FMIN_HZ,
            SWIFTF0_MODEL_FMAX_HZ,
            SWIFTF0_MODEL_FMIN_HZ,
        );
        let fmax_hz = finite_clamp(
            self.fmax_hz,
            fmin_hz,
            SWIFTF0_MODEL_FMAX_HZ,
            SWIFTF0_MODEL_FMAX_HZ,
        );
        Self {
            confidence_threshold: finite_clamp(self.confidence_threshold, 0.0, 1.0, 0.5),
            fmin_hz,
            fmax_hz,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchFrame {
    pub frame_index: usize,
    pub source_sample_position: usize,
    pub timestamp_seconds: f32,
    pub f0_hz: Option<f32>,
    pub raw_f0_hz: f32,
    pub confidence: f32,
    pub voiced: bool,
    pub rms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchContour {
    pub source_sample_rate: u32,
    pub analysis_sample_rate: u32,
    pub hop_size: usize,
    pub frames: Vec<PitchFrame>,
}

impl PitchContour {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn source_frame_hop_samples(&self) -> usize {
        source_frame_hop_samples(self)
    }

    pub fn frames_in_range(&self, start: usize, end: usize) -> &[PitchFrame] {
        frames_in_range(self, start, end)
    }
}

pub fn source_frame_hop_samples(contour: &PitchContour) -> usize {
    contour
        .frames
        .windows(2)
        .find_map(|window| {
            window[1]
                .source_sample_position
                .checked_sub(window[0].source_sample_position)
                .filter(|hop| *hop > 0)
        })
        .unwrap_or_else(|| {
            (contour.hop_size.max(1) as f32 * contour.source_sample_rate.max(1) as f32
                / contour.analysis_sample_rate.max(1) as f32)
                .round()
                .max(1.0) as usize
        })
}

pub fn frames_in_range(contour: &PitchContour, start: usize, end: usize) -> &[PitchFrame] {
    let first = contour
        .frames
        .partition_point(|frame| frame.source_sample_position < start);
    let last = contour
        .frames
        .partition_point(|frame| frame.source_sample_position < end);
    &contour.frames[first..last]
}

pub fn median_voiced_pitch(frames: &[PitchFrame]) -> Option<f32> {
    analysis::median_finite_positive(frames.iter().filter_map(|frame| frame.f0_hz))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PitchDetectionError {
    EmptyInput,
    InvalidSampleRate,
    Model(String),
    MalformedOutput,
}

impl fmt::Display for PitchDetectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(formatter, "pitch detection input is empty"),
            Self::InvalidSampleRate => write!(formatter, "pitch detection sample rate is invalid"),
            Self::Model(error) => write!(formatter, "SwiftF0 model error: {error}"),
            Self::MalformedOutput => write!(formatter, "SwiftF0 model returned malformed output"),
        }
    }
}

impl Error for PitchDetectionError {}

pub trait PitchDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32) -> Result<PitchContour, PitchDetectionError>;

    fn detect_with_config(
        &self,
        audio: &[f32],
        sample_rate: u32,
        _config: PitchDetectionConfig,
    ) -> Result<PitchContour, PitchDetectionError> {
        self.detect(audio, sample_rate)
    }
}

pub trait StreamingPitchTracker {
    fn next_block(&mut self, audio: &[f32]) -> Result<&[PitchFrame], PitchDetectionError>;
    fn reset(&mut self);
}

#[derive(Debug, Clone)]
pub struct ZeroCrossingStreamingPitchTracker {
    config: PitchDetectionConfig,
    source_sample_rate: u32,
    source_samples_seen: usize,
    block_frames: [PitchFrame; 1],
    block_frame_count: usize,
}

impl ZeroCrossingStreamingPitchTracker {
    pub fn new(source_sample_rate: u32, config: PitchDetectionConfig) -> Self {
        Self {
            config: config.sanitized(),
            source_sample_rate: source_sample_rate.max(1),
            source_samples_seen: 0,
            block_frames: [PitchFrame {
                frame_index: 0,
                source_sample_position: 0,
                timestamp_seconds: 0.0,
                f0_hz: None,
                raw_f0_hz: 0.0,
                confidence: 0.0,
                voiced: false,
                rms: 0.0,
            }],
            block_frame_count: 0,
        }
    }

    pub const fn config(&self) -> PitchDetectionConfig {
        self.config
    }

    pub const fn source_sample_rate(&self) -> u32 {
        self.source_sample_rate
    }

    fn pitch_frame_from_block(&self, audio: &[f32]) -> Option<PitchFrame> {
        let rms = analysis::rms(audio);
        let raw_f0_hz = zero_crossing_pitch_hz(audio, self.source_sample_rate)?;
        let confidence = if rms > 0.000_001 { 0.95 } else { 0.0 };
        let voiced = confidence >= self.config.confidence_threshold
            && (self.config.fmin_hz..=self.config.fmax_hz).contains(&raw_f0_hz);

        Some(PitchFrame {
            frame_index: self.source_samples_seen,
            source_sample_position: self.source_samples_seen,
            timestamp_seconds: self.source_samples_seen as f32 / self.source_sample_rate as f32,
            f0_hz: voiced.then_some(raw_f0_hz),
            raw_f0_hz,
            confidence,
            voiced,
            rms,
        })
    }
}

impl StreamingPitchTracker for ZeroCrossingStreamingPitchTracker {
    fn next_block(&mut self, audio: &[f32]) -> Result<&[PitchFrame], PitchDetectionError> {
        self.block_frame_count = 0;
        if let Some(frame) = self.pitch_frame_from_block(audio) {
            self.block_frames[0] = frame;
            self.block_frame_count = 1;
        }
        self.source_samples_seen = self.source_samples_seen.saturating_add(audio.len());
        Ok(&self.block_frames[..self.block_frame_count])
    }

    fn reset(&mut self) {
        self.source_samples_seen = 0;
        self.block_frame_count = 0;
    }
}

fn zero_crossing_pitch_hz(audio: &[f32], sample_rate: u32) -> Option<f32> {
    if audio.len() < 3 || sample_rate == 0 {
        return None;
    }

    let mut previous = finite_or(audio[0], 0.0);
    let mut first_crossing = None;
    let mut last_crossing = 0.0;
    let mut crossing_count = 0usize;

    for (index, sample) in audio.iter().copied().enumerate().skip(1) {
        let current = finite_or(sample, 0.0);
        if previous <= 0.0 && current > 0.0 {
            let denominator = current - previous;
            let fraction = if denominator.abs() > f32::EPSILON {
                -previous / denominator
            } else {
                0.0
            };
            let crossing = index as f32 - 1.0 + fraction;
            first_crossing.get_or_insert(crossing);
            last_crossing = crossing;
            crossing_count += 1;
        }
        previous = current;
    }

    let first_crossing = first_crossing?;
    if crossing_count < 2 {
        return None;
    }

    let span = last_crossing - first_crossing;
    if span <= f32::EPSILON {
        return None;
    }

    let period_samples = span / (crossing_count - 1) as f32;
    (period_samples > f32::EPSILON)
        .then_some(sample_rate as f32 / period_samples)
        .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
}

#[cfg(test)]
mod tests;
