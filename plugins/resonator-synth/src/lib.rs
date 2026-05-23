#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]

pub mod dsp;
mod parameters;
pub mod patch_io;
pub mod runtime;
mod vst3_entry;

pub use dsp::WaveguideStyle;
pub use parameters::PARAMETERS;

#[cfg(test)]
mod allocation_tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    thread_local! {
        static ALLOCATION_COUNT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocation();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn record_allocation() {
        ALLOCATION_COUNT.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    pub fn assert_no_allocations<R>(label: &str, run: impl FnOnce() -> R) -> R {
        ALLOCATION_COUNT.with(|count| count.set(Some(0)));
        let result = run();
        let allocations = ALLOCATION_COUNT.with(|count| {
            let allocations = count.get().unwrap_or(0);
            count.set(None);
            allocations
        });

        assert_eq!(allocations, 0, "{label} allocated {allocations} time(s)");
        result
    }
}

#[cfg(test)]
pub(crate) use allocation_tests::assert_no_allocations;

use ahara_dsp_utils::params::StructuralChangePolicy;
use ahara_plugin_shell::{
    AudioPlugin, ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};
use ahara_sample_library::{
    SampleDecodeError, SampleLibrary, SampleReference, SampleResolution, decode_wav_mono,
};
use dsp::{MAX_EXCITATION_LAYERS, RuntimeExcitationSlot};
#[cfg(test)]
pub(crate) use parameters::ParameterCodec;
pub(crate) use parameters::{
    ParameterApplyKind, RuntimeParameterTarget, apply_parameter_plain,
    apply_parameter_plain_for_controller, finite_value, parameter_binding,
    parameter_binding_by_index, parameter_binding_index, patch_parameter_plain_value,
};
use runtime::{ResonatorProcessor, RuntimePatch, runtime_slot_from_config};
use serde::{Deserialize, Serialize};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Ahara Resonator Synth", *b"ahara_resonator!");

const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResonatorSynthPatch {
    pub name: String,
    pub polyphony: u8,
    pub excitation_slots: Vec<ExcitationSlot>,
    pub resonator_a: ResonatorConfig,
    pub resonator_b: ResonatorConfig,
    pub routing: ResonatorRouting,
    #[serde(default)]
    pub retrigger_resonators: bool,
    pub output: OutputConfig,
    pub modulation: ModulationConfig,
}

impl Default for ResonatorSynthPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            polyphony: 8,
            excitation_slots: vec![ExcitationSlot::default()],
            resonator_a: ResonatorConfig::Modal(ModalConfig::default()),
            resonator_b: ResonatorConfig::Waveguide(WaveguideConfig::default()),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            retrigger_resonators: false,
            output: OutputConfig::default(),
            modulation: ModulationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExcitationSlot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<SampleReference>,
    pub gain_db: f32,
    pub velocity_low: u8,
    pub velocity_high: u8,
    pub start_offset_ms: f32,
    pub velocity_start_mod_ms: f32,
    pub looping: bool,
    pub pitch_track: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round_robin_group: Option<u8>,
}

