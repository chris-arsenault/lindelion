use std::{
    error::Error,
    fmt,
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use lindelion_dsp_utils::{
    analysis::{self, append_sanitized_audio, sanitize_audio_to_vec},
    interpolation,
    math::{finite_clamp, finite_or},
};
use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

pub const SWIFTF0_TARGET_SAMPLE_RATE: u32 = 16_000;
pub const SWIFTF0_HOP_SIZE: usize = 256;
pub const SWIFTF0_FRAME_SIZE: usize = 1024;
pub const SWIFTF0_CENTER_OFFSET_SAMPLES: f32 = 127.5;
pub const SWIFTF0_MODEL_FMIN_HZ: f32 = 46.875;
pub const SWIFTF0_MODEL_FMAX_HZ: f32 = 2_093.75;
pub const SWIFTF0_MODEL_BYTES: &[u8] = include_bytes!("../assets/swift_f0.onnx");

type SwiftF0Model = Arc<TypedSimplePlan>;
type SwiftF0ModelCache = Mutex<Option<(usize, Result<SwiftF0Model, String>)>>;

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
pub struct SwiftF0Detector {
    config: PitchDetectionConfig,
}

impl Default for SwiftF0Detector {
    fn default() -> Self {
        Self::new(PitchDetectionConfig::default())
    }
}

impl SwiftF0Detector {
    pub fn new(config: PitchDetectionConfig) -> Self {
        Self {
            config: config.sanitized(),
        }
    }

    pub const fn config(&self) -> PitchDetectionConfig {
        self.config
    }
}

impl PitchDetector for SwiftF0Detector {
    fn detect(&self, audio: &[f32], sample_rate: u32) -> Result<PitchContour, PitchDetectionError> {
        detect_pitch_contour(audio, sample_rate, self.config)
    }

    fn detect_with_config(
        &self,
        audio: &[f32],
        sample_rate: u32,
        config: PitchDetectionConfig,
    ) -> Result<PitchContour, PitchDetectionError> {
        detect_pitch_contour(audio, sample_rate, config)
    }
}

pub fn detect_pitch_contour(
    audio: &[f32],
    sample_rate: u32,
    config: PitchDetectionConfig,
) -> Result<PitchContour, PitchDetectionError> {
    if audio.is_empty() {
        return Err(PitchDetectionError::EmptyInput);
    }
    if sample_rate == 0 {
        return Err(PitchDetectionError::InvalidSampleRate);
    }

    let mut tracker = SwiftF0StreamingPitchTracker::new(sample_rate, config);
    let mut frames = Vec::new();
    frames.extend_from_slice(tracker.next_block(audio)?);
    frames.extend_from_slice(tracker.finish()?);

    Ok(PitchContour {
        source_sample_rate: sample_rate,
        analysis_sample_rate: SWIFTF0_TARGET_SAMPLE_RATE,
        hop_size: SWIFTF0_HOP_SIZE,
        frames,
    })
}

#[derive(Debug, Clone)]
pub struct SwiftF0StreamingPitchTracker {
    config: PitchDetectionConfig,
    source_sample_rate: u32,
    source_samples_seen: usize,
    source_buffer_start: usize,
    source_buffer: Vec<f32>,
    analysis_buffer_start: usize,
    analysis_buffer: Vec<f32>,
    next_analysis_sample_index: usize,
    next_frame_start: usize,
    block_frames: Vec<PitchFrame>,
    finished: bool,
}

impl SwiftF0StreamingPitchTracker {
    pub fn new(source_sample_rate: u32, config: PitchDetectionConfig) -> Self {
        Self {
            config: config.sanitized(),
            source_sample_rate: source_sample_rate.max(1),
            source_samples_seen: 0,
            source_buffer_start: 0,
            source_buffer: Vec::new(),
            analysis_buffer_start: 0,
            analysis_buffer: Vec::new(),
            next_analysis_sample_index: 0,
            next_frame_start: 0,
            block_frames: Vec::new(),
            finished: false,
        }
    }

    pub const fn config(&self) -> PitchDetectionConfig {
        self.config
    }

    pub const fn source_sample_rate(&self) -> u32 {
        self.source_sample_rate
    }

    pub fn finish(&mut self) -> Result<&[PitchFrame], PitchDetectionError> {
        self.block_frames.clear();
        if self.finished || self.source_samples_seen == 0 {
            return Ok(&self.block_frames);
        }

        self.finished = true;
        self.append_remaining_analysis_samples();
        let flush_until = self.expected_analysis_sample_len();
        self.process_available_frames(Some(flush_until))?;
        Ok(&self.block_frames)
    }

    fn append_source_block(&mut self, audio: &[f32]) {
        if audio.is_empty() {
            return;
        }

        self.finished = false;
        if self.source_sample_rate == SWIFTF0_TARGET_SAMPLE_RATE {
            append_sanitized_audio(&mut self.analysis_buffer, audio);
            self.source_samples_seen += audio.len();
            self.next_analysis_sample_index = self.source_samples_seen;
            return;
        }

        append_sanitized_audio(&mut self.source_buffer, audio);
        self.source_samples_seen += audio.len();
        self.append_available_analysis_samples();
        self.drain_source_buffer();
    }

    fn append_available_analysis_samples(&mut self) {
        if self.source_samples_seen == 0 {
            return;
        }

        let last_source_position = self.source_samples_seen.saturating_sub(1) as f32;
        while self.next_analysis_source_position() <= last_source_position {
            self.push_next_analysis_sample();
        }
    }

    fn append_remaining_analysis_samples(&mut self) {
        let target_len = self.expected_analysis_sample_len();
        while self.next_analysis_sample_index < target_len {
            self.push_next_analysis_sample();
        }
    }

    fn push_next_analysis_sample(&mut self) {
        let sample = if self.source_sample_rate == SWIFTF0_TARGET_SAMPLE_RATE {
            self.analysis_buffer.last().copied().unwrap_or(0.0)
        } else {
            let local_position =
                self.next_analysis_source_position() - self.source_buffer_start as f32;
            interpolation::linear(&self.source_buffer, local_position)
        };
        self.analysis_buffer.push(finite_or(sample, 0.0));
        self.next_analysis_sample_index += 1;
    }

    fn process_available_frames(
        &mut self,
        flush_until: Option<usize>,
    ) -> Result<(), PitchDetectionError> {
        let available_end = self.analysis_buffer_start + self.analysis_buffer.len();
        if self.analysis_buffer.is_empty() {
            return Ok(());
        }
        if flush_until.is_none() && self.next_frame_start + SWIFTF0_FRAME_SIZE > available_end {
            return Ok(());
        }
        if let Some(flush_until) = flush_until
            && self.next_frame_start >= flush_until
        {
            return Ok(());
        }

        let (pitch_hz, confidence) = run_swiftf0(&self.analysis_buffer)?;
        let mut last_emitted_start = None;
        for (local_frame_index, (raw_f0_hz, confidence)) in pitch_hz
            .iter()
            .copied()
            .zip(confidence.iter().copied())
            .enumerate()
        {
            let frame_start = self.analysis_buffer_start + local_frame_index * SWIFTF0_HOP_SIZE;
            if frame_start < self.next_frame_start {
                continue;
            }
            if let Some(flush_until) = flush_until {
                if frame_start >= flush_until {
                    break;
                }
            } else if frame_start + SWIFTF0_FRAME_SIZE > available_end {
                break;
            }

            let frame = self.pitch_frame_from_model_output(
                frame_start,
                local_frame_index,
                raw_f0_hz,
                confidence,
            );
            self.block_frames.push(frame);
            last_emitted_start = Some(frame_start);
        }

        if let Some(last_emitted_start) = last_emitted_start {
            self.next_frame_start = last_emitted_start + SWIFTF0_HOP_SIZE;
            self.drain_analysis_buffer();
        }
        Ok(())
    }

    fn pitch_frame_from_model_output(
        &self,
        frame_start: usize,
        local_frame_index: usize,
        raw_f0_hz: f32,
        confidence: f32,
    ) -> PitchFrame {
        let frame_index = frame_start / SWIFTF0_HOP_SIZE;
        let timestamp_seconds = swiftf0_timestamp_seconds(frame_index);
        let source_sample_position =
            (timestamp_seconds * self.source_sample_rate as f32).round() as usize;
        let rms = frame_rms(&self.analysis_buffer, local_frame_index);
        let raw_f0_hz = finite_or(raw_f0_hz, 0.0);
        let confidence = finite_clamp(confidence, 0.0, 1.0, 0.0);
        let voiced = confidence >= self.config.confidence_threshold
            && (self.config.fmin_hz..=self.config.fmax_hz).contains(&raw_f0_hz);
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds,
            f0_hz: voiced.then_some(raw_f0_hz),
            raw_f0_hz,
            confidence,
            voiced,
            rms,
        }
    }

    fn next_analysis_source_position(&self) -> f32 {
        self.next_analysis_sample_index as f32 * self.source_sample_rate as f32
            / SWIFTF0_TARGET_SAMPLE_RATE as f32
    }

    fn expected_analysis_sample_len(&self) -> usize {
        ((self.source_samples_seen as f64 * SWIFTF0_TARGET_SAMPLE_RATE as f64)
            / self.source_sample_rate as f64)
            .ceil()
            .max(1.0) as usize
    }

    fn drain_source_buffer(&mut self) {
        if self.source_buffer.is_empty() {
            return;
        }

        let retain_from = self.next_analysis_source_position().floor().max(1.0) as usize - 1;
        let drain_len = retain_from
            .saturating_sub(self.source_buffer_start)
            .min(self.source_buffer.len());
        if drain_len > 0 {
            self.source_buffer.drain(0..drain_len);
            self.source_buffer_start += drain_len;
        }
    }

    fn drain_analysis_buffer(&mut self) {
        let drain_len = self
            .next_frame_start
            .saturating_sub(self.analysis_buffer_start)
            .min(self.analysis_buffer.len());
        if drain_len > 0 {
            self.analysis_buffer.drain(0..drain_len);
            self.analysis_buffer_start += drain_len;
        }
    }
}

