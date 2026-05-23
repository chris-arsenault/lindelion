use std::{
    error::Error,
    fmt,
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use lindelion_dsp_utils::{
    analysis, interpolation,
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

    let config = config.sanitized();
    let analysis_audio = resample_to_swiftf0_rate(audio, sample_rate);
    let (pitch_hz, confidence) = run_swiftf0(&analysis_audio)?;
    let frames = pitch_hz
        .iter()
        .copied()
        .zip(confidence.iter().copied())
        .enumerate()
        .map(|(frame_index, (raw_f0_hz, confidence))| {
            let timestamp_seconds = swiftf0_timestamp_seconds(frame_index);
            let source_sample_position = (timestamp_seconds * sample_rate as f32).round() as usize;
            let rms = frame_rms(&analysis_audio, frame_index);
            let raw_f0_hz = finite_or(raw_f0_hz, 0.0);
            let confidence = finite_clamp(confidence, 0.0, 1.0, 0.0);
            let voiced = confidence >= config.confidence_threshold
                && (config.fmin_hz..=config.fmax_hz).contains(&raw_f0_hz);
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
        })
        .collect();

    Ok(PitchContour {
        source_sample_rate: sample_rate,
        analysis_sample_rate: SWIFTF0_TARGET_SAMPLE_RATE,
        hop_size: SWIFTF0_HOP_SIZE,
        frames,
    })
}

pub fn resample_to_swiftf0_rate(audio: &[f32], source_sample_rate: u32) -> Vec<f32> {
    if source_sample_rate == SWIFTF0_TARGET_SAMPLE_RATE {
        return sanitize_audio(audio);
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
    sanitize_audio(&out)
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
    let mut input = sanitize_audio(audio);
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

fn sanitize_audio(audio: &[f32]) -> Vec<f32> {
    audio
        .iter()
        .copied()
        .map(|sample| finite_or(sample, 0.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
