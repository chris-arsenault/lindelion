use lindelion_dsp_utils::{analysis, math::finite_or};
use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionAlgorithm {
    SuperFlux,
    ComplexFlux,
    SpectralSparsity,
    PitchStability,
    EnergyTransient,
    ManualGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerKind {
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceMarker {
    pub position_samples: usize,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AlgorithmParams {
    SuperFlux {
        lookback_frames: u32,
        max_filter_radius: u32,
    },
    ComplexFlux {
        lookback_frames: u32,
        group_delay_weight: f32,
    },
    SpectralSparsity {
        window_size: usize,
    },
    PitchStability {
        threshold_cents: f32,
        min_stable_duration_ms: f32,
    },
    EnergyTransient {
        frame_size: usize,
    },
    ManualGrid {
        divisions: usize,
        offset_ms: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub algorithm: DetectionAlgorithm,
    pub sensitivity: f32,
    pub min_slice_ms: f32,
    pub params: AlgorithmParams,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity: 0.5,
            min_slice_ms: 50.0,
            params: AlgorithmParams::SuperFlux {
                lookback_frames: 3,
                max_filter_radius: 3,
            },
        }
    }
}

pub trait OnsetDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchTrackFrame {
    pub source_sample_position: usize,
    pub f0_hz: Option<f32>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchTrack<'a> {
    pub source_sample_rate: u32,
    pub frame_hop_samples: usize,
    pub frames: &'a [PitchTrackFrame],
}

#[derive(Debug, Default)]
pub struct ConfiguredOnsetDetector;

impl OnsetDetector for ConfiguredOnsetDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker> {
        match config.algorithm {
            DetectionAlgorithm::SuperFlux
            | DetectionAlgorithm::ComplexFlux
            | DetectionAlgorithm::SpectralSparsity => {
                SuperFluxDetector.detect(audio, sample_rate, config)
            }
            DetectionAlgorithm::EnergyTransient => {
                EnergyTransientDetector.detect(audio, sample_rate, config)
            }
            DetectionAlgorithm::ManualGrid => ManualGridDetector.detect(audio, sample_rate, config),
            DetectionAlgorithm::PitchStability => Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct HybridOnsetDetector;

impl HybridOnsetDetector {
    pub fn detect_with_pitch_track(
        &self,
        audio: &[f32],
        sample_rate: u32,
        config: DetectionConfig,
        pitch_track: PitchTrack<'_>,
    ) -> Vec<SliceMarker> {
        let mut markers = SuperFluxDetector.detect(
            audio,
            sample_rate,
            DetectionConfig {
                algorithm: DetectionAlgorithm::SuperFlux,
                params: AlgorithmParams::SuperFlux {
                    lookback_frames: 3,
                    max_filter_radius: 3,
                },
                ..config
            },
        );
        markers.extend(pitch_stability_markers_from_track(
            pitch_track,
            pitch_threshold_cents(config),
            stable_duration_ms(config),
            config.min_slice_ms,
        ));
        dedupe_markers(
            markers,
            ms_to_samples(config.min_slice_ms, sample_rate),
            audio.len(),
        )
    }
}

impl OnsetDetector for HybridOnsetDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker> {
        SuperFluxDetector.detect(audio, sample_rate, config)
    }
}

#[derive(Debug, Default)]
pub struct ManualGridDetector;

impl OnsetDetector for ManualGridDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker> {
        let AlgorithmParams::ManualGrid {
            divisions,
            offset_ms,
        } = config.params
        else {
            return Vec::new();
        };

        if divisions == 0 || audio.is_empty() {
            return Vec::new();
        }

        let offset_samples = ms_to_samples(offset_ms, sample_rate);
        let step = audio.len() / divisions.max(1);
        (0..divisions)
            .map(|index| SliceMarker {
                position_samples: (offset_samples + index * step).min(audio.len() - 1),
                kind: MarkerKind::Auto,
            })
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct EnergyTransientDetector;

impl OnsetDetector for EnergyTransientDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker> {
        if audio.is_empty() || sample_rate == 0 {
            return Vec::new();
        }

        let frame_size = match config.params {
            AlgorithmParams::EnergyTransient { frame_size } => frame_size.max(32),
            _ => 512,
        };
        let min_gap = ms_to_samples(config.min_slice_ms, sample_rate);
        let threshold = 0.02 + (1.0 - config.sensitivity.clamp(0.0, 1.0)) * 0.18;
        let mut last_energy = 0.0;
        let mut last_marker = 0usize.saturating_sub(min_gap);
        let mut markers = vec![SliceMarker {
            position_samples: 0,
            kind: MarkerKind::Auto,
        }];

        for (frame_index, frame) in audio.chunks(frame_size).enumerate() {
            let energy = analysis::rms(frame).powi(2);
            let delta = energy - last_energy;
            let position = frame_index * frame_size;

            if delta > threshold && position.saturating_sub(last_marker) >= min_gap {
                markers.push(SliceMarker {
                    position_samples: position,
                    kind: MarkerKind::Auto,
                });
                last_marker = position;
            }

            last_energy = energy;
        }

        markers
    }
}

#[derive(Debug, Default)]
pub struct SuperFluxDetector;

impl OnsetDetector for SuperFluxDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32, config: DetectionConfig) -> Vec<SliceMarker> {
        if audio.is_empty() || sample_rate == 0 {
            return Vec::new();
        }

        let max_filter_radius = match config.params {
            AlgorithmParams::SuperFlux {
                max_filter_radius, ..
            } => max_filter_radius as usize,
            _ => 3,
        };
        let frame_size = 1024usize;
        let hop_size = 256usize;
        let min_gap = ms_to_samples(config.min_slice_ms, sample_rate);
        let flux = spectral_flux(audio, frame_size, hop_size, max_filter_radius);
        let positions = pick_flux_peaks(&flux, hop_size, config.sensitivity, min_gap);
        dedupe_markers(
            positions
                .into_iter()
                .map(|position_samples| SliceMarker {
                    position_samples,
                    kind: MarkerKind::Auto,
                })
                .collect(),
            min_gap,
            audio.len(),
        )
    }
}

#[derive(Debug, Default)]
pub struct PitchStabilityDetector;

impl PitchStabilityDetector {
    pub fn detect_from_pitch_track(
        &self,
        pitch_track: PitchTrack<'_>,
        config: DetectionConfig,
    ) -> Vec<SliceMarker> {
        pitch_stability_markers_from_track(
            pitch_track,
            pitch_threshold_cents(config),
            stable_duration_ms(config),
            config.min_slice_ms,
        )
    }
}

impl OnsetDetector for PitchStabilityDetector {
    fn detect(
        &self,
        _audio: &[f32],
        _sample_rate: u32,
        _config: DetectionConfig,
    ) -> Vec<SliceMarker> {
        Vec::new()
    }
}

pub fn pitch_stability_markers_from_track(
    pitch_track: PitchTrack<'_>,
    threshold_cents: f32,
    min_stable_duration_ms: f32,
    min_note_ms: f32,
) -> Vec<SliceMarker> {
    if pitch_track.frames.is_empty() {
        return Vec::new();
    }

    let frame_ms = pitch_track.frame_hop_samples as f32
        / pitch_track.source_sample_rate.max(1) as f32
        * 1000.0;
    let stable_frames = (min_stable_duration_ms.max(frame_ms) / frame_ms).round() as usize;
    let min_gap = ms_to_samples(min_note_ms, pitch_track.source_sample_rate);
    let mut markers = vec![SliceMarker {
        position_samples: 0,
        kind: MarkerKind::Auto,
    }];
    let threshold_cents = threshold_cents.max(1.0);

    for index in stable_frames..pitch_track.frames.len().saturating_sub(stable_frames) {
        let left = median_voiced_pitch(&pitch_track.frames[index - stable_frames..index]);
        let right = median_voiced_pitch(&pitch_track.frames[index..index + stable_frames]);
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        let cents = 1200.0 * (right / left).log2().abs();
        let position = pitch_track.frames[index].source_sample_position;
        let far_enough = markers
            .last()
            .map(|last| position.saturating_sub(last.position_samples) >= min_gap)
            .unwrap_or(true);
        if cents >= threshold_cents && far_enough {
            markers.push(SliceMarker {
                position_samples: position,
                kind: MarkerKind::Auto,
            });
        }
    }

    markers
}

fn spectral_flux(
    audio: &[f32],
    frame_size: usize,
    hop_size: usize,
    max_filter_radius: usize,
) -> Vec<f32> {
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let mut previous = vec![0.0; frame_size / 2 + 1];
    let mut flux = Vec::new();
    let mut start = 0;

    while start < audio.len() {
        let mut frame = fft.make_input_vec();
        for (index, sample) in frame.iter_mut().enumerate() {
            let source = audio.get(start + index).copied().unwrap_or(0.0);
            *sample = sanitize_sample(source) * hann(index, frame_size);
        }
        let mut spectrum = fft.make_output_vec();
        if fft.process(&mut frame, &mut spectrum).is_err() {
            break;
        }

        let magnitudes = spectrum
            .iter()
            .map(|bin| bin.norm_sqr().sqrt())
            .collect::<Vec<_>>();
        let mut sum = 0.0;
        for (bin, magnitude) in magnitudes.iter().copied().enumerate() {
            let previous_max = local_max(&previous, bin, max_filter_radius);
            sum += (magnitude - previous_max).max(0.0);
        }
        flux.push(sum);
        previous = magnitudes;
        start += hop_size;
    }

    normalize_flux(&mut flux);
    flux
}

fn pick_flux_peaks(
    flux: &[f32],
    hop_size: usize,
    sensitivity: f32,
    min_gap_samples: usize,
) -> Vec<usize> {
    if flux.is_empty() {
        return Vec::new();
    }

    let mean = flux.iter().copied().sum::<f32>() / flux.len() as f32;
    let variance = flux
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f32>()
        / flux.len() as f32;
    let std_dev = variance.sqrt();
    let threshold = (mean + std_dev * (1.5 - sensitivity.clamp(0.0, 1.0))).max(0.05);
    let mut peaks = vec![0];
    let mut last_peak = 0usize;

    for index in 1..flux.len().saturating_sub(1) {
        let position = index * hop_size;
        if flux[index] >= threshold
            && flux[index] >= flux[index - 1]
            && flux[index] >= flux[index + 1]
            && position.saturating_sub(last_peak) >= min_gap_samples
        {
            peaks.push(position);
            last_peak = position;
        }
    }

    peaks
}

fn dedupe_markers(
    mut markers: Vec<SliceMarker>,
    min_gap_samples: usize,
    audio_len: usize,
) -> Vec<SliceMarker> {
    if audio_len == 0 {
        return Vec::new();
    }

    markers.sort_by_key(|marker| marker.position_samples);
    let mut deduped: Vec<SliceMarker> = Vec::new();
    for mut marker in markers {
        marker.position_samples = marker.position_samples.min(audio_len - 1);
        let far_enough = deduped
            .last()
            .map(|last| {
                marker
                    .position_samples
                    .saturating_sub(last.position_samples)
                    >= min_gap_samples
            })
            .unwrap_or(true);
        if far_enough {
            deduped.push(marker);
        }
    }
    if deduped.first().map(|marker| marker.position_samples) != Some(0) {
        deduped.insert(
            0,
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
        );
    }
    deduped
}

fn normalize_flux(flux: &mut [f32]) {
    let max = flux.iter().copied().fold(0.0, f32::max);
    if max <= f32::EPSILON {
        return;
    }
    for value in flux {
        *value = (*value / max).clamp(0.0, 1.0);
    }
}

fn local_max(values: &[f32], center: usize, radius: usize) -> f32 {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(values.len());
    values[start..end].iter().copied().fold(0.0, f32::max)
}

fn median_voiced_pitch(frames: &[PitchTrackFrame]) -> Option<f32> {
    let mut pitches = frames
        .iter()
        .filter_map(|frame| frame.f0_hz)
        .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
        .collect::<Vec<_>>();
    if pitches.is_empty() {
        return None;
    }
    pitches.sort_by(f32::total_cmp);
    Some(pitches[pitches.len() / 2])
}

fn pitch_threshold_cents(config: DetectionConfig) -> f32 {
    match config.params {
        AlgorithmParams::PitchStability {
            threshold_cents, ..
        } => threshold_cents,
        _ => 120.0,
    }
}

fn stable_duration_ms(config: DetectionConfig) -> f32 {
    match config.params {
        AlgorithmParams::PitchStability {
            min_stable_duration_ms,
            ..
        } => min_stable_duration_ms,
        _ => 64.0,
    }
}

fn ms_to_samples(ms: f32, sample_rate: u32) -> usize {
    ((ms.max(0.0) * 0.001) * sample_rate as f32).round() as usize
}

fn sanitize_sample(sample: f32) -> f32 {
    finite_or(sample, 0.0)
}

fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = 2.0 * std::f32::consts::PI * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_grid_places_requested_markers() {
        let detector = ManualGridDetector;
        let config = DetectionConfig {
            algorithm: DetectionAlgorithm::ManualGrid,
            sensitivity: 0.5,
            min_slice_ms: 50.0,
            params: AlgorithmParams::ManualGrid {
                divisions: 4,
                offset_ms: 0.0,
            },
        };

        let markers = detector.detect(&vec![0.0; 400], 48_000, config);
        assert_eq!(markers.len(), 4);
        assert_eq!(markers[2].position_samples, 200);
    }

    #[test]
    fn energy_detector_finds_transient_after_silence() {
        let mut audio = vec![0.0; 4_800];
        audio.extend(vec![0.8; 4_800]);
        let detector = EnergyTransientDetector;
        let config = DetectionConfig {
            algorithm: DetectionAlgorithm::EnergyTransient,
            sensitivity: 0.8,
            min_slice_ms: 50.0,
            params: AlgorithmParams::EnergyTransient { frame_size: 256 },
        };

        let markers = detector.detect(&audio, 48_000, config);

        assert!(
            markers
                .iter()
                .any(|marker| marker.position_samples >= 4_608)
        );
    }

    #[test]
    fn superflux_detects_soft_sine_entry() {
        let mut audio = vec![0.0; 4_800];
        for index in 0..9_600 {
            let ramp = (index as f32 / 2_400.0).min(1.0);
            let phase = 2.0 * std::f32::consts::PI * 440.0 * index as f32 / 48_000.0;
            audio.push(phase.sin() * ramp * 0.7);
        }
        let detector = SuperFluxDetector;
        let config = DetectionConfig {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity: 0.4,
            min_slice_ms: 80.0,
            params: AlgorithmParams::SuperFlux {
                lookback_frames: 3,
                max_filter_radius: 3,
            },
        };

        let markers = detector.detect(&audio, 48_000, config);

        assert!(
            markers
                .iter()
                .any(|marker| marker.position_samples >= 4_000)
        );
    }

    #[test]
    fn pitch_stability_detects_legato_jump_from_supplied_track() {
        let frames = synthetic_pitch_jump_track();
        let track = PitchTrack {
            source_sample_rate: 16_000,
            frame_hop_samples: 256,
            frames: &frames,
        };
        let markers = pitch_stability_markers_from_track(track, 120.0, 48.0, 80.0);

        assert!(
            markers.iter().any(|marker| {
                marker.position_samples > 6_400 && marker.position_samples < 9_600
            })
        );
    }

    fn synthetic_pitch_jump_track() -> Vec<PitchTrackFrame> {
        (0..64)
            .map(|index| PitchTrackFrame {
                source_sample_position: index * 256,
                f0_hz: Some(if index < 32 { 440.0 } else { 660.0 }),
                confidence: 0.95,
            })
            .collect()
    }
}
