use ahara_dsp_utils::{analysis::peak_abs, math::snap_to_zero};

use super::voice::{Voice, VoiceTrigger};
use crate::OutputConfig;

const IDLE_LEVEL_THRESHOLD: f32 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceSlotState {
    Idle,
    Active,
    Released,
}

#[derive(Debug)]
pub struct SynthEngine<'a> {
    slots: Vec<VoiceSlot<'a>>,
    clock: u64,
}

impl<'a> SynthEngine<'a> {
    pub fn new(sample_rate: f32, polyphony: usize) -> Self {
        let polyphony = polyphony.max(1);
        Self {
            slots: (0..polyphony)
                .map(|_| VoiceSlot::new(sample_rate))
                .collect(),
            clock: 0,
        }
    }

    pub fn polyphony(&self) -> usize {
        self.slots.len()
    }

    pub fn active_voice_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.state != VoiceSlotState::Idle)
            .count()
    }

    pub fn slot_state(&self, index: usize) -> Option<VoiceSlotState> {
        self.slots.get(index).map(|slot| slot.state)
    }

    pub fn slot_note(&self, index: usize) -> Option<u8> {
        self.slots.get(index).and_then(|slot| slot.note)
    }

    pub fn slot_last_level(&self, index: usize) -> Option<f32> {
        self.slots.get(index).map(|slot| slot.last_level)
    }

    pub fn note_on(&mut self, trigger: VoiceTrigger<'a, '_>) -> usize {
        self.clock = self.clock.wrapping_add(1);
        let slot_index = self.choose_voice_slot();
        let slot = &mut self.slots[slot_index];

        slot.voice.trigger(trigger);
        slot.note = Some(trigger.midi_note);
        slot.state = VoiceSlotState::Active;
        slot.started_at = self.clock;
        slot.released_at = None;
        slot.last_level = trigger.velocity.clamp(0.0, 1.0);

        slot_index
    }

    pub fn note_off(&mut self, note: u8) {
        self.clock = self.clock.wrapping_add(1);

        for slot in &mut self.slots {
            if slot.state == VoiceSlotState::Active && slot.note == Some(note) {
                slot.state = VoiceSlotState::Released;
                slot.released_at = Some(self.clock);
                slot.voice.note_off();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.clock = self.clock.wrapping_add(1);

        for slot in &mut self.slots {
            if slot.state == VoiceSlotState::Active {
                slot.state = VoiceSlotState::Released;
                slot.released_at = Some(self.clock);
                slot.voice.note_off();
            }
        }
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        for slot in &mut self.slots {
            if slot.state != VoiceSlotState::Idle {
                slot.voice.set_pitch_bend(semitones);
            }
        }
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        for slot in &mut self.slots {
            if slot.state != VoiceSlotState::Idle {
                slot.voice.set_output_config(output);
            }
        }
    }

    pub fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        for slot in &mut self.slots {
            if slot.state != VoiceSlotState::Idle {
                slot.voice.set_waveguide_loop_gain(loop_gain);
            }
        }
    }

    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        for slot in &mut self.slots {
            if slot.state == VoiceSlotState::Idle {
                continue;
            }

            let mut block_peak = 0.0_f32;
            for index in 0..len {
                let (sample_left, sample_right) = slot.voice.process_stereo_sample();
                block_peak = block_peak.max(sample_left.abs()).max(sample_right.abs());
                left[index] = snap_to_zero(left[index] + sample_left);
                right[index] = snap_to_zero(right[index] + sample_right);
            }

            slot.last_level = block_peak;
            if slot.state == VoiceSlotState::Released
                && slot.voice.is_excitation_finished()
                && block_peak < IDLE_LEVEL_THRESHOLD
            {
                slot.clear();
            }
        }
    }

    pub fn render_replace(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.render_add(left, right);
    }

    fn choose_voice_slot(&self) -> usize {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.state == VoiceSlotState::Idle)
        {
            return index;
        }

        if let Some((index, _)) = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.state == VoiceSlotState::Released)
            .min_by(|(_, a), (_, b)| {
                a.released_at
                    .cmp(&b.released_at)
                    .then_with(|| a.last_level.total_cmp(&b.last_level))
            })
        {
            return index;
        }

        self.slots
            .iter()
            .enumerate()
            .min_by_key(|(_, slot)| slot.started_at)
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}

#[derive(Debug)]
struct VoiceSlot<'a> {
    voice: Voice<'a>,
    state: VoiceSlotState,
    note: Option<u8>,
    started_at: u64,
    released_at: Option<u64>,
    last_level: f32,
}

impl<'a> VoiceSlot<'a> {
    fn new(sample_rate: f32) -> Self {
        Self {
            voice: Voice::new(sample_rate),
            state: VoiceSlotState::Idle,
            note: None,
            started_at: 0,
            released_at: None,
            last_level: 0.0,
        }
    }

    fn clear(&mut self) {
        self.voice.clear();
        self.state = VoiceSlotState::Idle;
        self.note = None;
        self.released_at = None;
        self.last_level = 0.0;
    }
}

