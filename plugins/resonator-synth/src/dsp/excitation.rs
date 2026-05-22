use ahara_dsp_utils::{db_to_gain, interpolation};

pub const MAX_EXCITATION_LAYERS: usize = 4;
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

    pub const fn layer_count(&self) -> usize {
        self.layer_count
    }

    pub fn is_empty(&self) -> bool {
        self.layer_count == 0
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
    cursor: f32,
    increment: f32,
    looped: bool,
    finished: bool,
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
            cursor: start_offset_samples.max(0.0),
            increment,
            looped,
            finished: samples.is_empty(),
        }
    }

    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn next_sample(&mut self) -> f32 {
        if self.finished || self.samples.is_empty() {
            return 0.0;
        }

        let output = interpolation::linear(self.samples, self.cursor);
        self.cursor += self.increment;

        if self.cursor >= self.samples.len() as f32 {
            if self.looped {
                self.cursor = self.cursor.rem_euclid(self.samples.len() as f32);
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