impl StreamingPitchTracker for SwiftF0StreamingPitchTracker {
    fn next_block(&mut self, audio: &[f32]) -> Result<&[PitchFrame], PitchDetectionError> {
        self.block_frames.clear();
        self.append_source_block(audio);
        self.process_available_frames(None)?;
        Ok(&self.block_frames)
    }

    fn reset(&mut self) {
        let config = self.config;
        let source_sample_rate = self.source_sample_rate;
        *self = Self::new(source_sample_rate, config);
    }
}

pub fn resample_to_swiftf0_rate(audio: &[f32], source_sample_rate: u32) -> Vec<f32> {
    if source_sample_rate == SWIFTF0_TARGET_SAMPLE_RATE {
        return sanitize_audio_to_vec(audio);
    }

    let target_len = ((audio.len() as f64 * SWIFTF0_TARGET_SAMPLE_RATE as f64)
        / source_sample_rate.max(1) as f64)
        .ceil()
        .max(1.0) as usize;
    let source_step = source_sample_rate.max(1) as f32 / SWIFTF0_TARGET_SAMPLE_RATE as f32;
    let mut out = Vec::with_capacity(target_len);
    for index in 0..target_len {
        out.push(interpolation::linear(audio, index as f32 * source_step));
    }
    sanitize_audio_to_vec(&out)
}