pub fn stereo_peak(left: &[f32], right: &[f32]) -> f32 {
    peak_abs(left).max(peak_abs(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModalConfig, ModalPreset, OutputConfig, ResonatorConfig, ResonatorRouting,
        ResonatorSynthPatch, WaveguideConfig, assert_no_allocations,
    };
    use ahara_dsp_utils::analysis::{assert_all_finite, rms};

    #[test]
    fn note_on_uses_free_slots_before_stealing() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 3);

        assert_eq!(
            engine.note_on(trigger(60, &excitation, sample_rate, &patch)),
            0
        );
        assert_eq!(
            engine.note_on(trigger(64, &excitation, sample_rate, &patch)),
            1
        );
        assert_eq!(
            engine.note_on(trigger(67, &excitation, sample_rate, &patch)),
            2
        );

        assert_eq!(engine.active_voice_count(), 3);
        assert_eq!(engine.slot_note(0), Some(60));
        assert_eq!(engine.slot_note(1), Some(64));
        assert_eq!(engine.slot_note(2), Some(67));
    }

    #[test]
    fn released_voice_is_stolen_before_active_voice() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 2);

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_on(trigger(64, &excitation, sample_rate, &patch));
        engine.note_off(60);
        let stolen = engine.note_on(trigger(67, &excitation, sample_rate, &patch));

        assert_eq!(stolen, 0);
        assert_eq!(engine.slot_note(0), Some(67));
        assert_eq!(engine.slot_note(1), Some(64));
        assert_eq!(engine.slot_state(0), Some(VoiceSlotState::Active));
    }

    #[test]
    fn oldest_active_voice_is_stolen_when_pool_is_full() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 2);

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_on(trigger(64, &excitation, sample_rate, &patch));
        let stolen = engine.note_on(trigger(67, &excitation, sample_rate, &patch));

        assert_eq!(stolen, 0);
        assert_eq!(engine.slot_note(0), Some(67));
        assert_eq!(engine.slot_note(1), Some(64));
    }

    #[test]
    fn render_replace_outputs_finite_polyphonic_audio() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 4);
        let mut left = vec![0.0; 8_192];
        let mut right = vec![0.0; 8_192];

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_on(trigger(64, &excitation, sample_rate, &patch));
        engine.note_on(trigger(67, &excitation, sample_rate, &patch));
        engine.render_replace(&mut left, &mut right);

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(rms(&left) > 0.000_1);
        assert!(rms(&right) > 0.000_1);
        assert!(stereo_peak(&left, &right) < 4.0);
        assert!(engine.slot_last_level(0).unwrap() > 0.0);
    }

    #[test]
    fn released_quiet_voice_eventually_becomes_idle() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.1,
            ..WaveguideConfig::default()
        });
        patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.0,
            ..WaveguideConfig::default()
        });
        patch.routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 1);
        let mut left = vec![0.0; 16_384];
        let mut right = vec![0.0; 16_384];

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_off(60);
        engine.render_replace(&mut left, &mut right);
        engine.render_replace(&mut left, &mut right);

        assert_eq!(engine.slot_state(0), Some(VoiceSlotState::Idle));
    }

    #[test]
    fn note_on_and_render_do_not_allocate() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.resonator_a = ResonatorConfig::Modal(ModalConfig {
            mode_count: 256,
            preset: ModalPreset::Bell,
            ..ModalConfig::default()
        });
        patch.resonator_b = ResonatorConfig::Modal(ModalConfig {
            mode_count: 256,
            preset: ModalPreset::GlassBowl,
            ..ModalConfig::default()
        });
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 8);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        assert_no_allocations("note_on", || {
            engine.note_on(trigger(60, &excitation, sample_rate, &patch));
            engine.note_on(trigger(64, &excitation, sample_rate, &patch));
            engine.note_on(trigger(67, &excitation, sample_rate, &patch));
        });

        assert_no_allocations("render_replace", || {
            engine.render_replace(&mut left, &mut right);
        });

        assert_no_allocations("voice_stealing_note_on", || {
            for note in 68..80 {
                engine.note_on(trigger(note, &excitation, sample_rate, &patch));
            }
        });
    }

    fn trigger<'a>(
        note: u8,
        excitation: &'a [f32],
        sample_rate: f32,
        patch: &'a ResonatorSynthPatch,
    ) -> VoiceTrigger<'a, 'a> {
        VoiceTrigger::new(note, 1.0, excitation, sample_rate, patch)
    }

    fn impulse() -> Vec<f32> {
        let mut excitation = vec![0.0; 64];
        excitation[0] = 1.0;
        excitation
    }

    fn test_patch() -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            resonator_a: ResonatorConfig::Modal(ModalConfig {
                mode_count: 16,
                preset: ModalPreset::GenericStrike,
                decay_global: 0.4,
                ..ModalConfig::default()
            }),
            resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.9,
                ..WaveguideConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 0.8,
                mix_b: 0.2,
            },
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                master_gain_db: -6.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }
}
