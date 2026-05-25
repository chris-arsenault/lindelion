use std::{error::Error, fmt};

use lindelion_dsp_utils::{
    interpolation,
    math::{finite_clamp, semitones_to_ratio, snap_to_zero},
};
use serde::{Deserialize, Serialize};

use crate::{PitchShiftFrameAnalysis, PitchShiftSourceCache};

pub const DEFAULT_MAX_HARMONICS: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftRatios {
    pub pitch_ratio: f32,
    pub formant_ratio: Option<f32>,
}

impl PitchShiftRatios {
    pub const fn identity() -> Self {
        Self {
            pitch_ratio: 1.0,
            formant_ratio: None,
        }
    }

    pub fn from_semitones_cents(semitones: f32, cents: f32) -> Self {
        Self {
            pitch_ratio: semitones_to_ratio(semitones + cents / 100.0),
            formant_ratio: None,
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            pitch_ratio: finite_clamp(self.pitch_ratio, 0.125, 8.0, 1.0),
            formant_ratio: self
                .formant_ratio
                .map(|ratio| finite_clamp(ratio, 0.125, 8.0, 1.0)),
        }
    }

    pub fn effective_formant_ratio(self) -> f32 {
        self.sanitized().formant_ratio.unwrap_or(1.0)
    }
}

