use lindelion_plugin_shell::{
    ControlEvent, MIDI_CHANNEL_COUNT, MidiEvent, MidiExpressionControl, MidiExpressionControlRoute,
    MidiExpressionMapping, MidiExpressionSource, MidiExpressionUpdate, MidiVoiceExpression,
    NoteEvent, ParameterId,
};
use lindelion_plugin_shell::{ExpressionSource, ExpressionStream};

use crate::{
    AudioInputMode, AudioNoteEvent, ExcitationSlot, LiveExcitationConfig, LiveExcitationMode,
    RESONATOR_BRIGHTNESS_CONTROLLER, RESONATOR_MOD_WHEEL_CONTROLLER,
    RealtimeStreamingAudioAnalysisExpressionSource, RealtimeStreamingAudioAnalysisNoteDetector,
    ResonatorSynthPatch,
    dsp::{
        ExcitationSelector, LiveExcitationBlock, LiveExcitationLatchCapture, LiveExcitationPreRoll,
        MAX_EXCITATION_LAYERS, RuntimeExcitationSlot, SelectedExcitations, SynthEngine,
        VoiceExpression, VoiceTrigger,
    },
    realtime_audio_analysis_expression_source, realtime_audio_analysis_note_detector,
};

const MIDI_EXPRESSION_VOICES: usize = MIDI_CHANNEL_COUNT;
const DEFAULT_REALTIME_BLOCK_CAPACITY: usize = 8_192;
const GLOBAL_EXPRESSION_CHANNEL: u8 = 0;
const AUDIO_NOTE_CHANNEL: u8 = GLOBAL_EXPRESSION_CHANNEL;
const RESONATOR_EXPRESSION_CONTROL_ROUTES: &[MidiExpressionControlRoute] = &[
    MidiExpressionControlRoute::new(
        RESONATOR_MOD_WHEEL_CONTROLLER,
        MidiExpressionControl::ModWheel,
    ),
    MidiExpressionControlRoute::new(
        RESONATOR_BRIGHTNESS_CONTROLLER,
        MidiExpressionControl::Brightness,
    ),
];

const fn resonator_expression_mapping() -> MidiExpressionMapping<'static> {
    MidiExpressionMapping::new(RESONATOR_EXPRESSION_CONTROL_ROUTES)
}

pub const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
pub static BUILTIN_EXCITATION: [f32; 64] = [
    1.0,
    -0.74,
    0.52,
    -0.37,
    0.29,
    -0.21,
    0.16,
    -0.12,
    0.091,
    -0.068,
    0.051,
    -0.038,
    0.029,
    -0.022,
    0.017,
    -0.013,
    0.010,
    -0.0077,
    0.0059,
    -0.0045,
    0.0034,
    -0.0026,
    0.0020,
    -0.0015,
    0.0012,
    -0.0009,
    0.00068,
    -0.00052,
    0.00040,
    -0.00030,
    0.00023,
    -0.00018,
    0.00013,
    -0.00010,
    0.000078,
    -0.000059,
    0.000045,
    -0.000034,
    0.000026,
    -0.000020,
    0.000015,
    -0.000012,
    0.0000088,
    -0.0000067,
    0.0000051,
    -0.0000039,
    0.0000030,
    -0.0000023,
    0.0000017,
    -0.0000013,
    0.0000010,
    -0.00000077,
    0.00000059,
    -0.00000045,
    0.00000034,
    -0.00000026,
    0.00000020,
    -0.00000015,
    0.00000012,
    -0.000000088,
    0.000000067,
    -0.000000051,
    0.000000039,
    0.0,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResonatorRuntimeInput<'a> {
    pub(crate) events: &'a [MidiEvent],
    pub(crate) sidechain: &'a [f32],
}

impl<'a> ResonatorRuntimeInput<'a> {
    pub(crate) const fn new(events: &'a [MidiEvent]) -> Self {
        Self {
            events,
            sidechain: &[],
        }
    }

    pub(crate) const fn with_sidechain(mut self, sidechain: &'a [f32]) -> Self {
        self.sidechain = sidechain;
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimePatch<'a> {
    patch: ResonatorSynthPatch,
    slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
}

impl RuntimePatch<'static> {
    pub(crate) fn with_builtin_excitation(patch: ResonatorSynthPatch) -> Self {
        let slot_config = patch.excitation_slots.first().cloned().unwrap_or_default();
        Self {
            patch,
            slots: [
                Some(runtime_slot_from_config(
                    &slot_config,
                    &BUILTIN_EXCITATION,
                    BUILTIN_EXCITATION_SAMPLE_RATE,
                )),
                None,
                None,
                None,
            ],
        }
    }
}