fn run_swiftf0(audio_16k: &[f32]) -> Result<(Vec<f32>, Vec<f32>), PitchDetectionError> {
    let audio_16k = padded_input(audio_16k);
    let input =
        tract_onnx::prelude::tract_ndarray::Array2::from_shape_vec((1, audio_16k.len()), audio_16k)
            .map_err(|error| PitchDetectionError::Model(error.to_string()))?;
    let outputs = swiftf0_model(input.shape()[1])?
        .run(tvec!(input.into_tensor().into()))
        .map_err(|error| PitchDetectionError::Model(error.to_string()))?;

    if outputs.len() < 2 {
        return Err(PitchDetectionError::MalformedOutput);
    }

    let pitch_hz = tensor_to_vec(&outputs[0])?;
    let confidence = tensor_to_vec(&outputs[1])?;
    if pitch_hz.len() != confidence.len() {
        return Err(PitchDetectionError::MalformedOutput);
    }
    Ok((pitch_hz, confidence))
}

fn swiftf0_model(input_len: usize) -> Result<SwiftF0Model, PitchDetectionError> {
    static MODEL_CACHE: OnceLock<SwiftF0ModelCache> = OnceLock::new();

    let cache = MODEL_CACHE.get_or_init(|| Mutex::new(None));
    let mut cache = cache
        .lock()
        .map_err(|_| PitchDetectionError::Model("SwiftF0 model cache poisoned".to_string()))?;
    if let Some((cached_len, cached_model)) = cache.as_ref()
        && *cached_len == input_len
    {
        return cached_model.clone().map_err(PitchDetectionError::Model);
    }

    let model = build_swiftf0_model(input_len).map_err(|error| format!("{error:?}"));
    *cache = Some((input_len, model.clone()));
    model.map_err(PitchDetectionError::Model)
}

