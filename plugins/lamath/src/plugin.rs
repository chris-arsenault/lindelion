use lindelion_dsp_utils::params::StructuralChangePolicy;
use lindelion_plugin_shell::{
    AudioInputBuffer, AudioPlugin, ParameterApplyDispatcher, ParameterApplyOutcome, ParameterId,
    ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};
use lindelion_sample_library::{
    LoadedMonoAudioSlots, OwnedMonoAudioBuffer, ReferencedSampleLoadError,
    ReferencedSampleLoadReport, RuntimeMonoAudioBuffer, SampleLibrary, SampleReference,
    load_referenced_mono_audio_from_library, load_referenced_mono_audio_from_paths,
};

use crate::{
    DESCRIPTOR, PARAMETERS,
    dsp::{MAX_EXCITATION_LAYERS, RuntimeExcitationSlot},
    parameters::{ParameterApplyKind, RuntimeParameterTarget, dispatch_parameter_normalized},
    patch::ResonatorSynthPatch,
    patch_io,
    runtime::{
        AudioNoteStatus, ResonatorProcessor, ResonatorRuntimeInput, RuntimePatch,
        runtime_slot_from_config,
    },
};

#[derive(Debug)]
pub struct ResonatorSynth {
    setup: ProcessSetup,
    patch: ResonatorSynthPatch,
    processor: ResonatorProcessor<'static>,
    loaded_buffers: [Option<RuntimeMonoAudioBuffer>; MAX_EXCITATION_LAYERS],
    sidechain_input_scratch: Vec<f32>,
    sidechain_input_len: usize,
    telemetry: ResonatorTelemetry,
}

