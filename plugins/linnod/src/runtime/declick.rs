use lindelion_dsp_utils::playback::PlaybackRegion;

const DEFAULT_DECLICK_SECONDS: f32 = 0.0015;
const MAX_DECLICK_SAMPLES: f32 = 256.0;

#[derive(Debug, Clone, Copy)]
pub(super) struct PlaybackDeclick {
    region: PlaybackRegion,
    fade_samples: f32,
}

impl PlaybackDeclick {
    pub fn new(region: PlaybackRegion, sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            region,
            fade_samples: (sample_rate * DEFAULT_DECLICK_SECONDS)
                .round()
                .clamp(1.0, MAX_DECLICK_SAMPLES),
        }
    }

    pub fn finished() -> Self {
        Self {
            region: PlaybackRegion::full(0),
            fade_samples: 0.0,
        }
    }

    pub fn gain(self, offset_samples: f32) -> f32 {
        let duration = self.region.duration_samples();
        if duration <= f32::EPSILON || !offset_samples.is_finite() {
            return 0.0;
        }
        let fade_samples = self.fade_samples.min(duration * 0.5);
        if fade_samples <= 1.0 {
            return 1.0;
        }
        let from_start = offset_samples - self.region.start_sample();
        let to_end = self.region.end_sample() - offset_samples;
        smoothstep((from_start / fade_samples).clamp(0.0, 1.0))
            .min(smoothstep((to_end / fade_samples).clamp(0.0, 1.0)))
    }
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}
