use lindelion_dsp_utils::math::ms_to_samples;

use crate::{
    AlgorithmParams, DetectionConfig, MarkerKind, OnsetDetectionInput, OnsetDetector, SliceMarker,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct ManualGridDetector;

impl OnsetDetector for ManualGridDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let AlgorithmParams::ManualGrid {
            divisions,
            offset_ms,
        } = config.params
        else {
            return Vec::new();
        };

        let audio = input.audio;
        if divisions == 0 || audio.is_empty() {
            return Vec::new();
        }

        let offset_samples = ms_to_samples(offset_ms, input.sample_rate);
        let step = audio.len() / divisions.max(1);
        (0..divisions)
            .map(|index| SliceMarker {
                position_samples: (offset_samples + index * step).min(audio.len() - 1),
                kind: MarkerKind::Auto,
            })
            .collect()
    }
}