impl Default for ResonatorSynth {
    fn default() -> Self {
        let patch = ResonatorSynthPatch::default();
        Self {
            setup: ProcessSetup::default(),
            processor: ResonatorProcessor::with_builtin_excitation(48_000.0, patch.clone()),
            patch,
            loaded_buffers: empty_runtime_buffers(),
            sidechain_input_scratch: vec![0.0; ProcessSetup::default().max_block_size],
            sidechain_input_len: 0,
            telemetry: ResonatorTelemetry::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResonatorTelemetry {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_rms: f32,
    pub right_rms: f32,
    pub active_voices: usize,
    pub sidechain: ResonatorSidechainTelemetry,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResonatorSidechainTelemetry {
    pub required: bool,
    pub input_detected: bool,
    pub signal_active: bool,
    pub note_detected: bool,
    pub pitch_confidence: f32,
}

struct ResonatorParameterApplyDispatcher<'a> {
    processor: &'a mut ResonatorProcessor<'static>,
}

impl ParameterApplyDispatcher<ResonatorSynthPatch, ParameterApplyKind, RuntimeParameterTarget>
    for ResonatorParameterApplyDispatcher<'_>
{
    fn handle_parameter_apply(
        &mut self,
        patch: &mut ResonatorSynthPatch,
        outcome: ParameterApplyOutcome<ParameterApplyKind, RuntimeParameterTarget>,
    ) {
        match outcome.apply_kind {
            ParameterApplyKind::Live => {
                if outcome.runtime_target.is_active() {
                    self.processor
                        .set_parameter_plain(outcome.id, outcome.plain);
                }
            }
            ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary) => {
                self.processor.replace_patch_config(patch.clone());
            }
            ParameterApplyKind::Structural(
                StructuralChangePolicy::ResetState
                | StructuralChangePolicy::LiveCrossfade
                | StructuralChangePolicy::LiveMuteRamp,
            ) => {
                if outcome.runtime_target.is_active() {
                    self.processor
                        .set_parameter_plain(outcome.id, outcome.plain);
                } else {
                    self.processor.replace_patch_config(patch.clone());
                }
            }
            ParameterApplyKind::Ignored => {}
        }
    }
}

pub type LoadedExcitationBuffer = OwnedMonoAudioBuffer;
pub type SampleLoadReport = ReferencedSampleLoadReport;
pub type SampleLoadError<E> = ReferencedSampleLoadError<E>;

impl ResonatorSynth {
    pub fn patch(&self) -> &ResonatorSynthPatch {
        &self.patch
    }

    pub fn telemetry(&self) -> ResonatorTelemetry {
        self.telemetry
    }

    pub fn set_patch_with_loaded_excitations(
        &mut self,
        patch: ResonatorSynthPatch,
        buffers: Vec<LoadedExcitationBuffer>,
    ) {
        let mut buffers = buffers.into_iter();
        let runtime_buffers =
            std::array::from_fn(|_| buffers.next().map(RuntimeMonoAudioBuffer::from_owned));
        self.set_patch_with_runtime_buffers(patch, runtime_buffers);
    }

    pub fn set_patch_with_loaded_excitation_slots(
        &mut self,
        patch: ResonatorSynthPatch,
        buffers: [Option<LoadedExcitationBuffer>; MAX_EXCITATION_LAYERS],
    ) {
        let runtime_buffers = buffers.map(|buffer| buffer.map(RuntimeMonoAudioBuffer::from_owned));
        self.set_patch_with_runtime_buffers(patch, runtime_buffers);
    }

    pub fn load_patch_from_sample_library<L>(
        &mut self,
        patch: ResonatorSynthPatch,
        library: &L,
    ) -> Result<SampleLoadReport, SampleLoadError<L::Error>>
    where
        L: SampleLibrary,
    {
        let (buffers, report) = load_excitation_buffers_from_library(&patch, library)?;
        self.set_patch_with_loaded_excitation_slots(patch, buffers);
        Ok(report)
    }

    pub fn load_patch_from_sample_paths(&mut self, patch: ResonatorSynthPatch) -> SampleLoadReport {
        let (buffers, report) = load_excitation_buffers_from_sample_paths(&patch);
        self.set_patch_with_loaded_excitation_slots(patch, buffers);
        report
    }

    fn set_patch_with_runtime_buffers(
        &mut self,
        mut patch: ResonatorSynthPatch,
        runtime_buffers: [Option<RuntimeMonoAudioBuffer>; MAX_EXCITATION_LAYERS],
    ) {
        patch.normalize_routing_for_resonator_models();
        self.patch = patch;
        self.processor = processor_from_patch_and_buffers(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            &runtime_buffers,
            self.setup.max_block_size,
        );
        self.loaded_buffers = runtime_buffers;
    }

    pub fn set_parameter_normalized(&mut self, id: ParameterId, value: f32) {
        let mut dispatcher = ResonatorParameterApplyDispatcher {
            processor: &mut self.processor,
        };
        dispatch_parameter_normalized(&mut self.patch, id.0, value, &mut dispatcher);
    }

    pub fn set_pitch_bend_normalized(&mut self, value: f32) {
        self.processor.set_pitch_bend_normalized(value);
    }

    pub fn reset_audio_engine(&mut self) {
        self.sidechain_input_scratch.fill(0.0);
        self.sidechain_input_len = 0;
        self.telemetry = ResonatorTelemetry::default();
        self.rebuild_processor();
    }

    fn rebuild_processor(&mut self) {
        self.processor = processor_from_patch_and_buffers(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            &self.loaded_buffers,
            self.setup.max_block_size,
        );
    }

    #[cfg(test)]
    pub(crate) fn set_patch_for_test(&mut self, mut patch: ResonatorSynthPatch) {
        patch.normalize_routing_for_resonator_models();
        self.patch = patch;
        self.rebuild_processor();
    }

    #[cfg(test)]
    pub(crate) fn sidechain_input_for_test(&self) -> &[f32] {
        &self.sidechain_input_scratch[..self.sidechain_input_len]
    }
}

type LoadedExcitationSlots = LoadedMonoAudioSlots<MAX_EXCITATION_LAYERS>;

fn processor_from_patch_and_buffers(
    sample_rate: f32,
    patch: ResonatorSynthPatch,
    buffers: &[Option<RuntimeMonoAudioBuffer>; MAX_EXCITATION_LAYERS],
    max_block_size: usize,
) -> ResonatorProcessor<'static> {
    let sample_rate = sanitized_processor_sample_rate(sample_rate);
    if buffers.iter().all(Option::is_none) {
        return ResonatorProcessor::with_builtin_excitation_and_realtime_capacity(
            sample_rate,
            patch,
            max_block_size,
        );
    }

    ResonatorProcessor::new_with_realtime_capacity(
        sample_rate,
        RuntimePatch::new(patch.clone(), loaded_runtime_slots(&patch, buffers)),
        max_block_size,
    )
}

fn sanitized_processor_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}

fn loaded_runtime_slots(
    patch: &ResonatorSynthPatch,
    buffers: &[Option<RuntimeMonoAudioBuffer>; MAX_EXCITATION_LAYERS],
) -> [Option<RuntimeExcitationSlot<'static>>; MAX_EXCITATION_LAYERS] {
    let mut slots = [None; MAX_EXCITATION_LAYERS];
    for (index, buffer) in buffers.iter().enumerate() {
        let Some(buffer) = buffer else {
            continue;
        };
        let config = patch
            .excitation_slots
            .get(index)
            .cloned()
            .unwrap_or_default();
        // The processor field is dropped before loaded_buffers and is rebuilt
        // before loaded_buffers is replaced, so these stable boxed samples
        // outlive every RuntimeExcitationSlot that borrows them.
        let samples = unsafe { buffer.samples_with_static_lifetime() };
        slots[index] = Some(runtime_slot_from_config(
            &config,
            samples,
            buffer.sample_rate(),
        ));
    }
    slots
}

