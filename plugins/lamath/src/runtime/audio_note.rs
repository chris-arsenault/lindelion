use super::*;

impl<'a> ResonatorProcessor<'a> {
    pub(super) fn process_audio_note_input(
        &mut self,
        sidechain: &[f32],
        live_policy: LiveExcitationPolicy,
    ) {
        if !self.interaction_policy().creates_audio_notes() {
            return;
        }

        if sidechain.is_empty() {
            self.release_active_audio_note();
            self.audio_note_detector.reset();
            return;
        }

        let events = self
            .audio_note_detector
            .next_block(sidechain, self.runtime_patch.patch.note_detection);
        for event in events.iter().copied() {
            Self::handle_audio_note_event_in_runtime(
                event,
                &self.runtime_patch,
                &mut self.selector,
                &mut self.engine,
                &mut self.expression_source,
                &mut self.audio_expression_source,
                &mut self.audio_note_state,
                &self.live_latch_state,
                sidechain,
                live_policy,
            );
        }
    }

    pub(super) fn process_audio_expression_input(&mut self, sidechain: &[f32]) {
        if sidechain.is_empty() {
            if self.audio_expression_sample_position != 0 {
                self.audio_expression_source.frame_source_mut().reset();
                self.audio_expression_sample_position = 0;
            }
            return;
        }

        if !self.runtime_patch.patch.audio_expression.enabled {
            return;
        }
        if self.audio_note_state.active.is_none() {
            return;
        }

        self.audio_expression_source
            .set_mapping(self.runtime_patch.patch.audio_expression.mapping);
        self.audio_expression_source
            .set_audio_block(self.audio_expression_sample_position, sidechain);
        self.audio_expression_sample_position = self
            .audio_expression_sample_position
            .saturating_add(sidechain.len());
    }

