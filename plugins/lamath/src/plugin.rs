use lindelion_dsp_utils::params::StructuralChangePolicy;
use lindelion_plugin_shell::{
    AudioPlugin, ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};
use lindelion_sample_library::{
    SampleDecodeError, SampleLibrary, SampleReference, SampleResolution, decode_wav_mono,
};

use crate::{
    DESCRIPTOR, PARAMETERS,
    dsp::{MAX_EXCITATION_LAYERS, RuntimeExcitationSlot},
    parameters::{ParameterApplyKind, apply_parameter_plain, finite_value, parameter_binding},
    patch::ResonatorSynthPatch,
    patch_io,
    runtime::{ResonatorProcessor, RuntimePatch, runtime_slot_from_config},
};

#[derive(Debug)]
pub struct ResonatorSynth {
    setup: ProcessSetup,
    patch: ResonatorSynthPatch,
    processor: ResonatorProcessor<'static>,
    loaded_buffers: [Option<RuntimeSampleBuffer>; MAX_EXCITATION_LAYERS],
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedExcitationBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: f32,
}

impl LoadedExcitationBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: f32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }
}

#[derive(Debug)]
struct RuntimeSampleBuffer {
    samples: Box<[f32]>,
    sample_rate: f32,
}

impl RuntimeSampleBuffer {
    fn from_loaded(buffer: LoadedExcitationBuffer) -> Self {
        Self {
            samples: buffer.samples.into_boxed_slice(),
            sample_rate: finite_value(buffer.sample_rate, 1.0, 384_000.0, 48_000.0),
        }
    }

    fn as_static_slice(&self) -> &'static [f32] {
        // The processor is dropped before `loaded_buffers` and rebuilt before buffers are replaced.
        // The heap allocation behind the box is stable when the owner Vec moves.
        unsafe { std::slice::from_raw_parts(self.samples.as_ptr(), self.samples.len()) }
    }
}

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
            std::array::from_fn(|_| buffers.next().map(RuntimeSampleBuffer::from_loaded));
        self.set_patch_with_runtime_buffers(patch, runtime_buffers);
    }

    pub fn set_patch_with_loaded_excitation_slots(
        &mut self,
        patch: ResonatorSynthPatch,
        buffers: [Option<LoadedExcitationBuffer>; MAX_EXCITATION_LAYERS],
    ) {
        let runtime_buffers = buffers.map(|buffer| buffer.map(RuntimeSampleBuffer::from_loaded));
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
        patch: ResonatorSynthPatch,
        runtime_buffers: [Option<RuntimeSampleBuffer>; MAX_EXCITATION_LAYERS],
    ) {
        self.patch = patch;
        self.processor = processor_from_patch_and_buffers(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            &runtime_buffers,
        );
        self.loaded_buffers = runtime_buffers;
    }

    pub fn set_parameter_normalized(
        &mut self,
        id: lindelion_plugin_shell::ParameterId,
        value: f32,
    ) {
        let Some(binding) = parameter_binding(id.0) else {
            return;
        };
        let plain = binding.info().range.denormalize(value);

        match apply_parameter_plain(&mut self.patch, id.0, plain) {
            ParameterApplyKind::Live => {
                if binding.runtime_target().is_active() {
                    self.processor.set_parameter_plain(id, plain);
                }
            }
            ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary) => {
                self.processor.replace_patch_config(self.patch.clone());
            }
            ParameterApplyKind::Structural(
                StructuralChangePolicy::ResetState
                | StructuralChangePolicy::LiveCrossfade
                | StructuralChangePolicy::LiveMuteRamp,
            ) => {
                if binding.runtime_target().is_active() {
                    self.processor.set_parameter_plain(id, plain);
                } else {
                    self.processor.replace_patch_config(self.patch.clone());
                }
            }
            ParameterApplyKind::Ignored => {}
        }
    }

    pub fn set_pitch_bend_normalized(&mut self, value: f32) {
        self.processor.set_pitch_bend_normalized(value);
    }

    fn rebuild_processor(&mut self) {
        self.processor = processor_from_patch_and_buffers(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            &self.loaded_buffers,
        );
    }

    #[cfg(test)]
    pub(crate) fn set_patch_for_test(&mut self, patch: ResonatorSynthPatch) {
        self.patch = patch;
        self.rebuild_processor();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleLoadReport {
    pub loaded_slots: usize,
    pub missing_samples: Vec<SampleReference>,
}

#[derive(Debug)]
pub enum SampleLoadError<E> {
    Library(E),
    Decode {
        reference: SampleReference,
        path: std::path::PathBuf,
        source: SampleDecodeError,
    },
}

type LoadedExcitationSlots = [Option<LoadedExcitationBuffer>; MAX_EXCITATION_LAYERS];

fn processor_from_patch_and_buffers(
    sample_rate: f32,
    patch: ResonatorSynthPatch,
    buffers: &[Option<RuntimeSampleBuffer>; MAX_EXCITATION_LAYERS],
) -> ResonatorProcessor<'static> {
    if buffers.iter().all(Option::is_none) {
        return ResonatorProcessor::with_builtin_excitation(sample_rate, patch);
    }

    ResonatorProcessor::new(
        sample_rate,
        RuntimePatch::new(patch.clone(), loaded_runtime_slots(&patch, buffers)),
    )
}