fn build_swiftf0_model(input_len: usize) -> TractResult<SwiftF0Model> {
    tract_onnx::onnx()
        .model_for_read(&mut Cursor::new(SWIFTF0_MODEL_BYTES))
        .and_then(|model| model.with_input_fact(0, f32::fact([1, input_len]).into()))
        .and_then(|model| model.into_optimized())
        .and_then(|model| model.into_runnable())
}

fn tensor_to_vec(tensor: &TValue) -> Result<Vec<f32>, PitchDetectionError> {
    tensor
        .to_plain_array_view::<f32>()
        .map_err(|error| PitchDetectionError::Model(error.to_string()))
        .map(|view| view.iter().copied().collect())
}

fn padded_input(audio: &[f32]) -> Vec<f32> {
    let mut input = sanitize_audio_to_vec(audio);
    if input.len() < SWIFTF0_HOP_SIZE {
        input.resize(SWIFTF0_HOP_SIZE, 0.0);
    }
    input
}

fn swiftf0_timestamp_seconds(frame_index: usize) -> f32 {
    (frame_index as f32 * SWIFTF0_HOP_SIZE as f32 + SWIFTF0_CENTER_OFFSET_SAMPLES)
        / SWIFTF0_TARGET_SAMPLE_RATE as f32
}