    #[cfg(test)]
    pub(super) fn handle_audio_note_event(&mut self, event: AudioNoteEvent) {
        let live_policy = self.live_excitation_policy();
        Self::handle_audio_note_event_in_runtime(
            event,
            &self.runtime_patch,
            &mut self.selector,
            &mut self.engine,
            &mut self.expression_source,
            &mut self.audio_expression_source,
            &mut self.audio_note_state,
            &self.live_latch_state,
            &[],
            live_policy,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_audio_note_event_in_runtime(
        event: AudioNoteEvent,
        runtime_patch: &RuntimePatch<'a>,
        selector: &mut ExcitationSelector,
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        audio_expression_source: &mut RealtimeStreamingAudioAnalysisExpressionSource<
            MIDI_EXPRESSION_VOICES,
        >,
        audio_note_state: &mut AudioNoteRuntimeState,
        live_latch_state: &LiveExcitationLatchRuntimeState,
        sidechain: &[f32],
        live_policy: LiveExcitationPolicy,
    ) {
        if event.gate {
            Self::audio_note_on_in_runtime(
                event,
                runtime_patch,
                selector,
                engine,
                expression_source,
                audio_expression_source,
                audio_note_state,
                live_latch_state,
                sidechain,
                live_policy,
            );
        } else {
            Self::audio_note_off_in_runtime(
                event,
                engine,
                expression_source,
                audio_expression_source,
                audio_note_state,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn audio_note_on_in_runtime(
        event: AudioNoteEvent,
        runtime_patch: &RuntimePatch<'a>,
        selector: &mut ExcitationSelector,
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        audio_expression_source: &mut RealtimeStreamingAudioAnalysisExpressionSource<
            MIDI_EXPRESSION_VOICES,
        >,
        audio_note_state: &mut AudioNoteRuntimeState,
        live_latch_state: &LiveExcitationLatchRuntimeState,
        sidechain: &[f32],
        live_policy: LiveExcitationPolicy,
    ) {
        if let Some(active) = audio_note_state.take_active() {
            Self::release_audio_note_voice_in_runtime(
                engine,
                expression_source,
                audio_expression_source,
                active,
            );
        }

        let live_latch = live_policy.latch_capture(live_latch_state, sidechain, event.offset);
        let slot = Self::start_voice_in_runtime(
            runtime_patch,
            selector,
            engine,
            expression_source,
            AUDIO_NOTE_CHANNEL,
            event.note,
            event.velocity,
            live_latch,
        );
        audio_note_state.start(slot, event);
        audio_expression_source.voice_started(
            slot as u32,
            AUDIO_NOTE_CHANNEL,
            event.note,
            event.velocity,
        );
    }

    pub(super) fn audio_note_off_in_runtime(
        event: AudioNoteEvent,
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        audio_expression_source: &mut RealtimeStreamingAudioAnalysisExpressionSource<
            MIDI_EXPRESSION_VOICES,
        >,
        audio_note_state: &mut AudioNoteRuntimeState,
    ) {
        let Some(active) = audio_note_state.release(event.note) else {
            return;
        };
        Self::release_audio_note_voice_in_runtime(
            engine,
            expression_source,
            audio_expression_source,
            active,
        );
    }

    pub(super) fn release_audio_note_voice(&mut self, voice: AudioNoteVoice) {
        Self::release_audio_note_voice_in_runtime(
            &mut self.engine,
            &mut self.expression_source,
            &mut self.audio_expression_source,
            voice,
        );
    }

    pub(super) fn release_audio_note_voice_in_runtime(
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        audio_expression_source: &mut RealtimeStreamingAudioAnalysisExpressionSource<
            MIDI_EXPRESSION_VOICES,
        >,
        voice: AudioNoteVoice,
    ) {
        if engine.slot_note(voice.slot) != Some(voice.note) {
            return;
        }
        expression_source.set_voice_gate(voice.slot as u32, false);
        audio_expression_source.voice_released(voice.slot as u32);
        engine.note_off_voice(voice.slot);
    }

    pub(super) fn release_active_audio_note(&mut self) {
        if let Some(active) = self.audio_note_state.take_active() {
            self.release_audio_note_voice(active);
        }
    }

    pub(super) fn reset_audio_note_detector(&mut self) {
        self.audio_note_detector = realtime_audio_analysis_note_detector(
            self.audio_detector_sample_rate,
            self.realtime_block_capacity,
            self.runtime_patch.patch.note_detection,
        );
        self.audio_expression_source = realtime_audio_analysis_expression_source(
            self.audio_detector_sample_rate,
            self.realtime_block_capacity,
            self.runtime_patch.patch.audio_expression.mapping,
        );
        self.audio_expression_sample_position = 0;
        self.audio_note_state = AudioNoteRuntimeState::default();
    }

    pub(super) fn sync_runtime_expression_source(&mut self) {
        let audio_note_state = self.audio_note_state;
        let mut source = RuntimeExpressionSource {
            midi: &mut self.expression_source,
            audio: &mut self.audio_expression_source,
            audio_note_state,
            audio_enabled: self.runtime_patch.patch.audio_expression.enabled,
        };
        self.engine.sync_expression_source(&mut source);
    }
}

pub(super) struct RuntimeExpressionSource<'a, A, const VOICES: usize> {
    midi: &'a mut MidiExpressionSource<VOICES>,
    audio: &'a mut A,
    audio_note_state: AudioNoteRuntimeState,
    audio_enabled: bool,
}

impl<A, const VOICES: usize> ExpressionSource for RuntimeExpressionSource<'_, A, VOICES>
where
    A: ExpressionSource,
{
    fn voice_started(&mut self, voice_id: u32, channel: u8, note: u8, velocity: f32) {
        self.midi.voice_started(voice_id, channel, note, velocity);
        self.audio.voice_started(voice_id, channel, note, velocity);
    }

    fn voice_released(&mut self, voice_id: u32) {
        self.midi.voice_released(voice_id);
        self.audio.voice_released(voice_id);
    }

    fn next_block(&mut self, voice_id: u32) -> ExpressionStream {
        let Some(audio_voice) = self.audio_note_state.voice_for_slot(voice_id as usize) else {
            return self.midi.next_block(voice_id);
        };
        if !self.audio_enabled {
            return self.midi.next_block(voice_id);
        }

        let mut stream = self.audio.next_block(voice_id).sanitized();
        stream.velocity = audio_voice.velocity;
        stream.gate = true;
        stream.sanitized()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct AudioNoteRuntimeState {
    pub(super) active: Option<AudioNoteVoice>,
}

impl AudioNoteRuntimeState {
    fn start(&mut self, slot: usize, event: AudioNoteEvent) {
        self.active = Some(AudioNoteVoice {
            slot,
            note: event.note,
            pitch_hz: event.pitch_hz,
            velocity: event.velocity,
            confidence: event.confidence,
        });
    }

    fn release(&mut self, note: u8) -> Option<AudioNoteVoice> {
        if self.active.is_some_and(|active| active.note == note) {
            self.active.take()
        } else {
            None
        }
    }

    fn take_active(&mut self) -> Option<AudioNoteVoice> {
        self.active.take()
    }

    pub(super) fn clear_if_slot(&mut self, slot: usize) -> Option<AudioNoteVoice> {
        if self.owns_slot(slot) {
            self.active.take()
        } else {
            None
        }
    }

    fn voice_for_slot(&self, slot: usize) -> Option<AudioNoteVoice> {
        self.active.filter(|active| active.slot == slot)
    }

    pub(super) fn owns_slot(&self, slot: usize) -> bool {
        self.active.is_some_and(|active| active.slot == slot)
    }

    pub(super) fn status(self) -> AudioNoteStatus {
        let Some(active) = self.active else {
            return AudioNoteStatus::default();
        };

        AudioNoteStatus {
            active: true,
            pitch_hz: active.pitch_hz,
            confidence: active.confidence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AudioNoteVoice {
    pub(super) slot: usize,
    pub(super) note: u8,
    pub(super) pitch_hz: f32,
    pub(super) velocity: f32,
    pub(super) confidence: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AudioNoteStatus {
    pub(crate) active: bool,
    pub(crate) pitch_hz: f32,
    pub(crate) confidence: f32,
}
