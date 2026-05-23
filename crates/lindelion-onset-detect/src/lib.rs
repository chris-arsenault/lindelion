#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionAlgorithm {
    SuperFlux,
    ComplexFlux,
    SpectralSparsity,
    PitchStability,
    EnergyTransient,
    ManualGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceMarker {
    pub position_samples: usize,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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
            let energy =
                frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len().max(1) as f32;
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

        markers.truncate(16);
        markers
    }
}

fn ms_to_samples(ms: f32, sample_rate: u32) -> usize {
    ((ms.max(0.0) * 0.001) * sample_rate as f32).round() as usize
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
}