impl Default for ExcitationSlot {
    fn default() -> Self {
        Self {
            sample: None,
            gain_db: 0.0,
            velocity_low: 0,
            velocity_high: 127,
            start_offset_ms: 0.0,
            velocity_start_mod_ms: 0.0,
            looping: false,
            pitch_track: false,
            round_robin_group: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonatorConfig {
    Modal(ModalConfig),
    Waveguide(WaveguideConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModalPreset {
    Kalimba,
    Marimba,
    Bell,
    GlassBowl,
    MetalBar,
    Woodblock,
    GenericStrike,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModalConfig {
    pub mode_count: u16,
    pub preset: ModalPreset,
    pub semitone_offset: i8,
    pub cent_offset: f32,
    pub inharmonicity: f32,
    pub brightness: f32,
    pub decay_global: f32,
    pub decay_tilt: f32,
    pub position_of_strike: f32,
}

impl Default for ModalConfig {
    fn default() -> Self {
        Self {
            mode_count: 64,
            preset: ModalPreset::Marimba,
            semitone_offset: 0,
            cent_offset: 0.0,
            inharmonicity: 0.0,
            brightness: 0.5,
            decay_global: 1.0,
            decay_tilt: 0.5,
            position_of_strike: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WaveguideConfig {
    #[serde(default)]
    pub style: WaveguideStyle,
    pub semitone_offset: i8,
    pub cent_offset: f32,
    pub loop_filter_cutoff: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub position_of_strike: f32,
    #[serde(default = "default_boundary_reflection")]
    pub boundary_reflection: f32,
}

impl Default for WaveguideConfig {
    fn default() -> Self {
        Self {
            style: WaveguideStyle::String,
            semitone_offset: 0,
            cent_offset: 0.0,
            loop_filter_cutoff: 8_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.92,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
            boundary_reflection: default_boundary_reflection(),
        }
    }
}

const fn default_boundary_reflection() -> f32 {
    0.75
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonatorRouting {
    Parallel { mix_a: f32, mix_b: f32 },
    Series { mix_a: f32, mix_b: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub filter_mode: FilterMode,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub saturation_drive: f32,
    pub master_gain_db: f32,
    pub master_pan: f32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            filter_mode: FilterMode::LowPass,
            filter_cutoff: 20_000.0,
            filter_resonance: 0.0,
            saturation_drive: 0.0,
            master_gain_db: 0.0,
            master_pan: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    LowPass,
    BandPass,
    HighPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeConfig {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            attack_ms: 1.0,
            decay_ms: 80.0,
            sustain: 1.0,
            release_ms: 250.0,
        }
    }
}

impl From<EnvelopeConfig> for ahara_dsp_utils::envelope::Adsr {
    fn from(value: EnvelopeConfig) -> Self {
        Self {
            attack_ms: value.attack_ms,
            decay_ms: value.decay_ms,
            sustain: value.sustain,
            release_ms: value.release_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LfoConfig {
    pub rate_hz: f32,
    pub shape: LfoShape,
    pub tempo_sync: bool,
}

impl Default for LfoConfig {
    fn default() -> Self {
        Self {
            rate_hz: 2.0,
            shape: LfoShape::Sine,
            tempo_sync: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    SampleAndHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModulationConfig {
    pub amp_envelope: EnvelopeConfig,
    pub secondary_envelope: EnvelopeConfig,
    pub lfo: LfoConfig,
    pub pitch_bend_range_semitones: f32,
    pub velocity_to_excitation_depth: f32,
    pub slots: [ModulationSlot; 4],
}

impl Default for ModulationConfig {
    fn default() -> Self {
        Self {
            amp_envelope: EnvelopeConfig::default(),
            secondary_envelope: EnvelopeConfig {
                attack_ms: 0.0,
                decay_ms: 250.0,
                sustain: 0.0,
                release_ms: 150.0,
            },
            lfo: LfoConfig::default(),
            pitch_bend_range_semitones: 2.0,
            velocity_to_excitation_depth: 1.0,
            slots: [ModulationSlot::default(); 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModulationSlot {
    pub enabled: bool,
    pub source: ModulationSource,
    pub destination: ModulationDestination,
    pub amount: f32,
}

impl Default for ModulationSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            source: ModulationSource::Velocity,
            destination: ModulationDestination::FilterCutoff,
            amount: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModulationSource {
    SecondaryEnvelope,
    Lfo,
    Velocity,
    Aftertouch,
    ModWheel,
    Brightness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModulationDestination {
    FilterCutoff,
    ResonatorADamping,
    ResonatorBDamping,
    ResonatorAPosition,
    ResonatorBPosition,
    ExcitationGain,
    LfoRate,
}

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

    pub fn set_parameter_normalized(&mut self, id: ahara_plugin_shell::ParameterId, value: f32) {
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
        PluginState {
            format_version: STATE_VERSION,
            payload: patch_io::to_toml_string(&self.patch)
                .unwrap_or_default()
                .into_bytes(),
        }
    }

    fn load_state(&mut self, state: PluginState) {
        if state.format_version != STATE_VERSION {
            return;
        }

        let Ok(payload) = std::str::from_utf8(&state.payload) else {
            return;
        };

        if let Ok(patch) = patch_io::from_toml_str(payload) {
            self.load_patch_from_sample_paths(patch);
        }
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    use ahara_dsp_utils::{
        analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
        db_to_gain,
        math::midi_note_to_hz,
    };
    use ahara_plugin_shell::{
        AudioBuffer, ControlEvent, MidiEvent, NoteEvent, ParameterId, ProcessMode,
    };
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn audio_plugin_process_renders_default_patch() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        synth.reset(setup);
        synth.process(ProcessContext {
            setup,
            buffer: AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            events: &events,
        });

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(rms(&left) > 0.000_001);
        assert!(rms(&right) > 0.000_001);
    }

    #[test]
    fn default_patch_uses_marimba_modal_as_only_audible_resonator_path() {
        let patch = ResonatorSynthPatch::default();

        assert!(matches!(
            patch.resonator_a,
            ResonatorConfig::Modal(ModalConfig {
                preset: ModalPreset::Marimba,
                ..
            })
        ));
        assert!(matches!(
            patch.routing,
            ResonatorRouting::Parallel { mix_a, mix_b }
                if (mix_a - 1.0).abs() < 0.001 && mix_b.abs() < 0.001
        ));
        assert!(!patch.retrigger_resonators);
        assert_eq!(patch.excitation_slots.len(), 1);
        assert!(patch.excitation_slots[0].sample.is_none());
    }

    #[test]
    fn default_single_note_velocity_100_has_nominal_headroom() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 16_384,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 16_384];
        let mut right = vec![0.0; 16_384];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 100.0 / 127.0,
        })];

        synth.reset(setup);
        synth.process(ProcessContext {
            setup,
            buffer: AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            events: &events,
        });

        assert_all_finite(&left);
        assert_all_finite(&right);
        let peak = peak_abs(&left).max(peak_abs(&right));
        assert!(
            peak <= db_to_gain(-6.0),
            "default velocity-100 single note peak {peak} exceeded -6 dBFS"
        );
    }

    #[test]
    fn master_gain_is_linear_when_saturation_is_off() {
        let quiet = render_default_note(-11.0, 0.0);
        let loud = render_default_note(-3.0, 0.0);
        let expected_gain = db_to_gain(8.0);
        let mut max_error = 0.0_f32;

        for (quiet_sample, loud_sample) in quiet.iter().copied().zip(loud.iter().copied()) {
            if quiet_sample.abs() < 1.0e-7 && loud_sample.abs() < 1.0e-7 {
                continue;
            }
            max_error = max_error.max((loud_sample - quiet_sample * expected_gain).abs());
        }

        assert!(
            max_error < 1.0e-5,
            "master gain changed waveform shape with saturation off; max error {max_error}"
        );
    }

    #[test]
    fn live_filter_cutoff_changes_do_not_emit_non_finite_audio() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 256,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 256];
        let mut right = vec![0.0; 256];

        synth.reset(setup);
        synth.process(ProcessContext {
            setup,
            buffer: AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            events: &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 60,
                velocity: 100.0 / 127.0,
            })],
        });

        for normalized in [
            1.0,
            0.75,
            0.5,
            0.25,
            0.0,
            f32::NAN,
            f32::INFINITY,
            0.01,
            1.0,
        ] {
            synth.set_parameter_normalized(ParameterId(3), normalized);
            synth.process(ProcessContext {
                setup,
                buffer: AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                events: &[],
            });
            assert_all_finite(&left);
            assert_all_finite(&right);
            let peak = peak_abs(&left).max(peak_abs(&right));
            assert!(peak < 16.0, "live cutoff change produced peak {peak}");
        }
    }

    #[test]
    fn rapid_exposed_parameter_changes_stay_finite_and_bounded() {
        let rendered = render_automation_stress_clip();
        let settled_start = 2_048;
        let max_delta = max_adjacent_delta(&rendered.left[settled_start..])
            .max(max_adjacent_delta(&rendered.right[settled_start..]));

        assert_all_finite(&rendered.left);
        assert_all_finite(&rendered.right);
        assert!(
            rendered.peak < 8.0,
            "automation stress peak={}",
            rendered.peak
        );
        assert!(
            max_delta < 0.35,
            "rapid automation should not introduce hard sample steps, max_delta={max_delta}"
        );
    }

    #[test]
    fn structural_parameter_changes_do_not_silence_active_voice() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 256,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 256];
        let mut right = vec![0.0; 256];
        let mut after_change = Vec::new();

        synth.reset(setup);
        process_block(
            &mut synth,
            setup,
            &mut left,
            &mut right,
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 60,
                velocity: 1.0,
            })],
        );
        for _ in 0..8 {
            process_block(&mut synth, setup, &mut left, &mut right, &[]);
        }
        for (id, plain) in [(10, 1.0), (20, 1.0), (40, 0.0), (60, 500.0), (69, 3.0)] {
            set_parameter_plain(&mut synth, id, plain);
        }
        for _ in 0..16 {
            process_block(&mut synth, setup, &mut left, &mut right, &[]);
            after_change.extend_from_slice(&left);
        }

        assert_all_finite(&after_change);
        assert!(
            rms(&after_change) > 0.000_001,
            "structural edits should not rebuild away active voices"
        );
    }

    #[test]
    fn ableton_style_four_bar_clip_renders_inside_expected_bounds() {
        let rendered = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);

        assert_all_finite(&rendered.left);
        assert_all_finite(&rendered.right);
        assert!(
            rendered.rms > 0.000_01,
            "QA clip should render audible output"
        );
        assert!(
            rendered.peak < 8.0,
            "QA clip peak should remain bounded, peak={}",
            rendered.peak
        );
    }

    #[test]
    fn velocity_changes_excitation_level() {
        let quiet = render_single_note_rms(48_000.0, 128, 60, 32.0 / 127.0);
        let medium = render_single_note_rms(48_000.0, 128, 60, 80.0 / 127.0);
        let loud = render_single_note_rms(48_000.0, 128, 60, 127.0 / 127.0);

        assert!(medium > quiet * 1.5, "quiet={quiet}, medium={medium}");
        assert!(loud > medium * 1.2, "medium={medium}, loud={loud}");
    }

    #[test]
    fn pitch_tracks_midi_notes() {
        let sample_rate = 48_000.0_f32;
        let c3 = render_single_note_left(sample_rate, 128, 48, 1.0);
        let c4 = render_single_note_left(sample_rate, 128, 60, 1.0);
        let c3_hz = midi_note_to_hz(48.0);
        let c4_hz = midi_note_to_hz(60.0);

        assert!(
            dft_magnitude_at(&c3[512..], sample_rate, c3_hz)
                > dft_magnitude_at(&c3[512..], sample_rate, c4_hz),
            "C3 render should favor C3 over C4"
        );
        assert!(
            dft_magnitude_at(&c4[512..], sample_rate, c4_hz)
                > dft_magnitude_at(&c4[512..], sample_rate, c3_hz),
            "C4 render should favor C4 over C3"
        );
    }

    #[test]
    fn pitch_bend_moves_held_note_about_two_semitones() {
        let sample_rate = 48_000.0_f32;
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: f64::from(sample_rate),
            max_block_size: 128,
            mode: ProcessMode::Realtime,
        };
        let mut block_left = vec![0.0; 128];
        let mut block_right = vec![0.0; 128];

        synth.reset(setup);
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 48,
                velocity: 1.0,
            })],
        );
        for _ in 0..4 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        }
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Control(ControlEvent::PitchBend {
                channel: 0,
                semitones: 2.0,
            })],
        );

        let mut bent = Vec::new();
        for _ in 0..64 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            bent.extend_from_slice(&block_left);
        }

        let original = midi_note_to_hz(48.0);
        let bent_up = midi_note_to_hz(50.0);
        assert!(
            dft_magnitude_at(&bent, sample_rate, bent_up)
                > dft_magnitude_at(&bent, sample_rate, original),
            "pitch bend should move the held voice up about two semitones"
        );
    }

    #[test]
    fn expression_aftertouch_changes_deterministic_render() {
        let neutral = render_expression_damping_clip(
            ModulationSource::Aftertouch,
            1,
            &[note_on(0, 48)],
            &[channel_pressure(0, 0.0)],
        );
        let pressed = render_expression_damping_clip(
            ModulationSource::Aftertouch,
            1,
            &[note_on(0, 48)],
            &[channel_pressure(0, 1.0)],
        );

        assert_expression_render_material_change("aftertouch", &neutral, &pressed);
    }

    #[test]
    fn expression_mod_wheel_changes_deterministic_render() {
        let neutral = render_expression_damping_clip(
            ModulationSource::ModWheel,
            1,
            &[note_on(0, 48)],
            &[cc(0, 1, 0.0)],
        );
        let pushed = render_expression_damping_clip(
            ModulationSource::ModWheel,
            1,
            &[note_on(0, 48)],
            &[cc(0, 1, 1.0)],
        );

        assert_expression_render_material_change("mod wheel", &neutral, &pushed);
    }

    #[test]
    fn expression_brightness_cc74_changes_deterministic_render() {
        let neutral = render_expression_damping_clip(
            ModulationSource::Brightness,
            1,
            &[note_on(0, 48)],
            &[cc(0, 74, 0.0)],
        );
        let bright = render_expression_damping_clip(
            ModulationSource::Brightness,
            1,
            &[note_on(0, 48)],
            &[cc(0, 74, 1.0)],
        );

        assert_expression_render_material_change("brightness", &neutral, &bright);
    }

    #[test]
    fn expression_poly_pressure_targets_note_in_deterministic_render() {
        let notes = [note_on(0, 48), note_on(0, 60)];
        let neutral = render_expression_damping_clip(
            ModulationSource::Aftertouch,
            2,
            &notes,
            &[poly_pressure(0, 48, 0.0)],
        );
        let wrong_note = render_expression_damping_clip(
            ModulationSource::Aftertouch,
            2,
            &notes,
            &[poly_pressure(0, 72, 1.0)],
        );
        let target_note = render_expression_damping_clip(
            ModulationSource::Aftertouch,
            2,
            &notes,
            &[poly_pressure(0, 48, 1.0)],
        );

        assert_expression_render_no_material_change(
            "wrong-note poly pressure",
            &neutral,
            &wrong_note,
        );
        assert_expression_render_material_change("poly pressure", &neutral, &target_note);
    }

    #[test]
    fn expression_member_channel_pitch_bend_retunes_deterministic_render() {
        let neutral = render_pitch_tracking_expression_clip(1, &[]);
        let wrong_channel = render_pitch_tracking_expression_clip(1, &[pitch_bend(2, 2.0)]);
        let bent = render_pitch_tracking_expression_clip(1, &[pitch_bend(1, 2.0)]);

        assert_expression_render_no_material_change(
            "wrong-channel pitch bend",
            &neutral,
            &wrong_channel,
        );
        assert_frequency_dominates(&neutral.left[512..], 48_000.0, 60.0, 62.0);
        assert_frequency_dominates(&bent.left[512..], 48_000.0, 62.0, 60.0);
        assert_expression_render_material_change("member-channel pitch bend", &neutral, &bent);
    }

    #[test]
    fn exposed_parameters_materially_change_rendered_audio() {
        let master_quiet = render_single_note_with_params(&[(1, -24.0)], 48_000.0, 128);
        let master_loud = render_single_note_with_params(&[(1, 0.0)], 48_000.0, 128);
        assert!(
            master_loud.rms > master_quiet.rms * 8.0,
            "master gain should materially change level"
        );

        let loop_low = render_loop_tail_rms(0.1);
        let loop_high = render_loop_tail_rms(0.99);
        assert!(
            loop_high > loop_low * 1.5,
            "loop gain should materially lengthen the tail, low={loop_low}, high={loop_high}"
        );

        let dark = render_single_note_with_params(&[(3, 250.0)], 48_000.0, 128);
        let open = render_single_note_with_params(&[(3, 20_000.0)], 48_000.0, 128);
        assert!(
            open.rms > dark.rms * 1.25,
            "filter cutoff should materially change output, dark={}, open={}",
            dark.rms,
            open.rms
        );
    }

    #[test]
    fn loop_resonance_parameters_are_per_slot_controls() {
        let mut synth = ResonatorSynth::default();

        set_parameter_plain(&mut synth, 20, 1.0);
        set_parameter_plain(&mut synth, 31, 0.72);
        set_parameter_plain(&mut synth, 51, 0.63);

        assert_resonator_a_loop_resonance(synth.patch(), 0.72);
        assert_resonator_b_loop_resonance(synth.patch(), 0.63);
    }

    #[test]
    fn loop_resonance_materially_changes_exposed_waveguide_render() {
        let dry = render_single_note_with_params(
            &[
                (20, 1.0),
                (11, 1.0),
                (12, 0.0),
                (30, 1_700.0),
                (31, 0.0),
                (32, 0.965),
            ],
            48_000.0,
            128,
        );
        let resonant = render_single_note_with_params(
            &[
                (20, 1.0),
                (11, 1.0),
                (12, 0.0),
                (30, 1_700.0),
                (31, 0.9),
                (32, 0.965),
            ],
            48_000.0,
            128,
        );

        assert_all_finite(&dry.left);
        assert_all_finite(&resonant.left);
        assert!(rms_difference(&dry.left[512..], &resonant.left[512..]) > 0.000_001);
    }

    #[test]
    fn parallel_mix_a_b_materially_changes_parallel_render() {
        let a_only =
            render_single_note_with_params(&[(10, 0.0), (11, 1.0), (12, 0.0)], 48_000.0, 128);
        let b_only =
            render_single_note_with_params(&[(10, 0.0), (11, 0.0), (12, 1.0)], 48_000.0, 128);

        assert!(a_only.rms > 0.000_001);
        assert!(b_only.rms > 0.000_001);
        assert!(rms_difference(&a_only.left[512..], &b_only.left[512..]) > 0.000_001);
    }

    #[test]
    fn output_filter_modes_produce_distinct_bounded_output() {
        let lowpass = render_filter_mode(0.0);
        let bandpass = render_filter_mode(1.0);
        let highpass = render_filter_mode(2.0);

        assert_rendered_clip_is_finite_and_bounded(&lowpass);
        assert_rendered_clip_is_finite_and_bounded(&bandpass);
        assert_rendered_clip_is_finite_and_bounded(&highpass);
        assert!(rms_difference(&lowpass.left[512..], &bandpass.left[512..]) > 0.000_001);
        assert!(rms_difference(&lowpass.left[512..], &highpass.left[512..]) > 0.000_001);
        assert!(lowpass.rms > highpass.rms * 1.1);
    }

    #[test]
    fn saturation_drive_materially_changes_tone_without_large_level_jump() {
        let dry = render_default_note(-3.0, 0.0);
        let driven = render_default_note(-3.0, 1.0);
        let dry_rms = rms(&dry);
        let driven_rms = rms(&driven);

        assert_all_finite(&dry);
        assert_all_finite(&driven);
        assert!(rms_difference(&dry, &driven) > 0.000_001);
        assert!(
            driven_rms > dry_rms * 0.35,
            "driven_rms={driven_rms}, dry_rms={dry_rms}"
        );
        assert!(
            driven_rms < dry_rms * 2.5,
            "driven_rms={driven_rms}, dry_rms={dry_rms}"
        );
    }

    #[test]
    fn render_clip_is_stable_across_buffer_sizes_and_sample_rates() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for block_size in [32, 64, 128, 512] {
                let rendered = render_qa_clip(sample_rate, block_size, ProcessMode::Realtime);

                assert_all_finite(&rendered.left);
                assert_all_finite(&rendered.right);
                assert!(
                    rendered.rms > 0.000_001,
                    "clip should not be silent at sample_rate={sample_rate}, block_size={block_size}"
                );
                assert!(
                    rendered.peak < 8.0,
                    "clip peak should stay bounded at sample_rate={sample_rate}, block_size={block_size}, peak={}",
                    rendered.peak
                );
            }
        }
    }

    #[test]
    fn offline_render_matches_realtime_for_fixed_clip() {
        let realtime = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);
        let offline = render_qa_clip(48_000.0, 128, ProcessMode::Offline);
        let mut max_diff = 0.0_f32;

        for (realtime_sample, offline_sample) in realtime
            .left
            .iter()
            .copied()
            .zip(offline.left.iter().copied())
        {
            max_diff = max_diff.max((realtime_sample - offline_sample).abs());
        }

        assert!(
            max_diff < 1.0e-6,
            "offline render should match realtime render, max diff {max_diff}"
        );
    }

    #[test]
    fn parameter_state_roundtrip_preserves_exposed_audio_controls() {
        let mut synth = ResonatorSynth::default();
        set_parameter_plain(&mut synth, 1, -9.0);
        set_parameter_plain(&mut synth, 3, 1_200.0);
        set_parameter_plain(&mut synth, 4, 0.25);
        set_parameter_plain(&mut synth, 52, 0.42);
        set_parameter_plain(&mut synth, 55, 1.0);
        set_parameter_plain(&mut synth, 56, -0.5);

        let state = AudioPlugin::state(&synth);
        let mut restored = ResonatorSynth::default();
        AudioPlugin::load_state(&mut restored, state);

        assert!((restored.patch().output.master_gain_db + 9.0).abs() < 0.001);
        assert!((restored.patch().output.filter_cutoff - 1_200.0).abs() < 0.001);
        assert!((restored.patch().output.saturation_drive - 0.25).abs() < 0.001);
        assert_resonator_b_loop_gain(restored.patch(), 0.42);
        assert_resonator_b_waveguide_style(restored.patch(), WaveguideStyle::Tube);
        assert_resonator_b_boundary_reflection(restored.patch(), -0.5);
    }

    #[test]
    fn exposes_complete_patch_parameter_surface() {
        let names = PARAMETERS
            .iter()
            .map(|parameter| parameter.name)
            .collect::<Vec<_>>();

        for expected in [
            "Master Gain",
            "Master Pan",
            "Filter Mode",
            "Filter Resonance",
            "Routing",
            "Retrigger Resonators",
            "Resonator A Model",
            "Resonator A Modal Preset",
            "Resonator A Mode Count",
            "Resonator A Brightness",
            "Resonator A Loop Resonance",
            "Resonator A Loop Gain",
            "Resonator A Waveguide Style",
            "Resonator A Boundary Reflection",
            "Resonator B Model",
            "Resonator B Loop Filter",
            "Resonator B Loop Resonance",
            "Resonator B Loop Gain",
            "Resonator B Waveguide Style",
            "Resonator B Boundary Reflection",
            "Amp Attack",
            "Amp Release",
            "LFO Shape",
            "Mod 1 Source",
            "Mod 4 Amount",
        ] {
            assert!(names.contains(&expected), "missing parameter {expected}");
        }

        assert!(
            !names.contains(&"Loop Gain"),
            "global Loop Gain should not be exposed"
        );
        assert!(
            PARAMETERS.len() >= 48,
            "parameter surface should cover the editable patch, got {}",
            PARAMETERS.len()
        );
    }

    #[test]
    fn removed_global_loop_gain_parameter_is_ignored() {
        let mut patch = ResonatorSynthPatch::default();
        assert!(PARAMETERS.iter().all(|parameter| parameter.id.0 != 2));
        assert_eq!(patch_parameter_plain_value(&patch, 2), None);
        assert_eq!(
            apply_parameter_plain(&mut patch, 2, 0.1),
            ParameterApplyKind::Ignored
        );

        let mut synth = ResonatorSynth::default();
        synth.set_parameter_normalized(ParameterId(2), 0.0);
        assert_resonator_b_loop_gain(synth.patch(), 0.92);
    }

    #[test]
    fn model_and_routing_parameters_are_explicit_binary_choices() {
        for id in [10, 13, 20, 35, 40, 55] {
            let parameter = PARAMETERS
                .iter()
                .find(|parameter| parameter.id.0 == id)
                .expect("binary choice parameter should exist");
            assert_eq!(
                parameter.step_count,
                Some(1),
                "parameter {}",
                parameter.name
            );
            assert_eq!(parameter.range.min, 0.0, "parameter {}", parameter.name);
            assert_eq!(parameter.range.max, 1.0, "parameter {}", parameter.name);
        }
    }

    #[test]
    fn modulation_source_parameters_cover_brightness_cc74() {
        let source_parameters = [81, 85, 89, 93].map(modulation_source_parameter_shape);
        assert_eq!(source_parameters, [(Some(5), 5.0); 4]);

        let sources = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0].map(ModulationSource::from_plain);
        assert_eq!(
            sources,
            [
                ModulationSource::SecondaryEnvelope,
                ModulationSource::Lfo,
                ModulationSource::Velocity,
                ModulationSource::Aftertouch,
                ModulationSource::ModWheel,
                ModulationSource::Brightness,
            ]
        );
        assert_eq!(ModulationSource::Brightness.plain(), 5.0);
        assert_eq!(ModulationSource::Aftertouch.label(), "Pressure");
        assert_eq!(ModulationSource::label_from_plain(5.0), "Brightness");
    }

    #[test]
    fn modulation_destination_parameters_format_as_labels() {
        let destinations =
            [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(ModulationDestination::from_plain);
        assert_eq!(
            destinations,
            [
                ModulationDestination::FilterCutoff,
                ModulationDestination::ResonatorADamping,
                ModulationDestination::ResonatorBDamping,
                ModulationDestination::ResonatorAPosition,
                ModulationDestination::ResonatorBPosition,
                ModulationDestination::ExcitationGain,
                ModulationDestination::LfoRate,
            ]
        );
        assert_eq!(ModulationDestination::ResonatorBPosition.plain(), 4.0);
        assert_eq!(
            ModulationDestination::ResonatorBPosition.label(),
            "Res B Position"
        );
        assert_eq!(ModulationDestination::label_from_plain(6.0), "LFO Rate");
    }

    fn modulation_source_parameter_shape(id: u32) -> (Option<u32>, f32) {
        let parameter = PARAMETERS
            .iter()
            .find(|parameter| parameter.id.0 == id)
            .expect("modulation source parameter should exist");
        (parameter.step_count, parameter.range.max)
    }

    #[test]
    fn structural_parameters_have_explicit_apply_policies() {
        let mut patch = ResonatorSynthPatch::default();

        assert_eq!(
            apply_parameter_plain(&mut patch, 7, 1.0),
            ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp)
        );
        assert_eq!(
            apply_parameter_plain(&mut patch, 10, 1.0),
            ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp)
        );
        assert_eq!(
            apply_parameter_plain(&mut patch, 13, 1.0),
            ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
        );
        assert_eq!(
            apply_parameter_plain(&mut patch, 20, 1.0),
            ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
        );
        assert_eq!(
            apply_parameter_plain(&mut patch, 35, 1.0),
            ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
        );
        assert_eq!(
            apply_parameter_plain(&mut patch, 11, 0.25),
            ParameterApplyKind::Live
        );
    }

    #[test]
    fn waveguide_style_parameters_are_per_slot_controls() {
        let mut synth = ResonatorSynth::default();

        set_parameter_plain(&mut synth, 20, 1.0);
        set_parameter_plain(&mut synth, 35, 1.0);
        set_parameter_plain(&mut synth, 36, -0.65);
        set_parameter_plain(&mut synth, 55, 1.0);
        set_parameter_plain(&mut synth, 56, 0.4);

        assert_resonator_a_waveguide_style(synth.patch(), WaveguideStyle::Tube);
        assert_resonator_a_boundary_reflection(synth.patch(), -0.65);
        assert_resonator_b_waveguide_style(synth.patch(), WaveguideStyle::Tube);
        assert_resonator_b_boundary_reflection(synth.patch(), 0.4);
    }

    #[test]
    fn only_model_selector_changes_selected_resonator_model() {
        let mut synth = ResonatorSynth::default();

        assert_resonator_model(synth.patch().resonator_a, 0);
        assert_resonator_model(synth.patch().resonator_b, 1);

        set_parameter_plain(&mut synth, 32, 0.25);
        set_parameter_plain(&mut synth, 46, 0.95);

        assert_resonator_model(synth.patch().resonator_a, 0);
        assert_resonator_model(synth.patch().resonator_b, 1);

        set_parameter_plain(&mut synth, 20, 1.0);
        set_parameter_plain(&mut synth, 40, 0.0);

        assert_resonator_model(synth.patch().resonator_a, 1);
        assert_resonator_model(synth.patch().resonator_b, 0);
    }

    #[test]
    fn routing_switch_preserves_parallel_mix_values() {
        let mut synth = ResonatorSynth::default();

        set_parameter_plain(&mut synth, 11, 0.8);
        set_parameter_plain(&mut synth, 12, 0.2);
        assert_parallel_mix(synth.patch().routing, 0.8, 0.2);

        set_parameter_plain(&mut synth, 10, 1.0);
        assert_series_mix(synth.patch().routing, 0.8, 0.2);

        set_parameter_plain(&mut synth, 11, 0.25);
        assert_series_mix(synth.patch().routing, 0.25, 0.2);

        set_parameter_plain(&mut synth, 10, 0.0);
        assert_parallel_mix(synth.patch().routing, 0.25, 0.2);
    }

    fn assert_resonator_model(config: ResonatorConfig, expected: u8) {
        assert_eq!(resonator_model_index(config), expected);
    }

    fn resonator_model_index(config: ResonatorConfig) -> u8 {
        match config {
            ResonatorConfig::Modal(_) => 0,
            ResonatorConfig::Waveguide(_) => 1,
        }
    }

    fn assert_parallel_mix(routing: ResonatorRouting, expected_a: f32, expected_b: f32) {
        let ResonatorRouting::Parallel { mix_a, mix_b } = routing else {
            panic!("expected parallel routing, got {routing:?}");
        };
        assert_mix_values(mix_a, mix_b, expected_a, expected_b);
    }

    fn assert_series_mix(routing: ResonatorRouting, expected_a: f32, expected_b: f32) {
        let ResonatorRouting::Series { mix_a, mix_b } = routing else {
            panic!("expected series routing, got {routing:?}");
        };
        assert_mix_values(mix_a, mix_b, expected_a, expected_b);
    }

    fn assert_mix_values(mix_a: f32, mix_b: f32, expected_a: f32, expected_b: f32) {
        assert!((mix_a - expected_a).abs() < 0.001, "mix_a={mix_a}");
        assert!((mix_b - expected_b).abs() < 0.001, "mix_b={mix_b}");
    }

    #[test]
    fn expanded_parameter_updates_mutate_patch_and_roundtrip() {
        let patch = roundtrip_patch_after_parameter_updates();

        assert_expanded_output_and_routing(&patch);
        assert_expanded_resonator_parameters(&patch);
        assert_expanded_modulation_parameters(&patch);
    }

    fn roundtrip_patch_after_parameter_updates() -> ResonatorSynthPatch {
        let mut synth = ResonatorSynth::default();

        for (id, plain) in [
            (5, -0.5),
            (6, 0.35),
            (7, 2.0),
            (10, 1.0),
            (13, 1.0),
            (20, 1.0),
            (32, 0.975),
            (40, 0.0),
            (41, 4.0),
            (46, 0.8),
            (60, 12.0),
            (63, 480.0),
            (68, 7.5),
            (69, 3.0),
            (80, 1.0),
            (81, 5.0),
            (82, 3.0),
            (83, -0.33),
        ] {
            set_parameter_plain(&mut synth, id, plain);
        }

        let state = AudioPlugin::state(&synth);
        let mut restored = ResonatorSynth::default();
        AudioPlugin::load_state(&mut restored, state);

        restored.patch().clone()
    }

    fn assert_expanded_output_and_routing(patch: &ResonatorSynthPatch) {
        assert_eq!(patch.output.filter_mode, FilterMode::HighPass);
        assert!((patch.output.master_pan + 0.5).abs() < 0.001);
        assert!((patch.output.filter_resonance - 0.35).abs() < 0.001);
        assert!(matches!(patch.routing, ResonatorRouting::Series { .. }));
        assert!(patch.retrigger_resonators);
    }

    fn assert_expanded_resonator_parameters(patch: &ResonatorSynthPatch) {
        assert!(matches!(
            patch.resonator_a,
            ResonatorConfig::Waveguide(WaveguideConfig { loop_gain, .. })
                if (loop_gain - 0.975).abs() < 0.001
        ));
        assert!(matches!(
            patch.resonator_b,
            ResonatorConfig::Modal(ModalConfig {
                preset: ModalPreset::MetalBar,
                brightness,
                ..
            }) if (brightness - 0.8).abs() < 0.001
        ));
    }

    fn assert_expanded_modulation_parameters(patch: &ResonatorSynthPatch) {
        assert!((patch.modulation.amp_envelope.attack_ms - 12.0).abs() < 0.001);
        assert!((patch.modulation.amp_envelope.release_ms - 480.0).abs() < 0.001);
        assert!((patch.modulation.lfo.rate_hz - 7.5).abs() < 0.001);
        assert_eq!(patch.modulation.lfo.shape, LfoShape::Square);
        assert!(patch.modulation.slots[0].enabled);
        assert_eq!(
            patch.modulation.slots[0].source,
            ModulationSource::Brightness
        );
        assert_eq!(
            patch.modulation.slots[0].destination,
            ModulationDestination::ResonatorAPosition
        );
        assert!((patch.modulation.slots[0].amount + 0.33).abs() < 0.001);
    }

    #[test]
    fn repeated_clip_loop_note_offs_remain_bounded_and_decay_after_stop() {
        let sample_rate = 48_000.0;
        let block_size = 128;
        let setup = ProcessSetup {
            sample_rate,
            max_block_size: block_size,
            mode: ProcessMode::Realtime,
        };
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; block_size];
        let mut block_right = vec![0.0; block_size];
        let mut start_tail = Vec::new();
        let mut end_tail = Vec::new();

        synth.reset(setup);
        for loop_index in 0..4 {
            process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &[MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note: 48 + loop_index,
                    velocity: 100.0 / 127.0,
                })],
            );
            assert!(peak_abs(&block_left).max(peak_abs(&block_right)) < 8.0);

            for _ in 0..24 {
                process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
                assert_all_finite(&block_left);
                assert_all_finite(&block_right);
            }

            process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &[MidiEvent::Note(NoteEvent::Off {
                    channel: 0,
                    note: 48 + loop_index,
                    velocity: 0.0,
                })],
            );
        }

        for tail_block in 0..800 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            assert_all_finite(&block_left);
            assert_all_finite(&block_right);
            if tail_block < 64 {
                start_tail.extend_from_slice(&block_left);
            } else if tail_block >= 736 {
                end_tail.extend_from_slice(&block_left);
            }
        }

        assert!(
            rms(&end_tail) < rms(&start_tail) * 0.1,
            "output should decay after repeated note-offs"
        );
    }

    #[test]
    fn audio_plugin_process_does_not_allocate() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        synth.reset(setup);
        assert_no_allocations("audio plugin process", || {
            synth.process(ProcessContext {
                setup,
                buffer: AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                events: &events,
            });
        });
    }

    #[test]
    fn loaded_excitation_buffers_render_without_audio_thread_allocations() {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };
        let mut patch = ResonatorSynthPatch {
            polyphony: 2,
            resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.97,
                loop_filter_cutoff: 14_000.0,
                ..WaveguideConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                master_gain_db: -6.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        };
        patch.excitation_slots[0].gain_db = -3.0;
        patch.excitation_slots[0].pitch_track = true;
        let excitation = (0..96)
            .map(|index| {
                let phase = index as f32 / 96.0;
                (phase * std::f32::consts::TAU * 8.0).sin() * (1.0 - phase)
            })
            .collect::<Vec<_>>();
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        synth.reset(setup);
        synth.set_patch_with_loaded_excitations(
            patch,
            vec![LoadedExcitationBuffer::new(excitation, 48_000.0)],
        );
        assert_no_allocations("loaded excitation render", || {
            synth.process(ProcessContext {
                setup,
                buffer: AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                events: &events,
            });
        });

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(peak_abs(&left).max(peak_abs(&right)) > 0.000_001);
    }

    #[test]
    fn patch_sample_references_load_from_sample_library_and_render() {
        let root = temp_root("sample-ref-load");
        let sample_path = root.join("strike.wav");
        write_test_wav(&sample_path, &[0.0, 0.7, -0.35, 0.18, 0.0]);
        let reference = SampleReference::new("sample-hash", sample_path.clone());
        let mut patch = waveguide_tail_test_patch();
        patch.excitation_slots[0].sample = Some(reference.clone());
        patch.excitation_slots[0].gain_db = -3.0;
        patch.excitation_slots[0].pitch_track = true;
        let library = StaticSampleLibrary {
            path: Some(sample_path),
        };
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        synth.reset(setup);
        let report = synth
            .load_patch_from_sample_library(patch, &library)
            .unwrap();
        assert_eq!(report.loaded_slots, 1);
        assert!(report.missing_samples.is_empty());
        assert_no_allocations("resolved sample render", || {
            synth.process(ProcessContext {
                setup,
                buffer: AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                events: &[MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note: 60,
                    velocity: 1.0,
                })],
            });
        });

        assert_all_finite(&left);
        assert!(peak_abs(&left).max(peak_abs(&right)) > 0.000_001);
    }

    #[test]
    fn patch_sample_reference_missing_reports_without_crashing() {
        let reference = SampleReference::new("missing-hash", "Samples/missing.wav");
        let mut patch = ResonatorSynthPatch::default();
        patch.excitation_slots[0].sample = Some(reference.clone());
        let mut synth = ResonatorSynth::default();
        let report = synth
            .load_patch_from_sample_library(patch, &StaticSampleLibrary { path: None })
            .unwrap();

        assert_eq!(report.loaded_slots, 0);
        assert_eq!(report.missing_samples, vec![reference]);
    }

    #[test]
    fn state_load_resolves_absolute_sample_references_and_preserves_render() {
        let root = temp_root("sample-state-load");
        let sample_path = root.join("state-strike.wav");
        write_test_wav(&sample_path, &[0.0, 0.9, -0.45, 0.2, -0.1, 0.0]);
        let reference = SampleReference::new("state-hash", sample_path);
        let mut patch = waveguide_tail_test_patch();
        patch.excitation_slots[0].sample = Some(reference);
        patch.excitation_slots[0].gain_db = -3.0;
        patch.excitation_slots[0].pitch_track = true;
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };

        let mut source = ResonatorSynth::default();
        source.reset(setup);
        let report = source.load_patch_from_sample_paths(patch);
        assert_eq!(report.loaded_slots, 1);
        assert!(report.missing_samples.is_empty());
        let expected = render_one_block_after_state_load(&mut source, setup);
        let state = source.state();

        let mut restored = ResonatorSynth::default();
        restored.reset(setup);
        restored.load_state(state);
        let actual = render_one_block_after_state_load(&mut restored, setup);

        assert_all_finite(&actual);
        assert!(peak_abs(&actual) > 0.000_001);
        let max_diff = expected
            .iter()
            .copied()
            .zip(actual.iter().copied())
            .map(|(expected, actual)| (expected - actual).abs())
            .fold(0.0, f32::max);
        assert!(
            max_diff < 1.0e-6,
            "state load should restore sample-backed render, max_diff={max_diff}"
        );
    }

    fn render_one_block_after_state_load(
        synth: &mut ResonatorSynth,
        setup: ProcessSetup,
    ) -> Vec<f32> {
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        assert_no_allocations("state-loaded sample render", || {
            synth.process(ProcessContext {
                setup,
                buffer: AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                events: &events,
            });
        });
        left
    }

    fn render_default_note(master_gain_db: f32, saturation_drive: f32) -> Vec<f32> {
        let mut synth = ResonatorSynth::default();
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 16_384,
            mode: ProcessMode::Realtime,
        };
        let mut patch = ResonatorSynthPatch::default();
        patch.output.master_gain_db = master_gain_db;
        patch.output.saturation_drive = saturation_drive;
        let mut left = vec![0.0; 16_384];
        let mut right = vec![0.0; 16_384];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 100.0 / 127.0,
        })];

        synth.reset(setup);
        synth.set_patch_for_test(patch);
        synth.process(ProcessContext {
            setup,
            buffer: AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            events: &events,
        });

        left
    }

    #[derive(Debug)]
    struct RenderedClip {
        left: Vec<f32>,
        right: Vec<f32>,
        rms: f32,
        peak: f32,
    }

    #[derive(Debug, Clone, Copy)]
    enum ScheduledActionKind {
        Event(MidiEvent),
        Parameter { id: u32, plain: f32 },
    }

    #[derive(Debug, Clone, Copy)]
    struct ScheduledAction {
        block: usize,
        order: usize,
        kind: ScheduledActionKind,
    }

    fn render_qa_clip(sample_rate: f32, block_size: usize, mode: ProcessMode) -> RenderedClip {
        let setup = ProcessSetup {
            sample_rate: f64::from(sample_rate),
            max_block_size: block_size,
            mode,
        };
        let total_blocks = ((sample_rate * 8.0).ceil() as usize).div_ceil(block_size);
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; block_size];
        let mut block_right = vec![0.0; block_size];
        let mut left = Vec::with_capacity(total_blocks * block_size);
        let mut right = Vec::with_capacity(total_blocks * block_size);
        let mut schedule = qa_clip_schedule(sample_rate, block_size, total_blocks);
        let mut cursor = 0;
        let mut events = Vec::with_capacity(16);

        schedule.sort_by_key(|action| (action.block, action.order));
        synth.reset(setup);

        for block in 0..total_blocks {
            events.clear();
            while cursor < schedule.len() && schedule[cursor].block == block {
                match schedule[cursor].kind {
                    ScheduledActionKind::Event(event) => events.push(event),
                    ScheduledActionKind::Parameter { id, plain } => {
                        set_parameter_plain(&mut synth, id, plain);
                    }
                }
                cursor += 1;
            }

            process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &events,
            );
            left.extend_from_slice(&block_left);
            right.extend_from_slice(&block_right);
        }

        let rms = rms(&left).max(rms(&right));
        let peak = peak_abs(&left).max(peak_abs(&right));
        RenderedClip {
            left,
            right,
            rms,
            peak,
        }
    }

    fn render_automation_stress_clip() -> RenderedClip {
        let sample_rate = 48_000.0;
        let block_size = 128;
        let total_blocks = 160;
        let setup = ProcessSetup {
            sample_rate,
            max_block_size: block_size,
            mode: ProcessMode::Realtime,
        };
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; block_size];
        let mut block_right = vec![0.0; block_size];
        let mut left = Vec::with_capacity(total_blocks * block_size);
        let mut right = Vec::with_capacity(total_blocks * block_size);

        synth.reset(setup);
        for block in 0..total_blocks {
            if (16..96).contains(&block) && block % 2 == 0 {
                let high = (block / 2) % 2 == 0;
                set_parameter_plain(&mut synth, 1, if high { 6.0 } else { -42.0 });
                set_parameter_plain(&mut synth, 52, if high { 0.99 } else { 0.1 });
                set_parameter_plain(&mut synth, 3, if high { 20_000.0 } else { 250.0 });
                set_parameter_plain(&mut synth, 6, if high { 0.9 } else { 0.0 });
                set_parameter_plain(&mut synth, 4, if high { 1.0 } else { 0.0 });
            }

            let note_on = [MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 48,
                velocity: 1.0,
            })];
            let events = if block == 0 { &note_on[..] } else { &[] };
            process_block(&mut synth, setup, &mut block_left, &mut block_right, events);
            left.extend_from_slice(&block_left);
            right.extend_from_slice(&block_right);
        }

        let rms = rms(&left).max(rms(&right));
        let peak = peak_abs(&left).max(peak_abs(&right));
        RenderedClip {
            left,
            right,
            rms,
            peak,
        }
    }

    fn qa_clip_schedule(
        sample_rate: f32,
        block_size: usize,
        total_blocks: usize,
    ) -> Vec<ScheduledAction> {
        let mut builder = ScheduleBuilder::new(sample_rate, block_size, total_blocks);

        builder.parameter(0.0, 4, 0.0);

        builder.note(0.00, 0.25, 36, 32.0 / 127.0);
        builder.note(0.50, 0.75, 48, 80.0 / 127.0);
        builder.note(1.00, 1.25, 60, 1.0);

        for note in [48, 52, 55] {
            builder.note(2.00, 2.20, note, 100.0 / 127.0);
        }
        for note in [36, 40, 43, 47, 48, 52, 55, 59] {
            builder.note(2.75, 3.00, note, 95.0 / 127.0);
        }

        builder.note(4.00, 5.85, 48, 100.0 / 127.0);
        builder.pitch_bend(4.50, -2.0);
        builder.pitch_bend(5.00, 0.0);
        builder.pitch_bend(5.50, 2.0);
        builder.pitch_bend(5.85, 0.0);

        builder.parameter(6.00, 1, -60.0);
        builder.note(6.00, 7.75, 60, 100.0 / 127.0);
        builder.parameter(6.25, 1, 0.0);
        builder.parameter(6.50, 1, 12.0);
        builder.parameter(6.75, 52, 0.1);
        builder.parameter(7.00, 52, 0.98);
        builder.parameter(7.25, 3, 20.0);
        builder.parameter(7.50, 3, 20_000.0);

        builder.into_schedule()
    }

    struct ScheduleBuilder {
        sample_rate: f32,
        block_size: usize,
        total_blocks: usize,
        order: usize,
        schedule: Vec<ScheduledAction>,
    }

    impl ScheduleBuilder {
        fn new(sample_rate: f32, block_size: usize, total_blocks: usize) -> Self {
            Self {
                sample_rate,
                block_size,
                total_blocks,
                order: 0,
                schedule: Vec::new(),
            }
        }

        fn note(&mut self, start_seconds: f32, end_seconds: f32, note: u8, velocity: f32) {
            self.event(
                start_seconds,
                MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note,
                    velocity,
                }),
            );
            self.event(
                end_seconds,
                MidiEvent::Note(NoteEvent::Off {
                    channel: 0,
                    note,
                    velocity: 0.0,
                }),
            );
        }

        fn pitch_bend(&mut self, seconds: f32, semitones: f32) {
            self.event(
                seconds,
                MidiEvent::Control(ControlEvent::PitchBend {
                    channel: 0,
                    semitones,
                }),
            );
        }

        fn parameter(&mut self, seconds: f32, id: u32, plain: f32) {
            let block = self.block_at(seconds);
            let order = self.next_order();
            self.schedule.push(ScheduledAction {
                block,
                order,
                kind: ScheduledActionKind::Parameter { id, plain },
            });
        }

        fn event(&mut self, seconds: f32, event: MidiEvent) {
            let block = self.block_at(seconds);
            let order = self.next_order();
            self.schedule.push(ScheduledAction {
                block,
                order,
                kind: ScheduledActionKind::Event(event),
            });
        }

        fn block_at(&self, seconds: f32) -> usize {
            ((seconds * self.sample_rate) as usize / self.block_size)
                .min(self.total_blocks.saturating_sub(1))
        }

        fn next_order(&mut self) -> usize {
            let current = self.order;
            self.order += 1;
            current
        }

        fn into_schedule(self) -> Vec<ScheduledAction> {
            self.schedule
        }
    }

    fn process_block(
        synth: &mut ResonatorSynth,
        setup: ProcessSetup,
        left: &mut [f32],
        right: &mut [f32],
        events: &[MidiEvent],
    ) {
        synth.process(ProcessContext {
            setup,
            buffer: AudioBuffer { left, right },
            events,
        });
    }

    fn render_single_note_rms(sample_rate: f32, block_size: usize, note: u8, velocity: f32) -> f32 {
        let rendered =
            render_single_note_with_params_and_note(&[], sample_rate, block_size, note, velocity);
        rendered.rms
    }

    fn render_single_note_left(
        sample_rate: f32,
        block_size: usize,
        note: u8,
        velocity: f32,
    ) -> Vec<f32> {
        render_single_note_with_params_and_note(&[], sample_rate, block_size, note, velocity).left
    }

    fn render_single_note_with_params(
        params: &[(u32, f32)],
        sample_rate: f32,
        block_size: usize,
    ) -> RenderedClip {
        render_single_note_with_params_and_note(params, sample_rate, block_size, 60, 100.0 / 127.0)
    }

    fn render_filter_mode(mode: f32) -> RenderedClip {
        render_single_note_with_params(&[(3, 1_200.0), (6, 0.35), (7, mode)], 48_000.0, 128)
    }

    fn assert_rendered_clip_is_finite_and_bounded(rendered: &RenderedClip) {
        assert_all_finite(&rendered.left);
        assert_all_finite(&rendered.right);
        assert!(rendered.rms > 0.000_001);
        assert!(rendered.peak < 8.0);
    }

    fn render_single_note_with_params_and_note(
        params: &[(u32, f32)],
        sample_rate: f32,
        block_size: usize,
        note: u8,
        velocity: f32,
    ) -> RenderedClip {
        let setup = ProcessSetup {
            sample_rate: f64::from(sample_rate),
            max_block_size: block_size,
            mode: ProcessMode::Realtime,
        };
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; block_size];
        let mut block_right = vec![0.0; block_size];
        let mut left = Vec::with_capacity(block_size * 96);
        let mut right = Vec::with_capacity(block_size * 96);

        synth.reset(setup);
        for (id, plain) in params {
            set_parameter_plain(&mut synth, *id, *plain);
        }

        for block in 0..96 {
            if block == 0 {
                process_block(
                    &mut synth,
                    setup,
                    &mut block_left,
                    &mut block_right,
                    &[MidiEvent::Note(NoteEvent::On {
                        channel: 0,
                        note,
                        velocity,
                    })],
                );
            } else {
                process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            }
            left.extend_from_slice(&block_left);
            right.extend_from_slice(&block_right);
        }

        let rms = rms(&left).max(rms(&right));
        let peak = peak_abs(&left).max(peak_abs(&right));
        RenderedClip {
            left,
            right,
            rms,
            peak,
        }
    }

    fn render_expression_damping_clip(
        source: ModulationSource,
        polyphony: u8,
        start_events: &[MidiEvent],
        control_events: &[MidiEvent],
    ) -> RenderedClip {
        render_expression_clip(
            expression_damping_test_patch(source, polyphony),
            start_events,
            control_events,
        )
    }

    fn render_pitch_tracking_expression_clip(
        channel: u8,
        control_events: &[MidiEvent],
    ) -> RenderedClip {
        render_expression_clip(
            pitch_tracking_expression_patch(),
            &[note_on(channel, 60)],
            control_events,
        )
    }

    fn render_expression_clip(
        patch: ResonatorSynthPatch,
        start_events: &[MidiEvent],
        control_events: &[MidiEvent],
    ) -> RenderedClip {
        let sample_rate = 48_000.0;
        let block_size = 128;
        let setup = ProcessSetup {
            sample_rate,
            max_block_size: block_size,
            mode: ProcessMode::Realtime,
        };
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; block_size];
        let mut block_right = vec![0.0; block_size];
        let mut left = Vec::with_capacity(block_size * 65);
        let mut right = Vec::with_capacity(block_size * 65);

        synth.reset(setup);
        synth.set_patch_for_test(patch);
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            start_events,
        );
        for _ in 0..4 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        }
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            control_events,
        );
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);

        for _ in 0..64 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            left.extend_from_slice(&block_left);
            right.extend_from_slice(&block_right);
        }

        rendered_clip(left, right)
    }

    fn rendered_clip(left: Vec<f32>, right: Vec<f32>) -> RenderedClip {
        let rms = rms(&left).max(rms(&right));
        let peak = peak_abs(&left).max(peak_abs(&right));
        RenderedClip {
            left,
            right,
            rms,
            peak,
        }
    }

    fn assert_expression_render_material_change(
        label: &str,
        neutral: &RenderedClip,
        changed: &RenderedClip,
    ) {
        assert_rendered_clip_is_finite_and_bounded(neutral);
        assert_rendered_clip_is_finite_and_bounded(changed);
        let diff = rms_difference(&neutral.left[512..], &changed.left[512..]);
        assert!(
            diff > neutral.rms.max(0.000_001) * 0.05,
            "{label} should materially change render, neutral_rms={}, changed_rms={}, diff={diff}",
            neutral.rms,
            changed.rms
        );
    }

    fn assert_expression_render_no_material_change(
        label: &str,
        neutral: &RenderedClip,
        changed: &RenderedClip,
    ) {
        assert_rendered_clip_is_finite_and_bounded(neutral);
        assert_rendered_clip_is_finite_and_bounded(changed);
        let diff = rms_difference(&neutral.left[512..], &changed.left[512..]);
        assert!(
            diff < 1.0e-7,
            "{label} should not materially change render, diff={diff}"
        );
    }

    fn render_loop_tail_rms(loop_gain: f32) -> f32 {
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 128,
            mode: ProcessMode::Realtime,
        };
        let mut synth = ResonatorSynth::default();
        let mut block_left = vec![0.0; 128];
        let mut block_right = vec![0.0; 128];
        let mut tail = Vec::new();

        synth.reset(setup);
        synth.set_patch_for_test(waveguide_tail_test_patch());
        set_parameter_plain(&mut synth, 32, loop_gain);
        for block in 0..256 {
            match block {
                0 => process_block(
                    &mut synth,
                    setup,
                    &mut block_left,
                    &mut block_right,
                    &[MidiEvent::Note(NoteEvent::On {
                        channel: 0,
                        note: 60,
                        velocity: 1.0,
                    })],
                ),
                24 => process_block(
                    &mut synth,
                    setup,
                    &mut block_left,
                    &mut block_right,
                    &[MidiEvent::Note(NoteEvent::Off {
                        channel: 0,
                        note: 60,
                        velocity: 0.0,
                    })],
                ),
                _ => process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]),
            }
            if block >= 96 {
                tail.extend_from_slice(&block_left);
            }
        }

        rms(&tail)
    }

    fn waveguide_tail_test_patch() -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            polyphony: 1,
            resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.92,
                loop_filter_cutoff: 18_000.0,
                ..WaveguideConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                master_gain_db: 0.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }

    fn expression_damping_test_patch(
        source: ModulationSource,
        polyphony: u8,
    ) -> ResonatorSynthPatch {
        let mut patch = waveguide_tail_test_patch();
        patch.polyphony = polyphony;
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.62,
            loop_filter_cutoff: 12_000.0,
            ..WaveguideConfig::default()
        });
        patch.modulation.slots[0] = ModulationSlot {
            enabled: true,
            source,
            destination: ModulationDestination::ResonatorADamping,
            amount: 1.0,
        };
        patch
    }

    fn pitch_tracking_expression_patch() -> ResonatorSynthPatch {
        let mut patch = waveguide_tail_test_patch();
        patch.polyphony = 1;
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.99,
            loop_filter_cutoff: 12_000.0,
            ..WaveguideConfig::default()
        });
        patch
    }

    fn note_on(channel: u8, note: u8) -> MidiEvent {
        MidiEvent::Note(NoteEvent::On {
            channel,
            note,
            velocity: 1.0,
        })
    }

    fn channel_pressure(channel: u8, value: f32) -> MidiEvent {
        MidiEvent::Control(ControlEvent::ChannelPressure { channel, value })
    }

    fn poly_pressure(channel: u8, note: u8, value: f32) -> MidiEvent {
        MidiEvent::Control(ControlEvent::PolyPressure {
            channel,
            note,
            value,
        })
    }

    fn cc(channel: u8, controller: u8, value: f32) -> MidiEvent {
        MidiEvent::Control(ControlEvent::ContinuousController {
            channel,
            controller,
            value,
        })
    }

    fn pitch_bend(channel: u8, semitones: f32) -> MidiEvent {
        MidiEvent::Control(ControlEvent::PitchBend { channel, semitones })
    }

    fn assert_frequency_dominates(
        samples: &[f32],
        sample_rate: f32,
        high_note: f32,
        low_note: f32,
    ) {
        let high = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(high_note));
        let low = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(low_note));
        assert!(
            high > low,
            "note {high_note} magnitude {high} should exceed note {low_note} magnitude {low}"
        );
    }

    fn set_parameter_plain(synth: &mut ResonatorSynth, id: u32, plain: f32) {
        let Some(parameter) = PARAMETERS
            .iter()
            .find(|parameter| parameter.id == ParameterId(id))
        else {
            return;
        };
        synth.set_parameter_normalized(ParameterId(id), parameter.range.normalize(plain));
    }

    fn assert_resonator_b_loop_gain(patch: &ResonatorSynthPatch, expected: f32) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
            panic!("expected resonator B to be waveguide");
        };
        assert!((config.loop_gain - expected).abs() < 0.001);
    }

    fn assert_resonator_a_loop_resonance(patch: &ResonatorSynthPatch, expected: f32) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
            panic!("expected resonator A to be waveguide");
        };
        assert!((config.loop_filter_resonance - expected).abs() < 0.001);
    }

    fn assert_resonator_b_loop_resonance(patch: &ResonatorSynthPatch, expected: f32) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
            panic!("expected resonator B to be waveguide");
        };
        assert!((config.loop_filter_resonance - expected).abs() < 0.001);
    }

    fn assert_resonator_a_waveguide_style(patch: &ResonatorSynthPatch, expected: WaveguideStyle) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
            panic!("expected resonator A to be waveguide");
        };
        assert_eq!(config.style, expected);
    }

    fn assert_resonator_b_waveguide_style(patch: &ResonatorSynthPatch, expected: WaveguideStyle) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
            panic!("expected resonator B to be waveguide");
        };
        assert_eq!(config.style, expected);
    }

    fn assert_resonator_a_boundary_reflection(patch: &ResonatorSynthPatch, expected: f32) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
            panic!("expected resonator A to be waveguide");
        };
        assert!((config.boundary_reflection - expected).abs() < 0.001);
    }

    fn assert_resonator_b_boundary_reflection(patch: &ResonatorSynthPatch, expected: f32) {
        let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
            panic!("expected resonator B to be waveguide");
        };
        assert!((config.boundary_reflection - expected).abs() < 0.001);
    }

    fn rms_difference(a: &[f32], b: &[f32]) -> f32 {
        let len = a.len().min(b.len()).max(1);
        let sum = a
            .iter()
            .copied()
            .zip(b.iter().copied())
            .map(|(a, b)| {
                let diff = a - b;
                diff * diff
            })
            .sum::<f32>();
        (sum / len as f32).sqrt()
    }

    fn max_adjacent_delta(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|window| (window[1] - window[0]).abs())
            .fold(0.0, f32::max)
    }

    struct StaticSampleLibrary {
        path: Option<PathBuf>,
    }

    impl SampleLibrary for StaticSampleLibrary {
        type Error = ();

        fn resolve(&self, reference: &SampleReference) -> Result<SampleResolution, Self::Error> {
            Ok(match &self.path {
                Some(path) => SampleResolution::Found(path.clone()),
                None => SampleResolution::Missing(reference.clone()),
            })
        }

        fn ingest(
            &mut self,
            _path: PathBuf,
        ) -> Result<ahara_sample_library::SampleMetadata, Self::Error> {
            unimplemented!("test library only resolves existing references")
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lindelion-{name}-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_test_wav(path: &Path, samples: &[f32]) {
        let mut file = fs::File::create(path).unwrap();
        let data_len = samples.len() as u32 * 2;
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&48_000u32.to_le_bytes()).unwrap();
        file.write_all(&(48_000u32 * 2).to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();
        for sample in samples {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            file.write_all(&pcm.to_le_bytes()).unwrap();
        }
    }
}
