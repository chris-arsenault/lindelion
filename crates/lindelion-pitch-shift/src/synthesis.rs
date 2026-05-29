use std::{error::Error, fmt};

use lindelion_dsp_utils::{
    interpolation,
    math::{finite_clamp, semitones_to_ratio, snap_to_zero},
};
use serde::{Deserialize, Serialize};

use crate::{
    PitchShiftFrameAnalysis, PitchShiftSourceCache, pitch_synchronous_synthesis,
    resample_pro_render, resample_pro_stretch, resample_stretch_compat, spectral_peak_synthesis,
    synthesis_support::{frame_at_position, residual_sample},
    varispeed_synthesis,
};

pub const DEFAULT_MAX_HARMONICS: usize = 96;
const FORMANT_RATIO_MATCH_EPSILON: f32 = 1.0e-4;
const HARMONIC_MAGNITUDE_FLOOR_RATIO: f32 = 0.001;
const FORMANT_PRESERVE_MAGNITUDE_FLOOR_RATIO: f32 = 0.03;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PitchShiftSynthesisAlgorithm {
    #[default]
    Auto,
    Varispeed,
    SpectralPeak,
    PitchSynchronous,
    ResampleStretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftRenderConfig {
    #[serde(default)]
    pub algorithm: PitchShiftSynthesisAlgorithm,
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
            algorithm: PitchShiftSynthesisAlgorithm::Auto,
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
            algorithm: self.algorithm,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftRegionSampleRequest {
    pub start_sample: usize,
    pub end_sample: usize,
    pub offset_samples: f32,
    pub phase_offset_samples: Option<f32>,
    pub config: PitchShiftRenderConfig,
}

impl PitchShiftRegionSampleRequest {
    pub fn new(
        start_sample: usize,
        end_sample: usize,
        offset_samples: f32,
        ratios: PitchShiftRatios,
    ) -> Self {
        Self {
            start_sample,
            end_sample,
            offset_samples,
            phase_offset_samples: None,
            config: PitchShiftRenderConfig {
                ratios,
                ..PitchShiftRenderConfig::default()
            },
        }
    }

    pub fn with_phase_offset(mut self, phase_offset_samples: f32) -> Self {
        self.phase_offset_samples = Some(phase_offset_samples);
        self
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
#[derive(Debug, Clone, Copy)]
struct RenderSampleOffsets {
    source: f32,
    phase: f32,
}

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

    pub fn render_resample_pro_unity_stretch(
        &self,
        cache: &PitchShiftSourceCache,
    ) -> Result<Vec<f32>, PitchShiftRenderError> {
        self.render_resample_pro_stretch(cache, 1.0)
    }

    pub fn render_resample_pro_stretch(
        &self,
        cache: &PitchShiftSourceCache,
        stretch_ratio: f64,
    ) -> Result<Vec<f32>, PitchShiftRenderError> {
        if cache.source_len_samples == 0 {
            return Err(PitchShiftRenderError::EmptySource);
        }
        resample_pro_stretch::render_stretch(cache, stretch_ratio)
            .map_err(|_| PitchShiftRenderError::InvalidCache)
    }

    pub fn render_resample_pro_pitch_shift(
        &self,
        cache: &PitchShiftSourceCache,
        ratios: PitchShiftRatios,
    ) -> Result<Vec<f32>, PitchShiftRenderError> {
        if cache.source_len_samples == 0 {
            return Err(PitchShiftRenderError::EmptySource);
        }
        resample_pro_render::render_pitch_shift(cache, ratios)
            .map_err(|_| PitchShiftRenderError::InvalidCache)
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

        let config = request.config.sanitized();
        if matches!(
            config.algorithm,
            PitchShiftSynthesisAlgorithm::ResampleStretch
        ) && !is_identity_pitch_request(config.ratios)
        {
            resample_stretch_compat::render_region_to(
                source,
                cache,
                slice.start_sample,
                slice.end_sample,
                config.ratios,
                output,
            )?;
        } else {
            render_region_to(
                source,
                cache,
                slice.start_sample,
                slice.end_sample,
                config,
                output,
            );
        }
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
        self.render_region_sample(
            source,
            cache,
            PitchShiftRegionSampleRequest {
                start_sample: slice.start_sample,
                end_sample: slice.end_sample,
                offset_samples: request.offset_samples,
                phase_offset_samples: None,
                config: request.config,
            },
        )
    }

    pub fn render_region_sample(
        &self,
        source: &[f32],
        cache: &PitchShiftSourceCache,
        request: PitchShiftRegionSampleRequest,
    ) -> Result<f32, PitchShiftRenderError> {
        if source.is_empty() {
            return Err(PitchShiftRenderError::EmptySource);
        }
        if cache.sample_rate == 0 || cache.frames.is_empty() {
            return Err(PitchShiftRenderError::InvalidCache);
        }
        let start_sample = request.start_sample.min(source.len());
        let end_sample = request.end_sample.min(source.len()).max(start_sample);
        let duration = end_sample.saturating_sub(start_sample) as f32;
        let offset = finite_clamp(request.offset_samples, 0.0, duration, 0.0);
        if offset >= duration {
            return Ok(0.0);
        }

        Ok(render_sample_at_offset(
            source,
            cache,
            start_sample,
            end_sample,
            RenderSampleOffsets {
                source: offset,
                phase: request.phase_offset_samples.unwrap_or(offset),
            },
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
    end_sample: usize,
    config: PitchShiftRenderConfig,
    output: &mut [f32],
) {
    for (offset, sample) in output.iter_mut().enumerate() {
        *sample = render_sample_at_offset(
            source,
            cache,
            start_sample,
            end_sample,
            RenderSampleOffsets {
                source: offset as f32,
                phase: offset as f32,
            },
            config,
        );
    }
}

fn render_sample_at_offset(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
    offsets: RenderSampleOffsets,
    config: PitchShiftRenderConfig,
) -> f32 {
    let source_position = start_sample as f64 + offsets.source as f64;
    let ratios = config.ratios.sanitized();
    if is_identity_pitch_request(ratios) {
        return interpolation::linear_f64(source, source_position);
    }
    if let Some(sample) =
        render_algorithm_sample(source, cache, start_sample, end_sample, offsets, config)
    {
        return snap_to_zero(sample);
    }

    let source_index = source_position
        .floor()
        .clamp(0.0, source.len().saturating_sub(1) as f64) as usize;
    let frame = frame_at_position(&cache.frames, source_index);
    let source_sample = interpolation::linear_f64(source, source_position);
    // Drive the harmonic phase from the absolute sample position (like the
    // spectral-peak path), so adjacent regions/slices join without a phase reset.
    let phase_position = start_sample as f64 + offsets.phase as f64;
    let harmonic = voiced_harmonic_sample(frame, phase_position, cache.sample_rate as f32, config);
    let residual = residual_sample(source_sample, frame, config);
    snap_to_zero(harmonic + residual)
}

fn render_algorithm_sample(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
    offsets: RenderSampleOffsets,
    config: PitchShiftRenderConfig,
) -> Option<f32> {
    let ratios = config.ratios.sanitized();
    match config.algorithm {
        PitchShiftSynthesisAlgorithm::Varispeed => varispeed_synthesis::varispeed_sample(
            source,
            start_sample,
            end_sample,
            offsets.source,
            ratios.pitch_ratio,
        ),
        PitchShiftSynthesisAlgorithm::Auto if spectral_peak_model_preferred(ratios) => {
            spectral_peak_synthesis::spectral_peak_model_sample(
                cache,
                start_sample as f64 + offsets.source as f64,
                start_sample as f64 + offsets.phase as f64,
                config,
            )
        }
        PitchShiftSynthesisAlgorithm::Auto if formant_ratio_tracks_pitch(ratios) => {
            pitch_synchronous_synthesis::pitch_synchronous_sample(
                source,
                cache,
                start_sample,
                end_sample,
                offsets.source,
                config,
            )
        }
        PitchShiftSynthesisAlgorithm::SpectralPeak => {
            spectral_peak_synthesis::spectral_peak_model_sample(
                cache,
                start_sample as f64 + offsets.source as f64,
                start_sample as f64 + offsets.phase as f64,
                config,
            )
        }
        PitchShiftSynthesisAlgorithm::PitchSynchronous => {
            pitch_synchronous_synthesis::pitch_synchronous_sample(
                source,
                cache,
                start_sample,
                end_sample,
                offsets.source,
                config,
            )
        }
        PitchShiftSynthesisAlgorithm::ResampleStretch => {
            let position = start_sample as f64 + offsets.source as f64;
            resample_stretch_compat::render_sample(source, cache, position, ratios)
        }
        PitchShiftSynthesisAlgorithm::Auto => None,
    }
}

/// Additive harmonic (sinusoidal) resynthesis fallback: reconstructs the voiced
/// frame as a sum of harmonics whose phase is anchored to the absolute sample
/// position. This is not a phase vocoder — there is no inter-frame phase
/// accumulation; `phase_position` is the absolute sample index for the harmonic
/// phase so adjacent regions stay phase-continuous.
fn voiced_harmonic_sample(
    frame: &PitchShiftFrameAnalysis,
    phase_position: f64,
    sample_rate: f32,
    config: PitchShiftRenderConfig,
) -> f32 {
    let Some(f0_hz) = frame.f0_hz.filter(|f0| frame.voiced && *f0 > 0.0) else {
        return 0.0;
    };
    let ratios = config.ratios.sanitized();
    let sample_rate = sample_rate as f64;
    let target_f0 = f0_hz as f64 * ratios.pitch_ratio as f64;
    let nyquist = sample_rate * 0.49;
    if target_f0 <= 0.0 || target_f0 >= nyquist {
        return 0.0;
    }

    let harmonic_count = ((nyquist / target_f0) as usize).min(config.max_harmonics);
    let formant_ratio = ratios.effective_formant_ratio() as f64;
    let use_source_harmonics = formant_ratio_tracks_pitch(ratios);
    let peak_magnitude = frame.spectral_envelope.peak_magnitude();
    let skip_floor = peak_magnitude * HARMONIC_MAGNITUDE_FLOOR_RATIO;
    let preserve_floor = peak_magnitude * FORMANT_PRESERVE_MAGNITUDE_FLOOR_RATIO;
    let mut sample = 0.0_f64;
    let mut magnitude_sum = 0.0_f64;
    for harmonic in 1..=harmonic_count {
        let frequency = target_f0 * harmonic as f64;
        let envelope_frequency = frequency / formant_ratio;
        let magnitude = harmonic_magnitude(
            frame,
            harmonic,
            envelope_frequency as f32,
            use_source_harmonics,
        );
        if use_source_harmonics && magnitude <= skip_floor {
            continue;
        }
        let magnitude = if use_source_harmonics {
            magnitude
        } else {
            magnitude.max(preserve_floor)
        };
        let phase = std::f64::consts::TAU * frequency * phase_position / sample_rate;
        sample += phase.sin() * magnitude as f64;
        magnitude_sum += magnitude.abs() as f64;
    }

    if magnitude_sum <= f64::EPSILON {
        0.0
    } else {
        (sample / magnitude_sum * frame.rms as f64 * config.harmonic_level as f64) as f32
    }
}

fn harmonic_magnitude(
    frame: &PitchShiftFrameAnalysis,
    harmonic: usize,
    envelope_frequency: f32,
    use_source_harmonics: bool,
) -> f32 {
    if use_source_harmonics {
        frame
            .harmonic_magnitudes
            .get(harmonic.saturating_sub(1))
            .copied()
            .unwrap_or_else(|| frame.spectral_envelope.magnitude_at(envelope_frequency))
    } else {
        frame.spectral_envelope.magnitude_at(envelope_frequency)
    }
}

fn formant_ratio_tracks_pitch(ratios: PitchShiftRatios) -> bool {
    ratios.formant_ratio.is_some_and(|formant_ratio| {
        (formant_ratio - ratios.pitch_ratio).abs() <= FORMANT_RATIO_MATCH_EPSILON
    })
}

fn is_identity_pitch_request(ratios: PitchShiftRatios) -> bool {
    (ratios.pitch_ratio - 1.0).abs() <= FORMANT_RATIO_MATCH_EPSILON
        && ratios
            .formant_ratio
            .is_none_or(|ratio| (ratio - 1.0).abs() <= FORMANT_RATIO_MATCH_EPSILON)
}

fn spectral_peak_model_preferred(ratios: PitchShiftRatios) -> bool {
    ratios.formant_ratio.is_none()
}
