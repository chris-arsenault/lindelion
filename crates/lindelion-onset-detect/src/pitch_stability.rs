use lindelion_dsp_utils::math::{cents_between, ms_to_samples};
use lindelion_pitch_detect::median_voiced_pitch;

use crate::{
    DetectionConfig, MarkerKind, OnsetDetectionInput, OnsetDetector, PitchTrack, SliceMarker,
    onset_profile,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct PitchStabilityDetector;

impl OnsetDetector for PitchStabilityDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let Some(pitch_track) = input.pitch_track else {
            return Vec::new();
        };
        let profile = onset_profile(config);
        pitch_stability_markers_from_track(
            pitch_track,
            profile.pitch_stability_threshold_cents,
            profile.pitch_stability_duration_ms,
            config.min_slice_ms,
        )
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
        let cents = cents_between(left, right);
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