impl<'a> RuntimePatch<'a> {
    pub(crate) fn new(
        patch: ResonatorSynthPatch,
        slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
    ) -> Self {
        Self { patch, slots }
    }
}

#[derive(Debug)]
pub(crate) struct ResonatorProcessor<'a> {
    runtime_patch: RuntimePatch<'a>,
    engine: SynthEngine<'a>,
    selector: ExcitationSelector,
    expression_source: MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
    audio_expression_source: RealtimeStreamingAudioAnalysisExpressionSource<MIDI_EXPRESSION_VOICES>,
    audio_expression_sample_position: usize,
    audio_note_detector: RealtimeStreamingAudioAnalysisNoteDetector,
    audio_note_state: AudioNoteRuntimeState,
    audio_detector_sample_rate: u32,
    realtime_block_capacity: usize,
    sample_rate: f32,
    live_latch_state: LiveExcitationLatchRuntimeState,
}

impl ResonatorProcessor<'static> {
    pub(crate) fn with_builtin_excitation(sample_rate: f32, patch: ResonatorSynthPatch) -> Self {
        Self::new(sample_rate, RuntimePatch::with_builtin_excitation(patch))
    }

    pub(crate) fn with_builtin_excitation_and_realtime_capacity(
        sample_rate: f32,
        patch: ResonatorSynthPatch,
        realtime_block_capacity: usize,
    ) -> Self {
        Self::new_with_realtime_capacity(
            sample_rate,
            RuntimePatch::with_builtin_excitation(patch),
            realtime_block_capacity,
        )
    }
}

impl<'a> ResonatorProcessor<'a> {
    pub(crate) fn new(sample_rate: f32, runtime_patch: RuntimePatch<'a>) -> Self {
        Self::new_with_realtime_capacity(
            sample_rate,
            runtime_patch,
            DEFAULT_REALTIME_BLOCK_CAPACITY,
        )
    }

    pub(crate) fn new_with_realtime_capacity(
        sample_rate: f32,
        runtime_patch: RuntimePatch<'a>,
        realtime_block_capacity: usize,
    ) -> Self {
        let polyphony = runtime_patch.patch.polyphony.clamp(1, 16) as usize;
        let audio_detector_sample_rate = detector_sample_rate(sample_rate);
        let realtime_block_capacity = realtime_block_capacity.max(1);
        let audio_note_detector = realtime_audio_analysis_note_detector(
            audio_detector_sample_rate,
            realtime_block_capacity,
            runtime_patch.patch.note_detection,
        );
        let audio_expression_source = realtime_audio_analysis_expression_source(
            audio_detector_sample_rate,
            realtime_block_capacity,
            runtime_patch.patch.audio_expression.mapping,
        );
        let live_latch_state =
            LiveExcitationLatchRuntimeState::new(sample_rate, runtime_patch.patch.live_excitation);
        Self {
            runtime_patch,
            engine: SynthEngine::with_live_latch_capacity(
                sample_rate,
                polyphony,
                live_latch_state.capacity_samples(),
            ),
            selector: ExcitationSelector::default(),
            expression_source: MidiExpressionSource::default(),
            audio_expression_source,
            audio_expression_sample_position: 0,
            audio_note_detector,
            audio_note_state: AudioNoteRuntimeState::default(),
            audio_detector_sample_rate,
            realtime_block_capacity,
            sample_rate,
            live_latch_state,
        }
    }

