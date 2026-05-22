pub mod dsp;
pub mod patch_io;
pub mod runtime;
mod vst3_entry;

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

use ahara_plugin_shell::{
    AudioPlugin, ParameterInfo, ParameterRange, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup,
};
use ahara_sample_library::SampleReference;
use runtime::ResonatorProcessor;
use serde::{Deserialize, Serialize};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Ahara Resonator Synth", *b"ahara_resonator!");

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::continuous(
        1,
        "Master Gain",
        "dB",
        ParameterRange::linear(-60.0, 12.0, 0.0),
    ),
    ParameterInfo::continuous(2, "Loop Gain", "", ParameterRange::linear(0.0, 0.999, 0.92)),
    ParameterInfo::continuous(
        3,
        "Filter Cutoff",
        "Hz",
        ParameterRange::linear(20.0, 20_000.0, 20_000.0),
    ),
];

const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResonatorSynthPatch {
    pub name: String,
    pub polyphony: u8,
    pub excitation_slots: Vec<ExcitationSlot>,
    pub resonator_a: ResonatorConfig,
    pub resonator_b: ResonatorConfig,
    pub routing: ResonatorRouting,
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
                mix_a: 0.5,
                mix_b: 0.5,
            },
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
    pub semitone_offset: i8,
    pub cent_offset: f32,
    pub loop_filter_cutoff: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub position_of_strike: f32,
}

impl Default for WaveguideConfig {
    fn default() -> Self {
        Self {
            semitone_offset: 0,
            cent_offset: 0.0,
            loop_filter_cutoff: 8_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.92,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonatorRouting {
    Parallel { mix_a: f32, mix_b: f32 },
    Series,
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
}

impl Default for ResonatorSynth {
    fn default() -> Self {
        let patch = ResonatorSynthPatch::default();
        Self {
            setup: ProcessSetup::default(),
            processor: ResonatorProcessor::with_builtin_excitation(48_000.0, patch.clone()),
            patch,
        }
    }
}

impl ResonatorSynth {
    pub fn patch(&self) -> &ResonatorSynthPatch {
        &self.patch
    }

    pub fn set_parameter_normalized(&mut self, id: ahara_plugin_shell::ParameterId, value: f32) {
        let Some(parameter) = PARAMETERS.iter().find(|parameter| parameter.id == id) else {
            return;
        };
        let plain = parameter.range.denormalize(value);

        match id.0 {
            1 => self.patch.output.master_gain_db = plain,
            2 => set_patch_loop_gain(&mut self.patch, plain),
            3 => self.patch.output.filter_cutoff = plain,
            _ => {}
        }
        self.processor.set_parameter_plain(id, plain);
    }

    #[cfg(test)]
    pub(crate) fn set_patch_for_test(&mut self, patch: ResonatorSynthPatch) {
        self.patch = patch;
        self.processor = ResonatorProcessor::with_builtin_excitation(
            self.setup.sample_rate as f32,
            self.patch.clone(),
        );
    }
}

fn set_patch_loop_gain(patch: &mut ResonatorSynthPatch, loop_gain: f32) {
    if let ResonatorConfig::Waveguide(mut config) = patch.resonator_a {
        config.loop_gain = loop_gain;
        patch.resonator_a = ResonatorConfig::Waveguide(config);
    }
    if let ResonatorConfig::Waveguide(mut config) = patch.resonator_b {
        config.loop_gain = loop_gain;
        patch.resonator_b = ResonatorConfig::Waveguide(config);
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
        self.processor = ResonatorProcessor::with_builtin_excitation(
            setup.sample_rate as f32,
            self.patch.clone(),
        );
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        self.processor
            .process(context.events, context.buffer.left, context.buffer.right);
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
            self.patch = patch;
            self.processor = ResonatorProcessor::with_builtin_excitation(
                self.setup.sample_rate as f32,
                self.patch.clone(),
            );
        }
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    use ahara_dsp_utils::analysis::{assert_all_finite, rms};
    use ahara_plugin_shell::{AudioBuffer, MidiEvent, NoteEvent, ProcessMode};

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
}
