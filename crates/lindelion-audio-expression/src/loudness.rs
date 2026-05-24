use lindelion_dsp_utils::analysis::{rms, spectral_centroid_hz};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StreamingLoudnessFrame {
    pub start_sample: usize,
    pub end_sample: usize,
    pub rms: f32,
    pub spectral_centroid_hz: f32,
}

pub trait StreamingLoudnessTracker {
    fn next_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        sample_rate: u32,
    ) -> StreamingLoudnessFrame;

    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RmsCentroidLoudnessTracker {
    frame: StreamingLoudnessFrame,
}

impl RmsCentroidLoudnessTracker {
    pub const fn current_frame(&self) -> StreamingLoudnessFrame {
        self.frame
    }
}

impl StreamingLoudnessTracker for RmsCentroidLoudnessTracker {
    fn next_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        sample_rate: u32,
    ) -> StreamingLoudnessFrame {
        self.frame = StreamingLoudnessFrame {
            start_sample,
            end_sample: start_sample.saturating_add(audio.len()),
            rms: rms(audio),
            spectral_centroid_hz: spectral_centroid_hz(audio, sample_rate.max(1) as f32)
                .unwrap_or(0.0),
        };
        self.frame
    }

    fn reset(&mut self) {
        self.frame = StreamingLoudnessFrame::default();
    }
}
