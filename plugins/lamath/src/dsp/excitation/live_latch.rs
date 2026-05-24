use lindelion_dsp_utils::db_to_gain;

use super::{
    ExcitationCursor, LIVE_EXCITATION_MAX_GAIN_DB, LIVE_EXCITATION_MIN_GAIN_DB,
    sanitize_live_sample,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveExcitationLatchCapture<'a> {
    pre_roll: &'a LiveExcitationPreRoll,
    block: &'a [f32],
    onset_offset: usize,
    pre_roll_samples: usize,
    window_samples: usize,
    fade_samples: usize,
    gain: f32,
}

impl<'a> LiveExcitationLatchCapture<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pre_roll: &'a LiveExcitationPreRoll,
        block: &'a [f32],
        onset_offset: usize,
        pre_roll_samples: usize,
        window_samples: usize,
        fade_samples: usize,
        gain_db: f32,
    ) -> Self {
        let gain_db = if gain_db.is_finite() {
            gain_db.clamp(LIVE_EXCITATION_MIN_GAIN_DB, LIVE_EXCITATION_MAX_GAIN_DB)
        } else {
            0.0
        };

        Self {
            pre_roll,
            block,
            onset_offset: onset_offset.min(block.len()),
            pre_roll_samples,
            window_samples,
            fade_samples,
            gain: db_to_gain(gain_db),
        }
    }

    pub const fn total_samples(self) -> usize {
        self.pre_roll_samples.saturating_add(self.window_samples)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveExcitationPreRoll {
    samples: Vec<f32>,
    write_index: usize,
    filled: usize,
}

impl LiveExcitationPreRoll {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            write_index: 0,
            filled: 0,
        }
    }

    pub fn reset(&mut self) {
        self.samples.fill(0.0);
        self.write_index = 0;
        self.filled = 0;
    }

    pub fn push_block(&mut self, block: &[f32]) {
        if self.samples.is_empty() {
            return;
        }

        for sample in block {
            self.samples[self.write_index] = sanitize_live_sample(*sample);
            self.write_index = (self.write_index + 1) % self.samples.len();
            self.filled = (self.filled + 1).min(self.samples.len());
        }
    }

    fn copy_recent_scaled_into(&self, target: &mut [f32], gain: f32) {
        target.fill(0.0);
        if target.is_empty() || self.samples.is_empty() || self.filled == 0 {
            return;
        }

        let count = target.len().min(self.filled);
        let target_start = target.len() - count;
        let source_start = (self.write_index + self.samples.len() - count) % self.samples.len();
        for index in 0..count {
            let source_index = (source_start + index) % self.samples.len();
            target[target_start + index] = self.samples[source_index] * gain;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceLiveExcitationLatch {
    buffer: Vec<f32>,
    active_len: usize,
    pending_write: usize,
    fade_samples: usize,
    gain: f32,
    playback: BufferedExcitationPlayback,
}

impl VoiceLiveExcitationLatch {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            active_len: 0,
            pending_write: 0,
            fade_samples: 0,
            gain: 1.0,
            playback: BufferedExcitationPlayback::finished(),
        }
    }

    pub fn trigger(&mut self, capture: LiveExcitationLatchCapture<'_>) {
        self.clear();
        if self.buffer.is_empty() || capture.total_samples() == 0 {
            return;
        }

        self.active_len = capture.total_samples().min(self.buffer.len());
        self.fade_samples = capture.fade_samples.min(self.active_len / 2);
        self.gain = capture.gain;
        self.buffer[..self.active_len].fill(0.0);

        let pre_roll_samples = capture.pre_roll_samples.min(self.active_len);
        self.copy_pre_roll(&capture, pre_roll_samples);
        self.pending_write = pre_roll_samples;
        self.copy_post_onset_block(capture.block, capture.onset_offset);
        self.playback = BufferedExcitationPlayback::new(self.active_len);
    }

    pub fn continue_capture(&mut self, block: &[f32]) {
        if self.pending_write >= self.active_len || block.is_empty() {
            return;
        }
        self.copy_post_onset_block(block, 0);
    }

    pub fn next_sample(&mut self) -> f32 {
        self.playback
            .next_sample(&self.buffer[..self.active_len.min(self.buffer.len())])
    }

    pub fn is_finished(&self) -> bool {
        self.playback.is_finished()
    }

    pub fn clear(&mut self) {
        self.active_len = 0;
        self.pending_write = 0;
        self.fade_samples = 0;
        self.gain = 1.0;
        self.playback = BufferedExcitationPlayback::finished();
    }

    fn copy_pre_roll(&mut self, capture: &LiveExcitationLatchCapture<'_>, pre_roll_samples: usize) {
        if pre_roll_samples == 0 {
            return;
        }

        let current_pre_samples = pre_roll_samples.min(capture.onset_offset);
        let previous_pre_samples = pre_roll_samples - current_pre_samples;
        capture
            .pre_roll
            .copy_recent_scaled_into(&mut self.buffer[..previous_pre_samples], capture.gain);

        let current_start = capture.onset_offset - current_pre_samples;
        for (index, sample) in capture.block[current_start..capture.onset_offset]
            .iter()
            .copied()
            .enumerate()
        {
            let write_index = previous_pre_samples + index;
            self.buffer[write_index] =
                sanitize_live_sample(sample) * capture.gain * self.fade_gain(write_index);
        }

        for index in 0..previous_pre_samples {
            self.buffer[index] *= self.fade_gain(index);
        }
    }

    fn copy_post_onset_block(&mut self, block: &[f32], onset_offset: usize) {
        let start = onset_offset.min(block.len());
        let remaining = self.active_len.saturating_sub(self.pending_write);
        let count = remaining.min(block.len() - start);
        for (index, sample) in block[start..start + count].iter().copied().enumerate() {
            let write_index = self.pending_write + index;
            self.buffer[write_index] =
                sanitize_live_sample(sample) * self.gain * self.fade_gain(write_index);
        }
        self.pending_write += count;
    }

    fn fade_gain(&self, index: usize) -> f32 {
        if self.fade_samples == 0 || self.active_len == 0 {
            return 1.0;
        }

        let fade = self.fade_samples as f32;
        let fade_in = if index < self.fade_samples {
            (index + 1) as f32 / fade
        } else {
            1.0
        };
        let fade_out_start = self.active_len - self.fade_samples;
        let fade_out = if index >= fade_out_start {
            (self.active_len - index) as f32 / fade
        } else {
            1.0
        };
        fade_in.min(fade_out).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BufferedExcitationPlayback {
    cursor: ExcitationCursor,
}

impl BufferedExcitationPlayback {
    const fn finished() -> Self {
        Self {
            cursor: ExcitationCursor::finished(),
        }
    }

    fn new(sample_count: usize) -> Self {
        Self {
            cursor: ExcitationCursor::new(sample_count, 0.0, 1.0, false),
        }
    }

    const fn is_finished(&self) -> bool {
        self.cursor.is_finished()
    }

    fn next_sample(&mut self, samples: &[f32]) -> f32 {
        self.cursor.next_sample(samples)
    }
}
