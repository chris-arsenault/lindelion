use lindelion_dsp_utils::{db_to_gain, interpolation};

pub const MAX_EXCITATION_LAYERS: usize = 4;
const LIVE_EXCITATION_MIN_GAIN_DB: f32 = -60.0;
const LIVE_EXCITATION_MAX_GAIN_DB: f32 = 24.0;
const ROUND_ROBIN_GROUP_COUNT: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct ExcitationLayer<'a> {
    pub samples: &'a [f32],
    pub sample_rate: f32,
    pub gain: f32,
    pub start_offset_samples: f32,
    pub looped: bool,
    pub pitch_track: bool,
}

impl<'a> ExcitationLayer<'a> {
    pub fn new(samples: &'a [f32], sample_rate: f32) -> Self {
        Self {
            samples,
            sample_rate,
            gain: 1.0,
            start_offset_samples: 0.0,
            looped: false,
            pitch_track: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeExcitationSlot<'a> {
    pub samples: &'a [f32],
    pub sample_rate: f32,
    pub gain_db: f32,
    pub velocity_low: u8,
    pub velocity_high: u8,
    pub start_offset_samples: f32,
    pub velocity_start_offset_samples: f32,
    pub looped: bool,
    pub pitch_track: bool,
    pub round_robin_group: Option<u8>,
}

impl<'a> RuntimeExcitationSlot<'a> {
    #[cfg(test)]
    pub fn new(samples: &'a [f32], sample_rate: f32) -> Self {
        Self {
            samples,
            sample_rate,
            gain_db: 0.0,
            velocity_low: 0,
            velocity_high: 127,
            start_offset_samples: 0.0,
            velocity_start_offset_samples: 0.0,
            looped: false,
            pitch_track: false,
            round_robin_group: None,
        }
    }

    fn accepts_velocity(&self, velocity_u8: u8) -> bool {
        self.velocity_low <= velocity_u8 && velocity_u8 <= self.velocity_high
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SelectedExcitations<'a> {
    layers: [Option<ExcitationLayer<'a>>; MAX_EXCITATION_LAYERS],
    layer_count: usize,
}

impl<'a> Default for SelectedExcitations<'a> {
    fn default() -> Self {
        Self {
            layers: [None; MAX_EXCITATION_LAYERS],
            layer_count: 0,
        }
    }
}

impl<'a> SelectedExcitations<'a> {
    pub fn from_single(samples: &'a [f32], sample_rate: f32) -> Self {
        let mut selected = Self::default();
        selected.push(ExcitationLayer::new(samples, sample_rate));
        selected
    }

    pub fn push(&mut self, layer: ExcitationLayer<'a>) -> bool {
        if self.layer_count >= MAX_EXCITATION_LAYERS {
            return false;
        }

        self.layers[self.layer_count] = Some(layer);
        self.layer_count += 1;
        true
    }

    pub const fn layers(&self) -> &[Option<ExcitationLayer<'a>>; MAX_EXCITATION_LAYERS] {
        &self.layers
    }

    #[cfg(test)]
    pub const fn layer_count(&self) -> usize {
        self.layer_count
    }

    pub fn is_empty(&self) -> bool {
        self.layer_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveExcitationBlock<'a> {
    samples: &'a [f32],
    gain: f32,
}

impl<'a> LiveExcitationBlock<'a> {
    pub const fn disabled() -> Self {
        Self {
            samples: &[],
            gain: 0.0,
        }
    }

    pub fn from_mono_block(samples: &'a [f32], gain_db: f32) -> Self {
        if samples.is_empty() {
            return Self::disabled();
        }

        let gain_db = if gain_db.is_finite() {
            gain_db.clamp(LIVE_EXCITATION_MIN_GAIN_DB, LIVE_EXCITATION_MAX_GAIN_DB)
        } else {
            0.0
        };
        Self {
            samples,
            gain: db_to_gain(gain_db),
        }
    }

    pub fn sample_at(self, index: usize) -> f32 {
        self.samples
            .get(index)
            .copied()
            .map(sanitize_live_sample)
            .unwrap_or(0.0)
            * self.gain
    }

    #[cfg(test)]
    pub fn is_enabled(self) -> bool {
        !self.samples.is_empty() && self.gain > 0.0
    }
}

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

#[derive(Debug, Clone)]
pub struct ExcitationSelector {
    round_robin_cursors: [usize; ROUND_ROBIN_GROUP_COUNT],
}

impl Default for ExcitationSelector {
    fn default() -> Self {
        Self {
            round_robin_cursors: [0; ROUND_ROBIN_GROUP_COUNT],
        }
    }
}

impl ExcitationSelector {
    pub fn select<'a>(
        &mut self,
        slots: &[Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
        velocity: f32,
    ) -> SelectedExcitations<'a> {
        let velocity = velocity.clamp(0.0, 1.0);
        let velocity_u8 = (velocity * 127.0).round() as u8;
        let mut selected = SelectedExcitations::default();
        let mut seen_groups = [None; MAX_EXCITATION_LAYERS];
        let mut seen_group_count = 0;

        for slot in slots {
            let Some(slot) = slot else {
                continue;
            };

            if !slot.accepts_velocity(velocity_u8) {
                continue;
            }

            let Some(group) = slot.round_robin_group else {
                selected.push(layer_from_slot(slot, velocity));
                continue;
            };

            if contains_group(&seen_groups, seen_group_count, group) {
                continue;
            }

            seen_groups[seen_group_count] = Some(group);
            seen_group_count += 1;

            let selected_index = self.select_round_robin_slot(slots, velocity_u8, group);
            if let Some(selected_slot) = slots[selected_index].as_ref() {
                selected.push(layer_from_slot(selected_slot, velocity));
            }
        }

        selected
    }

    fn select_round_robin_slot<'a>(
        &mut self,
        slots: &[Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
        velocity_u8: u8,
        group: u8,
    ) -> usize {
        let mut group_indices = [0usize; MAX_EXCITATION_LAYERS];
        let mut group_count = 0;

        for (index, slot) in slots.iter().enumerate() {
            let Some(slot) = slot else {
                continue;
            };

            if slot.round_robin_group == Some(group) && slot.accepts_velocity(velocity_u8) {
                group_indices[group_count] = index;
                group_count += 1;
            }
        }

        let cursor_index = group as usize % ROUND_ROBIN_GROUP_COUNT;
        let selected = group_indices[self.round_robin_cursors[cursor_index] % group_count];
        self.round_robin_cursors[cursor_index] =
            self.round_robin_cursors[cursor_index].wrapping_add(1);
        selected
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VoiceExcitation<'a> {
    layers: [Option<ActiveExcitationLayer<'a>>; MAX_EXCITATION_LAYERS],
}

impl<'a> Default for VoiceExcitation<'a> {
    fn default() -> Self {
        Self {
            layers: [None; MAX_EXCITATION_LAYERS],
        }
    }
}

impl<'a> VoiceExcitation<'a> {
    pub fn trigger(
        &mut self,
        selected: SelectedExcitations<'a>,
        output_sample_rate: f32,
        pitch_ratio: f32,
    ) {
        self.clear();

        for (index, layer) in selected.layers().iter().copied().enumerate() {
            let Some(layer) = layer else {
                continue;
            };
            let layer_pitch_ratio = if layer.pitch_track { pitch_ratio } else { 1.0 };

            self.layers[index] = Some(ActiveExcitationLayer {
                playback: ExcitationPlayback::new(
                    layer.samples,
                    layer.sample_rate,
                    output_sample_rate,
                    layer_pitch_ratio,
                    layer.start_offset_samples,
                    layer.looped,
                ),
                gain: layer.gain,
            });
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        self.layers
            .iter_mut()
            .filter_map(Option::as_mut)
            .map(ActiveExcitationLayer::next_sample)
            .sum()
    }

    pub fn is_finished(&self) -> bool {
        self.layers
            .iter()
            .flatten()
            .all(ActiveExcitationLayer::is_finished)
    }

    pub fn clear(&mut self) {
        self.layers = [None; MAX_EXCITATION_LAYERS];
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveExcitationLayer<'a> {
    playback: ExcitationPlayback<'a>,
    gain: f32,
}

impl ActiveExcitationLayer<'_> {
    fn next_sample(&mut self) -> f32 {
        self.playback.next_sample() * self.gain
    }

    fn is_finished(&self) -> bool {
        self.playback.is_finished()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExcitationPlayback<'a> {
    samples: &'a [f32],
    cursor: ExcitationCursor,
}

impl<'a> ExcitationPlayback<'a> {
    pub fn new(
        samples: &'a [f32],
        source_sample_rate: f32,
        output_sample_rate: f32,
        pitch_ratio: f32,
        start_offset_samples: f32,
        looped: bool,
    ) -> Self {
        let increment = if source_sample_rate > 0.0 && output_sample_rate > 0.0 {
            source_sample_rate / output_sample_rate * pitch_ratio.max(0.0)
        } else {
            1.0
        };

        Self {
            samples,
            cursor: ExcitationCursor::new(samples.len(), start_offset_samples, increment, looped),
        }
    }

    pub const fn is_finished(&self) -> bool {
        self.cursor.is_finished()
    }

    pub fn next_sample(&mut self) -> f32 {
        self.cursor.next_sample(self.samples)
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct ExcitationCursor {
    cursor: f32,
    increment: f32,
    looped: bool,
    finished: bool,
}

impl ExcitationCursor {
    const fn finished() -> Self {
        Self {
            cursor: 0.0,
            increment: 1.0,
            looped: false,
            finished: true,
        }
    }

    fn new(sample_count: usize, start_offset_samples: f32, increment: f32, looped: bool) -> Self {
        Self {
            cursor: start_offset_samples.max(0.0),
            increment,
            looped,
            finished: sample_count == 0,
        }
    }

    const fn is_finished(&self) -> bool {
        self.finished
    }

    fn next_sample(&mut self, samples: &[f32]) -> f32 {
        if self.finished || samples.is_empty() {
            return 0.0;
        }

        let output = interpolation::linear(samples, self.cursor);
        self.cursor += self.increment;

        if self.cursor >= samples.len() as f32 {
            if self.looped {
                self.cursor = self.cursor.rem_euclid(samples.len() as f32);
            } else {
                self.finished = true;
            }
        }

        output
    }
}

fn layer_from_slot<'a>(slot: &RuntimeExcitationSlot<'a>, velocity: f32) -> ExcitationLayer<'a> {
    ExcitationLayer {
        samples: slot.samples,
        sample_rate: slot.sample_rate,
        gain: db_to_gain(slot.gain_db),
        start_offset_samples: slot.start_offset_samples
            + velocity * slot.velocity_start_offset_samples,
        looped: slot.looped,
        pitch_track: slot.pitch_track,
    }
}

fn contains_group(groups: &[Option<u8>; MAX_EXCITATION_LAYERS], count: usize, group: u8) -> bool {
    groups[..count].contains(&Some(group))
}

fn sanitize_live_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;

    #[test]
    fn one_shot_plays_then_finishes() {
        let samples = [1.0, 0.5, -0.5];
        let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 1.0, 0.0, false);

        assert_eq!(playback.next_sample(), 1.0);
        assert_eq!(playback.next_sample(), 0.5);
        assert_eq!(playback.next_sample(), -0.5);
        assert_eq!(playback.next_sample(), 0.0);
        assert!(playback.is_finished());
    }

    #[test]
    fn pitch_ratio_advances_cursor() {
        let samples = [0.0, 1.0, 2.0, 3.0];
        let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 2.0, 0.0, false);

        assert_eq!(playback.next_sample(), 0.0);
        assert_eq!(playback.next_sample(), 2.0);
    }

    #[test]
    fn looped_playback_wraps() {
        let samples = [1.0, 2.0];
        let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 1.0, 0.0, true);

        assert_eq!(playback.next_sample(), 1.0);
        assert_eq!(playback.next_sample(), 2.0);
        assert_eq!(playback.next_sample(), 1.0);
        assert!(!playback.is_finished());
    }

    #[test]
    fn live_excitation_block_sanitizes_bounds_and_scales_input() {
        let samples = [0.5, f32::NAN, 2.0, f32::NEG_INFINITY, -2.0];
        let block = LiveExcitationBlock::from_mono_block(&samples, 6.0);
        let gain = db_to_gain(6.0);

        assert!(block.is_enabled());
        assert_eq!(block.sample_at(0), 0.5 * gain);
        assert_eq!(block.sample_at(1), 0.0);
        assert_eq!(block.sample_at(2), gain);
        assert_eq!(block.sample_at(3), 0.0);
        assert_eq!(block.sample_at(4), -gain);
        assert_eq!(block.sample_at(5), 0.0);
    }

    #[test]
    fn live_excitation_block_does_not_allocate() {
        let samples = [0.25; 64];
        let block = LiveExcitationBlock::from_mono_block(&samples, 0.0);

        assert_no_allocations("live excitation block sample reads", || {
            let mut sum = 0.0;
            for index in 0..samples.len() {
                sum += block.sample_at(index);
            }
            assert!(sum > 0.0);
        });
    }

    #[test]
    fn voice_latch_captures_pre_roll_current_block_and_future_blocks() {
        let mut pre_roll = LiveExcitationPreRoll::with_capacity(3);
        pre_roll.push_block(&[0.1, 0.2, 0.3]);
        let block = [0.4, 0.5, 0.6, 0.7];
        let future = [0.8, 0.9];
        let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 2, 3, 4, 0, 0.0);
        let mut latch = VoiceLiveExcitationLatch::with_capacity(7);

        latch.trigger(capture);
        latch.continue_capture(&future);

        let output = (0..8).map(|_| latch.next_sample()).collect::<Vec<_>>();
        assert_eq!(output, vec![0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.0]);
        assert!(latch.is_finished());
    }

    #[test]
    fn voice_latch_applies_short_fades() {
        let pre_roll = LiveExcitationPreRoll::with_capacity(0);
        let block = [1.0, 1.0, 1.0, 1.0];
        let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 0, 0, 4, 2, 0.0);
        let mut latch = VoiceLiveExcitationLatch::with_capacity(4);

        latch.trigger(capture);

        assert_eq!(latch.next_sample(), 0.5);
        assert_eq!(latch.next_sample(), 1.0);
        assert_eq!(latch.next_sample(), 1.0);
        assert_eq!(latch.next_sample(), 0.5);
        assert_eq!(latch.next_sample(), 0.0);
    }

    #[test]
    fn voice_latch_capture_does_not_allocate() {
        let mut pre_roll = LiveExcitationPreRoll::with_capacity(32);
        pre_roll.push_block(&[0.25; 32]);
        let block = [0.5; 64];
        let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 16, 16, 32, 4, 0.0);
        let mut latch = VoiceLiveExcitationLatch::with_capacity(48);

        assert_no_allocations("voice live latch trigger and playback", || {
            latch.trigger(capture);
            latch.continue_capture(&block);
            let mut sum = 0.0;
            for _ in 0..48 {
                sum += latch.next_sample();
            }
            assert!(sum > 0.0);
        });
    }

    #[test]
    fn selector_layers_ungrouped_velocity_matches() {
        let a = [1.0];
        let b = [2.0];
        let slots = [
            Some(RuntimeExcitationSlot::new(&a, 48_000.0)),
            Some(RuntimeExcitationSlot {
                velocity_low: 64,
                ..RuntimeExcitationSlot::new(&b, 48_000.0)
            }),
            None,
            None,
        ];
        let mut selector = ExcitationSelector::default();

        let soft = selector.select(&slots, 0.25);
        let hard = selector.select(&slots, 0.75);

        assert_eq!(soft.layer_count(), 1);
        assert_eq!(hard.layer_count(), 2);
    }

    #[test]
    fn selector_cycles_round_robin_group() {
        let a = [1.0];
        let b = [2.0];
        let slots = [
            Some(RuntimeExcitationSlot {
                round_robin_group: Some(2),
                ..RuntimeExcitationSlot::new(&a, 48_000.0)
            }),
            Some(RuntimeExcitationSlot {
                round_robin_group: Some(2),
                ..RuntimeExcitationSlot::new(&b, 48_000.0)
            }),
            None,
            None,
        ];
        let mut selector = ExcitationSelector::default();

        let first = selector.select(&slots, 1.0);
        let second = selector.select(&slots, 1.0);
        let third = selector.select(&slots, 1.0);

        assert_eq!(first.layers()[0].unwrap().samples[0], 1.0);
        assert_eq!(second.layers()[0].unwrap().samples[0], 2.0);
        assert_eq!(third.layers()[0].unwrap().samples[0], 1.0);
    }

    #[test]
    fn voice_excitation_sums_layers() {
        let a = [1.0, 0.0];
        let b = [0.5, 0.0];
        let mut selected = SelectedExcitations::default();
        selected.push(ExcitationLayer {
            gain: 2.0,
            ..ExcitationLayer::new(&a, 48_000.0)
        });
        selected.push(ExcitationLayer {
            gain: 4.0,
            ..ExcitationLayer::new(&b, 48_000.0)
        });
        let mut excitation = VoiceExcitation::default();

        excitation.trigger(selected, 48_000.0, 1.0);

        assert_eq!(excitation.next_sample(), 4.0);
        assert_eq!(excitation.next_sample(), 0.0);
        assert!(excitation.is_finished());
    }

    #[test]
    fn selection_and_layer_trigger_do_not_allocate() {
        let a = [1.0, 0.0];
        let b = [0.5, 0.0];
        let slots = [
            Some(RuntimeExcitationSlot::new(&a, 48_000.0)),
            Some(RuntimeExcitationSlot {
                round_robin_group: Some(1),
                ..RuntimeExcitationSlot::new(&b, 48_000.0)
            }),
            None,
            None,
        ];
        let mut selector = ExcitationSelector::default();
        let mut excitation = VoiceExcitation::default();

        assert_no_allocations("excitation selection and trigger", || {
            let selected = selector.select(&slots, 1.0);
            excitation.trigger(selected, 48_000.0, 1.0);
            let _ = excitation.next_sample();
        });
    }
}
