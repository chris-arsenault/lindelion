use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use serde::{Deserialize, Serialize};

use crate::PitchShiftAnalysisConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftSourceCache {
    pub key: SourceCacheKey,
    pub sample_rate: u32,
    pub source_len_samples: usize,
    pub config: PitchShiftAnalysisConfig,
    pub frames: Vec<PitchShiftFrameAnalysis>,
    pub epoch_samples: Vec<usize>,
    pub voicing_segments: Vec<VoicingSegment>,
    pub slice_summaries: Vec<PitchShiftSliceSummary>,
}

impl PitchShiftSourceCache {
    pub fn slice_summary(&self, slice_index: usize) -> Option<&PitchShiftSliceSummary> {
        self.slice_summaries
            .iter()
            .find(|summary| summary.slice_index == slice_index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCacheKey {
    pub sample_rate: u32,
    pub source_len_samples: usize,
    pub audio_hash: u64,
    pub marker_hash: u64,
    pub pitch_contour_hash: u64,
    pub config_hash: u64,
}

impl SourceCacheKey {
    pub fn from_inputs(
        audio: &[f32],
        sample_rate: u32,
        pitch_contour: &PitchContour,
        markers: &[SliceMarker],
        config: PitchShiftAnalysisConfig,
    ) -> Self {
        Self {
            sample_rate,
            source_len_samples: audio.len(),
            audio_hash: hash_audio(audio),
            marker_hash: hash_markers(markers),
            pitch_contour_hash: hash_pitch_contour(pitch_contour),
            config_hash: hash_config(config),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftFrameAnalysis {
    pub frame_index: usize,
    pub center_sample: usize,
    pub start_sample: usize,
    pub end_sample: usize,
    pub f0_hz: Option<f32>,
    pub confidence: f32,
    pub voiced: bool,
    pub rms: f32,
    pub harmonic_magnitudes: Vec<f32>,
    pub spectral_envelope: SpectralEnvelope,
    pub residual: ResidualEnergyDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralEnvelope {
    pub harmonic_spacing_hz: Option<f32>,
    pub points: Vec<SpectralEnvelopePoint>,
}

impl SpectralEnvelope {
    pub fn peak_magnitude(&self) -> f32 {
        self.points
            .iter()
            .map(|point| point.magnitude)
            .fold(0.0, f32::max)
    }

    pub fn magnitude_at(&self, frequency_hz: f32) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if frequency_hz <= first.frequency_hz {
            return first.magnitude;
        }

        for pair in self.points.windows(2) {
            let left = pair[0];
            let right = pair[1];
            if frequency_hz <= right.frequency_hz {
                let span = (right.frequency_hz - left.frequency_hz).max(f32::EPSILON);
                let t = ((frequency_hz - left.frequency_hz) / span).clamp(0.0, 1.0);
                return left.magnitude + (right.magnitude - left.magnitude) * t;
            }
        }

        self.points
            .last()
            .map(|point| point.magnitude)
            .unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpectralEnvelopePoint {
    pub frequency_hz: f32,
    pub magnitude: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResidualEnergyDescriptor {
    pub total_energy: f32,
    pub harmonic_energy: f32,
    pub residual_energy: f32,
    pub aperiodic_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoicingKind {
    Voiced,
    Unvoiced,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VoicingSegment {
    pub kind: VoicingKind,
    pub start_sample: usize,
    pub end_sample: usize,
    pub frame_count: usize,
    pub median_f0_hz: Option<f32>,
    pub mean_confidence: f32,
    pub mean_rms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchShiftSliceSummary {
    pub slice_index: usize,
    pub start_sample: usize,
    pub end_sample: usize,
    pub detected_f0_hz: Option<f32>,
    pub mean_confidence: f32,
    pub voiced_ratio: f32,
    pub mean_residual_energy: f32,
    pub mean_aperiodic_ratio: f32,
}

fn hash_audio(audio: &[f32]) -> u64 {
    let mut state = StableHasher::new();
    for sample in audio {
        state.write_f32(*sample);
    }
    state.finish()
}

fn hash_markers(markers: &[SliceMarker]) -> u64 {
    let mut state = StableHasher::new();
    for marker in markers {
        state.write_usize(marker.position_samples);
        state.write_u8(match marker.kind {
            MarkerKind::Auto => 0,
            MarkerKind::User => 1,
        });
    }
    state.finish()
}

fn hash_pitch_contour(contour: &PitchContour) -> u64 {
    let mut state = StableHasher::new();
    state.write_u32(contour.source_sample_rate);
    state.write_u32(contour.analysis_sample_rate);
    state.write_usize(contour.hop_size);
    for frame in &contour.frames {
        hash_pitch_frame(&mut state, frame);
    }
    state.finish()
}

fn hash_pitch_frame(state: &mut StableHasher, frame: &PitchFrame) {
    state.write_usize(frame.frame_index);
    state.write_usize(frame.source_sample_position);
    state.write_f32(frame.timestamp_seconds);
    state.write_f32(frame.f0_hz.unwrap_or(0.0));
    state.write_f32(frame.raw_f0_hz);
    state.write_f32(frame.confidence);
    state.write_u8(u8::from(frame.voiced));
    state.write_f32(frame.rms);
}

fn hash_config(config: PitchShiftAnalysisConfig) -> u64 {
    let mut state = StableHasher::new();
    state.write_usize(config.frame_size);
    state.write_usize(config.envelope_points);
    state.write_f32(config.envelope_smoothing_harmonics);
    state.write_f32(config.min_voiced_confidence);
    state.finish()
}

struct StableHasher(u64);

impl StableHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET)
    }

    const fn finish(self) -> u64 {
        self.0
    }

    fn write_u8(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(Self::PRIME);
    }

    fn write_u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_f32(&mut self, value: f32) {
        let value = if value.is_finite() { value } else { 0.0 };
        self.write_u32(value.to_bits());
    }
}