impl Default for PitchShiftRatios {
    fn default() -> Self {
        Self::identity()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResidualMixPolicy {
    Preserve,
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftRenderConfig {
    pub ratios: PitchShiftRatios,
    pub residual_policy: ResidualMixPolicy,
    pub harmonic_level: f32,
    pub residual_level: f32,
    pub unvoiced_level: f32,
    pub max_harmonics: usize,
}

impl Default for PitchShiftRenderConfig {
    fn default() -> Self {
        Self {
            ratios: PitchShiftRatios::identity(),
            residual_policy: ResidualMixPolicy::Preserve,
            harmonic_level: 1.0,
            residual_level: 1.0,
            unvoiced_level: 1.0,
            max_harmonics: DEFAULT_MAX_HARMONICS,
        }
    }
}

impl PitchShiftRenderConfig {
    pub fn sanitized(self) -> Self {
        Self {
            ratios: self.ratios.sanitized(),
            residual_policy: self.residual_policy,
            harmonic_level: finite_clamp(self.harmonic_level, 0.0, 4.0, 1.0),
            residual_level: finite_clamp(self.residual_level, 0.0, 4.0, 1.0),
            unvoiced_level: finite_clamp(self.unvoiced_level, 0.0, 4.0, 1.0),
            max_harmonics: self.max_harmonics.clamp(1, 512),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftSliceRenderRequest {
    pub slice_index: usize,
    pub config: PitchShiftRenderConfig,
}

impl PitchShiftSliceRenderRequest {
    pub fn new(slice_index: usize, ratios: PitchShiftRatios) -> Self {
        Self {
            slice_index,
            config: PitchShiftRenderConfig {
                ratios,
                ..PitchShiftRenderConfig::default()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftSliceSampleRequest {
    pub slice_index: usize,
    pub offset_samples: f32,
    pub config: PitchShiftRenderConfig,
}

impl PitchShiftSliceSampleRequest {
    pub fn new(slice_index: usize, offset_samples: f32, ratios: PitchShiftRatios) -> Self {
        Self {
            slice_index,
            offset_samples,
            config: PitchShiftRenderConfig {
                ratios,
                ..PitchShiftRenderConfig::default()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchShiftRenderError {
    EmptySource,
    InvalidCache,
    MissingSlice,
    OutputLength { expected: usize, actual: usize },
}

impl fmt::Display for PitchShiftRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySource => write!(formatter, "pitch-shift render source is empty"),
            Self::InvalidCache => write!(formatter, "pitch-shift render cache is invalid"),
            Self::MissingSlice => write!(formatter, "pitch-shift render slice is missing"),
            Self::OutputLength { expected, actual } => {
                write!(
                    formatter,
                    "pitch-shift render output length {actual} != {expected}"
                )
            }
        }
    }
}

impl Error for PitchShiftRenderError {}

#[derive(Debug, Default, Clone, Copy)]
pub struct PitchShiftEngine;

impl PitchShiftEngine {
    pub fn render_slice(
        &self,
        source: &[f32],
        cache: &PitchShiftSourceCache,
        request: PitchShiftSliceRenderRequest,
    ) -> Result<Vec<f32>, PitchShiftRenderError> {
        let duration = slice_duration(cache, request.slice_index)?;
        let mut output = vec![0.0; duration];
        self.render_slice_to(source, cache, request, &mut output)?;
        Ok(output)
    }

    pub fn render_slice_to(
        &self,
        source: &[f32],
        cache: &PitchShiftSourceCache,
        request: PitchShiftSliceRenderRequest,
        output: &mut [f32],
    ) -> Result<(), PitchShiftRenderError> {
        if source.is_empty() {
            return Err(PitchShiftRenderError::EmptySource);
        }
        if cache.sample_rate == 0 || cache.frames.is_empty() {
            return Err(PitchShiftRenderError::InvalidCache);
        }
        let slice = cache
            .slice_summary(request.slice_index)
            .ok_or(PitchShiftRenderError::MissingSlice)?;
        let duration = slice.end_sample.saturating_sub(slice.start_sample);
        if output.len() != duration {
            return Err(PitchShiftRenderError::OutputLength {
                expected: duration,
                actual: output.len(),
            });
        }

        render_region_to(
            source,
            cache,
            slice.start_sample,
            request.config.sanitized(),
            output,
        );
        Ok(())
    }

    pub fn render_slice_sample(
        &self,
        source: &[f32],
        cache: &PitchShiftSourceCache,
        request: PitchShiftSliceSampleRequest,
    ) -> Result<f32, PitchShiftRenderError> {
        if source.is_empty() {
            return Err(PitchShiftRenderError::EmptySource);
        }
        if cache.sample_rate == 0 || cache.frames.is_empty() {
            return Err(PitchShiftRenderError::InvalidCache);
        }
        let slice = cache
            .slice_summary(request.slice_index)
            .ok_or(PitchShiftRenderError::MissingSlice)?;
        let duration = slice.end_sample.saturating_sub(slice.start_sample) as f32;
        let offset = finite_clamp(request.offset_samples, 0.0, duration, 0.0);
        if offset >= duration {
            return Ok(0.0);
        }

        Ok(render_sample_at_offset(
            source,
            cache,
            slice.start_sample,
            offset,
            request.config.sanitized(),
        ))
    }
}

fn slice_duration(
    cache: &PitchShiftSourceCache,
    slice_index: usize,
) -> Result<usize, PitchShiftRenderError> {
    cache
        .slice_summary(slice_index)
        .map(|slice| slice.end_sample.saturating_sub(slice.start_sample))
        .ok_or(PitchShiftRenderError::MissingSlice)
}

fn render_region_to(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    config: PitchShiftRenderConfig,
    output: &mut [f32],
) {
    for (offset, sample) in output.iter_mut().enumerate() {
        *sample = render_sample_at_offset(source, cache, start_sample, offset as f32, config);
    }
}

fn render_sample_at_offset(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    offset_samples: f32,
    config: PitchShiftRenderConfig,
) -> f32 {
    let source_position = start_sample as f32 + offset_samples;
    let source_index = source_position
        .floor()
        .clamp(0.0, source.len().saturating_sub(1) as f32) as usize;
    let frame = frame_at_position(&cache.frames, source_index);
    let source_sample = interpolation::linear(source, source_position);
    let harmonic = voiced_harmonic_sample(frame, offset_samples, cache.sample_rate as f32, config);
    let residual = residual_sample(source_sample, frame, config);
    snap_to_zero(harmonic + residual)
}

fn frame_at_position(
    frames: &[PitchShiftFrameAnalysis],
    position_samples: usize,
) -> &PitchShiftFrameAnalysis {
    let index = frames.partition_point(|frame| frame.center_sample <= position_samples);
    &frames[index.saturating_sub(1).min(frames.len() - 1)]
}

fn voiced_harmonic_sample(
    frame: &PitchShiftFrameAnalysis,
    offset_samples: f32,
    sample_rate: f32,
    config: PitchShiftRenderConfig,
) -> f32 {
    let Some(f0_hz) = frame.f0_hz.filter(|f0| frame.voiced && *f0 > 0.0) else {
        return 0.0;
    };
    let ratios = config.ratios.sanitized();
    let target_f0 = f0_hz * ratios.pitch_ratio;
    let nyquist = sample_rate * 0.49;
    if target_f0 <= 0.0 || target_f0 >= nyquist {
        return 0.0;
    }

    let harmonic_count = ((nyquist / target_f0) as usize).min(config.max_harmonics);
    let formant_ratio = ratios.effective_formant_ratio();
    let magnitude_floor = frame.spectral_envelope.peak_magnitude() * 0.03;
    let mut sample = 0.0;
    let mut magnitude_sum = 0.0;
    for harmonic in 1..=harmonic_count {
        let frequency = target_f0 * harmonic as f32;
        let envelope_frequency = frequency / formant_ratio;
        let magnitude = frame
            .spectral_envelope
            .magnitude_at(envelope_frequency)
            .max(magnitude_floor);
        let phase = std::f32::consts::TAU * frequency * offset_samples / sample_rate;
        sample += phase.sin() * magnitude;
        magnitude_sum += magnitude.abs();
    }

    if magnitude_sum <= f32::EPSILON {
        0.0
    } else {
        sample / magnitude_sum * frame.rms * config.harmonic_level
    }
}

fn residual_sample(
    source_sample: f32,
    frame: &PitchShiftFrameAnalysis,
    config: PitchShiftRenderConfig,
) -> f32 {
    match config.residual_policy {
        ResidualMixPolicy::Muted => 0.0,
        ResidualMixPolicy::Preserve if frame.voiced => {
            source_sample * frame.residual.aperiodic_ratio * config.residual_level
        }
        ResidualMixPolicy::Preserve => source_sample * config.unvoiced_level,
    }
}
