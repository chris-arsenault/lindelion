mod audio_note;
mod builtin;
mod event_handling;
mod interaction;
mod live_excitation;

use audio_note::AudioNoteRuntimeState;
pub(crate) use audio_note::AudioNoteStatus;
use builtin::{BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE};
use interaction::AudioMidiInteractionPolicy;
use live_excitation::{LiveExcitationLatchRuntimeState, LiveExcitationPolicy};

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

    fn interaction_policy(&self) -> AudioMidiInteractionPolicy {
        AudioMidiInteractionPolicy::new(self.runtime_patch.patch.audio_input.mode)
    }

    fn live_excitation_policy(&self) -> LiveExcitationPolicy {
        LiveExcitationPolicy::new(self.runtime_patch.patch.live_excitation)
    }
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