fn frame_rms(audio_16k: &[f32], frame_index: usize) -> f32 {
    let start = frame_index * SWIFTF0_HOP_SIZE;
    let end = (start + SWIFTF0_FRAME_SIZE).min(audio_16k.len());
    analysis::rms(audio_16k.get(start..end).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::math::cents_between;

    #[test]
    fn swiftf0_model_bytes_are_embedded() {
        assert!(SWIFTF0_MODEL_BYTES.len() > 300_000);
    }

    #[test]
    fn resampling_preserves_duration() {
        let audio = vec![0.0; 48_000];
        let resampled = resample_to_swiftf0_rate(&audio, 48_000);

        assert_eq!(resampled.len(), 16_000);
    }

    #[test]
    fn silence_is_unvoiced_and_finite() {
        let contour = SwiftF0Detector::default()
            .detect(&vec![0.0; 16_000], 16_000)
            .unwrap();

        assert!(!contour.is_empty());
        assert!(contour.frames.iter().all(|frame| !frame.voiced));
        assert!(
            contour
                .frames
                .iter()
                .all(|frame| frame.confidence.is_finite())
        );
    }

    #[test]
    fn non_finite_input_is_sanitized() {
        let mut audio = vec![0.0; 16_000];
        audio[100] = f32::NAN;
        audio[200] = f32::INFINITY;

        let contour = SwiftF0Detector::default().detect(&audio, 16_000).unwrap();

        assert!(
            contour
                .frames
                .iter()
                .all(|frame| frame.raw_f0_hz.is_finite())
        );
        assert!(contour.frames.iter().all(|frame| frame.rms.is_finite()));
    }

    #[test]
    fn swiftf0_tracks_synthetic_sine_pitch() {
        let audio = sine_wave(440.0, 16_000);

        let contour = SwiftF0Detector::default().detect(&audio, 16_000).unwrap();
        let voiced = contour
            .frames
            .iter()
            .filter_map(|frame| frame.f0_hz)
            .collect::<Vec<_>>();

        assert!(
            voiced.len() >= 4,
            "expected voiced SwiftF0 frames, got {}",
            voiced.len()
        );
        assert_close_cents(median(voiced), 440.0, 80.0);
    }

    #[test]
    fn contour_reports_source_hop_from_frame_positions() {
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 1_000, Some(220.0)),
                pitch_frame(1, 1_768, Some(220.0)),
                pitch_frame(2, 2_536, Some(220.0)),
            ],
        };

        assert_eq!(contour.source_frame_hop_samples(), 768);
    }

    #[test]
    fn contour_hop_fallback_uses_contour_sample_rates() {
        let contour = PitchContour {
            source_sample_rate: 44_100,
            analysis_sample_rate: 22_050,
            hop_size: 128,
            frames: vec![pitch_frame(0, 0, Some(220.0))],
        };

        assert_eq!(contour.source_frame_hop_samples(), 256);
    }

    #[test]
    fn contour_frames_in_range_and_median_use_shared_pitch_frames() {
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0)),
                pitch_frame(1, 768, None),
                pitch_frame(2, 1_536, Some(660.0)),
                pitch_frame(3, 2_304, Some(550.0)),
            ],
        };

        let frames = contour.frames_in_range(700, 2_400);

        assert_eq!(frames.len(), 3);
        assert_eq!(median_voiced_pitch(frames), Some(660.0));
    }

    #[test]
    fn streaming_pitch_tracker_emits_monotonic_frames_across_blocks() {
        let audio = sine_wave(440.0, 16_000);
        let mut tracker =
            SwiftF0StreamingPitchTracker::new(16_000, PitchDetectionConfig::default());
        let mut frames = Vec::new();

        for block in audio.chunks(4_096) {
            frames.extend_from_slice(tracker.next_block(block).unwrap());
        }
        frames.extend_from_slice(tracker.finish().unwrap());

        assert!(frames.len() >= 4);
        assert!(
            frames
                .windows(2)
                .all(|pair| pair[0].source_sample_position < pair[1].source_sample_position)
        );
        let voiced = frames
            .iter()
            .filter_map(|frame| frame.f0_hz)
            .collect::<Vec<_>>();
        assert_close_cents(median(voiced), 440.0, 80.0);
    }

    fn sine_wave(frequency_hz: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| {
                let phase = std::f32::consts::TAU * frequency_hz * index as f32 / 16_000.0;
                phase.sin() * 0.5
            })
            .collect()
    }

    fn median(mut values: Vec<f32>) -> f32 {
        values.sort_by(f32::total_cmp);
        values[values.len() / 2]
    }

    fn assert_close_cents(actual_hz: f32, expected_hz: f32, tolerance_cents: f32) {
        let cents = cents_between(expected_hz, actual_hz);
        assert!(
            cents <= tolerance_cents,
            "expected {actual_hz} Hz within {tolerance_cents} cents of {expected_hz} Hz, got {cents}"
        );
    }

    fn pitch_frame(
        frame_index: usize,
        source_sample_position: usize,
        f0_hz: Option<f32>,
    ) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz,
            raw_f0_hz: f0_hz.unwrap_or(0.0),
            confidence: if f0_hz.is_some() { 0.95 } else { 0.1 },
            voiced: f0_hz.is_some(),
            rms: if f0_hz.is_some() { 0.2 } else { 0.0 },
        }
    }
}