    pub(crate) fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        self.process_with_runtime_input(ResonatorRuntimeInput::new(events), left, right);
    }

    pub(crate) fn process_with_runtime_input(
        &mut self,
        input: ResonatorRuntimeInput<'_>,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        left.fill(0.0);
        right.fill(0.0);

        let policy = self.interaction_policy();
        let live_policy = self.live_excitation_policy();
        if input.sidechain.is_empty() {
            self.live_latch_state.reset_input();
        } else {
            self.engine.continue_live_latch_captures(input.sidechain);
        }

        for event in input.events {
            if policy.should_handle_midi_event(*event) {
                self.handle_event_with_live_latch(*event, input.sidechain, live_policy);
            }
        }

        self.process_audio_note_input(input.sidechain, live_policy);
        self.process_audio_expression_input(input.sidechain);
        let live_excitation = live_policy.continuous_block(input.sidechain);

        self.sync_runtime_expression_source();
        self.engine
            .render_add_with_live_excitation(left, right, live_excitation);
        self.live_latch_state.push_sidechain_block(input.sidechain);
    }

    // Kept as the source-agnostic expression path; the current plugin process uses MIDI expression.
    #[allow(dead_code)]
    pub(crate) fn process_with_expression_source(
        &mut self,
        source: &mut impl ExpressionSource,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        left.fill(0.0);
        right.fill(0.0);

        let policy = self.interaction_policy();
        for event in events {
            if policy.should_handle_midi_event(*event) {
                self.handle_event_with_expression_source(*event, source);
            }
        }

        self.engine.sync_expression_source(source);
        self.engine.render_add(left, right);
    }

    pub(crate) fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub(crate) fn audio_note_status(&self) -> AudioNoteStatus {
        self.audio_note_state.status()
    }

    pub(crate) fn replace_patch_config(&mut self, patch: ResonatorSynthPatch) {
        self.release_active_audio_note();
        let live_latch_state =
            LiveExcitationLatchRuntimeState::new(self.sample_rate, patch.live_excitation);
        let rebuild_engine = live_latch_state.capacity_samples()
            != self.live_latch_state.capacity_samples()
            || patch.polyphony.clamp(1, 16) as usize != self.engine.polyphony();
        self.runtime_patch.patch = patch;
        self.live_latch_state = live_latch_state;
        if rebuild_engine {
            self.engine = SynthEngine::with_live_latch_capacity(
                self.sample_rate,
                self.runtime_patch.patch.polyphony.clamp(1, 16) as usize,
                self.live_latch_state.capacity_samples(),
            );
            self.expression_source = MidiExpressionSource::default();
        }
        self.reset_audio_note_detector();
    }

    pub(crate) fn set_parameter_plain(&mut self, parameter: ParameterId, value: f32) {
        let Some(binding) = crate::parameter_binding(parameter.0) else {
            return;
        };
        let target = binding.runtime_target();
        if !target.is_active() {
            return;
        }

        binding.apply_plain(&mut self.runtime_patch.patch, value);
        match target {
            crate::RuntimeParameterTarget::None | crate::RuntimeParameterTarget::Patch => {}
            crate::RuntimeParameterTarget::Output => self
                .engine
                .set_output_config(self.runtime_patch.patch.output),
            crate::RuntimeParameterTarget::Routing => {
                self.engine.set_routing(self.runtime_patch.patch.routing);
            }
        }
    }

    pub(crate) fn set_pitch_bend_normalized(&mut self, normalized: f32) {
        let range = self.pitch_bend_range();
        self.set_pitch_bend_semitones((normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * range);
    }

    fn handle_event_with_live_latch(
        &mut self,
        event: MidiEvent,
        sidechain: &[f32],
        live_policy: LiveExcitationPolicy,
    ) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => {
                self.note_on_with_latch_capture(channel, note, velocity, sidechain, 0, live_policy);
            }
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity: _,
            })
            | MidiEvent::Note(NoteEvent::Off {
                channel,
                note,
                velocity: _,
            }) => self.note_off(channel, note),
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    fn handle_event_with_expression_source(
        &mut self,
        event: MidiEvent,
        source: &mut impl ExpressionSource,
    ) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => {
                let slot = self.note_on(channel, note, velocity);
                source.voice_started(slot as u32, channel, note, velocity);
            }
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity: _,
            })
            | MidiEvent::Note(NoteEvent::Off {
                channel,
                note,
                velocity: _,
            }) => self.note_off_with_expression_source(channel, note, source),
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    fn process_audio_note_input(&mut self, sidechain: &[f32], live_policy: LiveExcitationPolicy) {
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

    fn process_audio_expression_input(&mut self, sidechain: &[f32]) {
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
    fn handle_audio_note_event(&mut self, event: AudioNoteEvent) {
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
    fn handle_audio_note_event_in_runtime(
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
    fn audio_note_on_in_runtime(
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

    fn audio_note_off_in_runtime(
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

    fn interaction_policy(&self) -> AudioMidiInteractionPolicy {
        AudioMidiInteractionPolicy::new(self.runtime_patch.patch.audio_input.mode)
    }

    fn live_excitation_policy(&self) -> LiveExcitationPolicy {
        LiveExcitationPolicy::new(self.runtime_patch.patch.live_excitation)
    }

    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::PolyPressure {
                channel,
                note,
                value,
            } => self.engine.set_poly_pressure(channel, note, value),
            _ => {
                if let Some(update) = self.expression_source.apply_control_with_mapping(
                    control,
                    self.pitch_bend_range(),
                    resonator_expression_mapping(),
                ) {
                    self.sync_expression_update_to_engine(update);
                }
            }
        }
    }

    fn note_on(&mut self, channel: u8, note: u8, velocity: f32) -> usize {
        let slot = self.start_voice(channel, note, velocity);
        if self.audio_note_state.clear_if_slot(slot).is_some() {
            self.audio_expression_source.voice_released(slot as u32);
        }
        slot
    }

    #[allow(clippy::too_many_arguments)]
    fn note_on_with_latch_capture(
        &mut self,
        channel: u8,
        note: u8,
        velocity: f32,
        sidechain: &[f32],
        onset_offset: usize,
        live_policy: LiveExcitationPolicy,
    ) -> usize {
        let live_latch = live_policy.latch_capture(&self.live_latch_state, sidechain, onset_offset);
        let slot = Self::start_voice_in_runtime(
            &self.runtime_patch,
            &mut self.selector,
            &mut self.engine,
            &mut self.expression_source,
            channel,
            note,
            velocity,
            live_latch,
        );
        if self.audio_note_state.clear_if_slot(slot).is_some() {
            self.audio_expression_source.voice_released(slot as u32);
        }
        slot
    }

    fn start_voice(&mut self, channel: u8, note: u8, velocity: f32) -> usize {
        self.start_voice_with_latch(channel, note, velocity, None)
    }

    fn start_voice_with_latch(
        &mut self,
        channel: u8,
        note: u8,
        velocity: f32,
        live_latch: Option<LiveExcitationLatchCapture<'_>>,
    ) -> usize {
        Self::start_voice_in_runtime(
            &self.runtime_patch,
            &mut self.selector,
            &mut self.engine,
            &mut self.expression_source,
            channel,
            note,
            velocity,
            live_latch,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_voice_in_runtime(
        runtime_patch: &RuntimePatch<'a>,
        selector: &mut ExcitationSelector,
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        channel: u8,
        note: u8,
        velocity: f32,
        live_latch: Option<LiveExcitationLatchCapture<'_>>,
    ) -> usize {
        let selected = selector.select(&runtime_patch.slots, velocity);
        let excitations = if selected.is_empty() {
            SelectedExcitations::from_single(&BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE)
        } else {
            selected
        };
        let modulation = runtime_patch.patch.modulation;
        let mut trigger =
            VoiceTrigger::with_excitations(note, velocity, excitations, &runtime_patch.patch);
        trigger.channel = channel;
        trigger.expression = expression_source.note_expression(channel, velocity).into();
        trigger.modulation = modulation;
        trigger.live_latch = live_latch;
        let slot = engine.note_on(trigger);
        expression_source.begin_voice(slot as u32, channel, velocity);
        slot
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        let channel = sanitize_channel(channel);
        let mut slots = [0_usize; MIDI_EXPRESSION_VOICES];
        let count = self.collect_midi_owned_note_slots(channel, note, &mut slots);
        for slot in slots[..count].iter().copied() {
            self.release_midi_owned_voice(slot);
        }
    }

    fn note_off_with_expression_source(
        &mut self,
        channel: u8,
        note: u8,
        source: &mut impl ExpressionSource,
    ) {
        let channel = sanitize_channel(channel);
        let mut slots = [0_usize; MIDI_EXPRESSION_VOICES];
        let count = self.collect_midi_owned_note_slots(channel, note, &mut slots);
        for slot in slots[..count].iter().copied() {
            self.release_midi_owned_voice(slot);
            source.voice_released(slot as u32);
        }
    }

    fn collect_midi_owned_note_slots(&self, channel: u8, note: u8, target: &mut [usize]) -> usize {
        let mut count = 0;
        for index in 0..self.engine.polyphony() {
            if self.audio_note_state.owns_slot(index) {
                continue;
            }
            if self.engine.slot_channel(index) != Some(channel)
                || self.engine.slot_note(index) != Some(note)
            {
                continue;
            }
            if count < target.len() {
                target[count] = index;
                count += 1;
            }
        }
        count
    }

    fn release_midi_owned_voice(&mut self, slot: usize) {
        self.expression_source.set_voice_gate(slot as u32, false);
        self.engine.note_off_voice(slot);
    }

    fn release_audio_note_voice(&mut self, voice: AudioNoteVoice) {
        Self::release_audio_note_voice_in_runtime(
            &mut self.engine,
            &mut self.expression_source,
            &mut self.audio_expression_source,
            voice,
        );
    }

    fn release_audio_note_voice_in_runtime(
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

    fn release_active_audio_note(&mut self) {
        if let Some(active) = self.audio_note_state.take_active() {
            self.release_audio_note_voice(active);
        }
    }

    fn reset_audio_note_detector(&mut self) {
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

    fn sync_runtime_expression_source(&mut self) {
        let audio_note_state = self.audio_note_state;
        let mut source = RuntimeExpressionSource {
            midi: &mut self.expression_source,
            audio: &mut self.audio_expression_source,
            audio_note_state,
            audio_enabled: self.runtime_patch.patch.audio_expression.enabled,
        };
        self.engine.sync_expression_source(&mut source);
    }

    fn set_pitch_bend_semitones(&mut self, semitones: f32) {
        self.set_channel_pitch_bend(GLOBAL_EXPRESSION_CHANNEL, semitones);
    }

    fn set_channel_pitch_bend(&mut self, channel: u8, semitones: f32) {
        let update =
            self.expression_source
                .set_pitch_bend(channel, semitones, self.pitch_bend_range());
        self.sync_expression_update_to_engine(update);
    }

    fn sync_expression_update_to_engine(&mut self, update: MidiExpressionUpdate) {
        let expression = VoiceExpression::from(update.expression);
        if update.channel == GLOBAL_EXPRESSION_CHANNEL {
            self.engine.set_expression_controls(
                expression.stream.pitch_bend,
                expression.stream.pressure,
                expression.stream.brightness,
                expression.mod_wheel,
            );
        } else {
            self.engine.set_expression_controls_for_channel(
                update.channel,
                expression.stream.pitch_bend,
                expression.stream.pressure,
                expression.stream.brightness,
                expression.mod_wheel,
            );
        }
    }

    fn pitch_bend_range(&self) -> f32 {
        self.runtime_patch
            .patch
            .modulation
            .pitch_bend_range_semitones
            .abs()
            .max(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioMidiInteractionPolicy {
    mode: AudioInputMode,
}

impl AudioMidiInteractionPolicy {
    const fn new(mode: AudioInputMode) -> Self {
        Self { mode }
    }

    fn creates_audio_notes(self) -> bool {
        matches!(
            self.mode,
            AudioInputMode::AudioCreatesNotes | AudioInputMode::MidiPlusAudioCreatesNotes
        )
    }

    fn handles_midi_notes(self) -> bool {
        !matches!(self.mode, AudioInputMode::AudioCreatesNotes)
    }

    fn should_handle_midi_event(self, event: MidiEvent) -> bool {
        !matches!(event, MidiEvent::Note(_)) || self.handles_midi_notes()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LiveExcitationPolicy {
    config: LiveExcitationConfig,
}

impl LiveExcitationPolicy {
    const fn new(config: LiveExcitationConfig) -> Self {
        Self { config }
    }

    fn uses_continuous(self) -> bool {
        matches!(
            self.config.mode,
            LiveExcitationMode::Continuous | LiveExcitationMode::ContinuousAndNoteLatched
        )
    }

    fn continuous_block<'a>(self, sidechain: &'a [f32]) -> LiveExcitationBlock<'a> {
        if !self.uses_continuous() {
            return LiveExcitationBlock::disabled();
        }
        LiveExcitationBlock::from_mono_block(sidechain, self.config.gain_db)
    }

    fn uses_latch(self) -> bool {
        matches!(
            self.config.mode,
            LiveExcitationMode::NoteLatched | LiveExcitationMode::ContinuousAndNoteLatched
        )
    }

    fn latch_capture<'a>(
        self,
        state: &'a LiveExcitationLatchRuntimeState,
        sidechain: &'a [f32],
        onset_offset: usize,
    ) -> Option<LiveExcitationLatchCapture<'a>> {
        if !self.uses_latch() || sidechain.is_empty() || state.capacity_samples() == 0 {
            return None;
        }

        Some(LiveExcitationLatchCapture::new(
            state.pre_roll(),
            sidechain,
            onset_offset,
            state.pre_roll_samples(),
            state.window_samples(),
            state.fade_samples(),
            self.config.gain_db,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct LiveExcitationLatchRuntimeState {
    pre_roll: LiveExcitationPreRoll,
    capacity_samples: usize,
    pre_roll_samples: usize,
    window_samples: usize,
    fade_samples: usize,
}

impl LiveExcitationLatchRuntimeState {
    fn new(sample_rate: f32, config: LiveExcitationConfig) -> Self {
        let sample_rate = sanitized_latch_sample_rate(sample_rate);
        let pre_roll_samples = ms_to_samples(
            finite_clamp(config.latch_pre_roll_ms, 0.0, 500.0, 20.0),
            sample_rate,
        );
        let window_samples = ms_to_samples(
            finite_clamp(config.latch_window_ms, 1.0, 2_000.0, 120.0),
            sample_rate,
        )
        .max(1);
        let fade_samples = ms_to_samples(
            finite_clamp(config.latch_fade_ms, 0.0, 250.0, 5.0),
            sample_rate,
        );
        let capacity_samples = pre_roll_samples.saturating_add(window_samples);

        Self {
            pre_roll: LiveExcitationPreRoll::with_capacity(pre_roll_samples),
            capacity_samples,
            pre_roll_samples,
            window_samples,
            fade_samples,
        }
    }

    const fn capacity_samples(&self) -> usize {
        self.capacity_samples
    }

    const fn pre_roll_samples(&self) -> usize {
        self.pre_roll_samples
    }

    const fn window_samples(&self) -> usize {
        self.window_samples
    }

    const fn fade_samples(&self) -> usize {
        self.fade_samples
    }

    fn pre_roll(&self) -> &LiveExcitationPreRoll {
        &self.pre_roll
    }

    fn push_sidechain_block(&mut self, sidechain: &[f32]) {
        if sidechain.is_empty() {
            return;
        }
        self.pre_roll.push_block(sidechain);
    }

    fn reset_input(&mut self) {
        self.pre_roll.reset();
    }
}

struct RuntimeExpressionSource<'a, A, const VOICES: usize> {
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
struct AudioNoteRuntimeState {
    active: Option<AudioNoteVoice>,
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

    fn clear_if_slot(&mut self, slot: usize) -> Option<AudioNoteVoice> {
        if self.owns_slot(slot) {
            self.active.take()
        } else {
            None
        }
    }

    fn voice_for_slot(&self, slot: usize) -> Option<AudioNoteVoice> {
        self.active.filter(|active| active.slot == slot)
    }

    fn owns_slot(&self, slot: usize) -> bool {
        self.active.is_some_and(|active| active.slot == slot)
    }

    fn status(self) -> AudioNoteStatus {
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
struct AudioNoteVoice {
    slot: usize,
    note: u8,
    pitch_hz: f32,
    velocity: f32,
    confidence: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AudioNoteStatus {
    pub(crate) active: bool,
    pub(crate) pitch_hz: f32,
    pub(crate) confidence: f32,
}

impl From<MidiVoiceExpression> for VoiceExpression {
    fn from(expression: MidiVoiceExpression) -> Self {
        Self {
            stream: expression.stream,
            mod_wheel: expression.mod_wheel,
        }
        .sanitized()
    }
}

fn detector_sample_rate(sample_rate: f32) -> u32 {
    if !sample_rate.is_finite() {
        return BUILTIN_EXCITATION_SAMPLE_RATE as u32;
    }
    sample_rate.round().clamp(1.0, u32::MAX as f32) as u32
}

fn sanitized_latch_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        BUILTIN_EXCITATION_SAMPLE_RATE
    }
}

fn ms_to_samples(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds * 0.001 * sample_rate).round().max(0.0) as usize
}

fn finite_clamp(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

fn sanitize_channel(channel: u8) -> u8 {
    channel.min((MIDI_CHANNEL_COUNT - 1) as u8)
}

pub(crate) fn runtime_slot_from_config<'a>(
    config: &ExcitationSlot,
    samples: &'a [f32],
    sample_rate: f32,
) -> RuntimeExcitationSlot<'a> {
    RuntimeExcitationSlot {
        samples,
        sample_rate,
        gain_db: config.gain_db,
        velocity_low: config.velocity_low,
        velocity_high: config.velocity_high,
        start_offset_samples: config.start_offset_ms.max(0.0) * sample_rate * 0.001,
        velocity_start_offset_samples: config.velocity_start_mod_ms * sample_rate * 0.001,
        looped: config.looping,
        pitch_track: config.pitch_track,
        round_robin_group: config.round_robin_group,
    }
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