fn empty_runtime_buffers() -> [Option<RuntimeMonoAudioBuffer>; MAX_EXCITATION_LAYERS] {
    std::array::from_fn(|_| None)
}

fn project_sidechain_input(
    input: AudioInputBuffer<'_>,
    block_len: usize,
    scratch: &mut [f32],
) -> usize {
    if input.is_empty() || block_len == 0 || scratch.is_empty() {
        return 0;
    }

    let len = block_len.min(scratch.len());
    let target = &mut scratch[..len];
    let written = input.write_mono_to(target);
    target[written..].fill(0.0);
    len
}

fn load_excitation_buffers_from_library<L>(
    patch: &ResonatorSynthPatch,
    library: &L,
) -> Result<(LoadedExcitationSlots, SampleLoadReport), SampleLoadError<L::Error>>
where
    L: SampleLibrary,
{
    load_referenced_mono_audio_from_library(excitation_sample_references(patch), library)
}

fn load_excitation_buffers_from_sample_paths(
    patch: &ResonatorSynthPatch,
) -> (LoadedExcitationSlots, SampleLoadReport) {
    load_referenced_mono_audio_from_paths(excitation_sample_references(patch))
}

fn excitation_sample_references(
    patch: &ResonatorSynthPatch,
) -> impl Iterator<Item = (usize, &SampleReference)> {
    patch
        .excitation_slots
        .iter()
        .take(MAX_EXCITATION_LAYERS)
        .enumerate()
        .filter_map(|(index, slot)| slot.sample.as_ref().map(|reference| (index, reference)))
}

fn telemetry_from_audio(
    left: &[f32],
    right: &[f32],
    active_voices: usize,
    sidechain: ResonatorSidechainTelemetry,
) -> ResonatorTelemetry {
    let (left_peak, left_rms) = channel_stats(left);
    let (right_peak, right_rms) = channel_stats(right);
    ResonatorTelemetry {
        left_peak,
        right_peak,
        left_rms,
        right_rms,
        active_voices,
        sidechain,
    }
}

fn sidechain_telemetry(
    patch: &ResonatorSynthPatch,
    sidechain: &[f32],
    audio_note: AudioNoteStatus,
) -> ResonatorSidechainTelemetry {
    let (_, sidechain_rms) = channel_stats(sidechain);
    ResonatorSidechainTelemetry {
        required: sidechain_required(patch),
        input_detected: !sidechain.is_empty(),
        signal_active: sidechain_rms > 0.000_01,
        note_detected: audio_note.active,
        pitch_confidence: finite_unit_value(audio_note.confidence),
    }
}

fn sidechain_required(patch: &ResonatorSynthPatch) -> bool {
    patch.audio_input.mode != crate::AudioInputMode::Off
        || patch.live_excitation.mode != crate::LiveExcitationMode::Off
}

fn channel_stats(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }

    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f32;
    for sample in samples {
        let sample = if sample.is_finite() { *sample } else { 0.0 };
        peak = peak.max(sample.abs());
        sum_squares += sample * sample;
    }
    (peak, (sum_squares / samples.len() as f32).sqrt())
}

fn finite_unit_value(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl AudioPlugin for ResonatorSynth {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        self.sidechain_input_scratch
            .resize(setup.max_block_size.max(1), 0.0);
        self.sidechain_input_len = 0;
        self.rebuild_processor();
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        self.sidechain_input_len = project_sidechain_input(
            context.input,
            context.buffer.len(),
            &mut self.sidechain_input_scratch,
        );
        if self.sidechain_input_len == 0 {
            self.processor
                .process(context.events, context.buffer.left, context.buffer.right);
        } else {
            let runtime_input = ResonatorRuntimeInput::new(context.events)
                .with_sidechain(&self.sidechain_input_scratch[..self.sidechain_input_len]);
            self.processor.process_with_runtime_input(
                runtime_input,
                context.buffer.left,
                context.buffer.right,
            );
        }
        self.telemetry = telemetry_from_audio(
            context.buffer.left,
            context.buffer.right,
            self.processor.active_voice_count(),
            sidechain_telemetry(
                &self.patch,
                &self.sidechain_input_scratch[..self.sidechain_input_len],
                self.processor.audio_note_status(),
            ),
        );
    }

    fn state(&self) -> PluginState {
        patch_io::to_plugin_state(&self.patch)
            .unwrap_or_else(|_| PluginState::empty(patch_io::FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(patch) = patch_io::from_plugin_state(state) {
            self.load_patch_from_sample_paths(patch);
        }
    }
}