fn loaded_runtime_slots(
    patch: &ResonatorSynthPatch,
    buffers: &[Option<RuntimeSampleBuffer>; MAX_EXCITATION_LAYERS],
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
        slots[index] = Some(runtime_slot_from_config(
            &config,
            buffer.as_static_slice(),
            buffer.sample_rate,
        ));
    }
    slots
}

fn empty_runtime_buffers() -> [Option<RuntimeSampleBuffer>; MAX_EXCITATION_LAYERS] {
    std::array::from_fn(|_| None)
}

fn load_excitation_buffers_from_library<L>(
    patch: &ResonatorSynthPatch,
    library: &L,
) -> Result<(LoadedExcitationSlots, SampleLoadReport), SampleLoadError<L::Error>>
where
    L: SampleLibrary,
{
    let mut buffers = std::array::from_fn(|_| None);
    let mut missing_samples = Vec::new();
    let mut loaded_slots = 0;

    for (index, slot) in patch
        .excitation_slots
        .iter()
        .take(MAX_EXCITATION_LAYERS)
        .enumerate()
    {
        let Some(reference) = slot.sample.as_ref() else {
            continue;
        };

        match library
            .resolve(reference)
            .map_err(SampleLoadError::Library)?
        {
            SampleResolution::Found(path) => {
                let decoded = decode_wav_mono(&path).map_err(|source| SampleLoadError::Decode {
                    reference: reference.clone(),
                    path,
                    source,
                })?;
                buffers[index] = Some(LoadedExcitationBuffer::new(
                    decoded.samples,
                    decoded.sample_rate as f32,
                ));
                loaded_slots += 1;
            }
            SampleResolution::Missing(reference) => missing_samples.push(reference),
        }
    }

    Ok((
        buffers,
        SampleLoadReport {
            loaded_slots,
            missing_samples,
        },
    ))
}

fn load_excitation_buffers_from_sample_paths(
    patch: &ResonatorSynthPatch,
) -> (LoadedExcitationSlots, SampleLoadReport) {
    let mut buffers = std::array::from_fn(|_| None);
    let mut missing_samples = Vec::new();
    let mut loaded_slots = 0;

    for (index, slot) in patch
        .excitation_slots
        .iter()
        .take(MAX_EXCITATION_LAYERS)
        .enumerate()
    {
        let Some(reference) = slot.sample.as_ref() else {
            continue;
        };

        match decode_wav_mono(&reference.last_known_path) {
            Ok(decoded) => {
                buffers[index] = Some(LoadedExcitationBuffer::new(
                    decoded.samples,
                    decoded.sample_rate as f32,
                ));
                loaded_slots += 1;
            }
            Err(_) => missing_samples.push(reference.clone()),
        }
    }

    (
        buffers,
        SampleLoadReport {
            loaded_slots,
            missing_samples,
        },
    )
}

fn telemetry_from_audio(left: &[f32], right: &[f32], active_voices: usize) -> ResonatorTelemetry {
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

    let (left_peak, left_rms) = channel_stats(left);
    let (right_peak, right_rms) = channel_stats(right);
    ResonatorTelemetry {
        left_peak,
        right_peak,
        left_rms,
        right_rms,
        active_voices,
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
        self.rebuild_processor();
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        self.processor
            .process(context.events, context.buffer.left, context.buffer.right);
        self.telemetry = telemetry_from_audio(
            context.buffer.left,
            context.buffer.right,
            self.processor.active_voice_count(),
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
