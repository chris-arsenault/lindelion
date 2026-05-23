use lindelion_onset_detect::{AlgorithmParams, DetectionAlgorithm, DetectionConfig, SliceMarker};
use lindelion_plugin_shell::{
    AudioPlugin, ParameterInfo, ParameterRange, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup,
};
use lindelion_psola::{PitchAnalysis, PitchShift};
use lindelion_sample_library::SampleReference;
use lindelion_ui::{PadId, TriggerMode};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Linnod", *b"lindelion_linnod");

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::continuous(
        1,
        "Master Gain",
        "dB",
        ParameterRange::linear(-60.0, 12.0, 0.0),
    ),
    ParameterInfo::continuous(
        2,
        "Detection Sensitivity",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ),
    ParameterInfo::continuous(
        3,
        "Tuning Reference",
        "Hz",
        ParameterRange::linear(400.0, 480.0, 440.0),
    ),
];

#[derive(Debug, Clone, PartialEq)]
pub struct LinnodPatch {
    pub name: String,
    pub source_sample: Option<SampleReference>,
    pub detection: DetectionConfig,
    pub markers: Vec<SliceMarker>,
    pub slices: Vec<SliceParams>,
    pub tuning: TuningConfig,
    pub trigger_mode: TriggerMode,
    pub active_chromatic_pad: PadId,
    pub pad_map: Vec<PadAssignment>,
}

impl Default for LinnodPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            source_sample: None,
            detection: DetectionConfig {
                algorithm: DetectionAlgorithm::SuperFlux,
                sensitivity: 0.5,
                min_slice_ms: 50.0,
                params: AlgorithmParams::SuperFlux {
                    lookback_frames: 3,
                    max_filter_radius: 3,
                },
            },
            markers: Vec::new(),
            slices: (1..=16).map(SliceParams::default_for_index).collect(),
            tuning: TuningConfig::default(),
            trigger_mode: TriggerMode::Pad,
            active_chromatic_pad: PadId(1),
            pad_map: (1..=16)
                .map(|pad| PadAssignment {
                    pad: PadId(pad),
                    slice_index: (pad - 1) as usize,
                    midi_note: 35 + pad,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PadAssignment {
    pub pad: PadId,
    pub slice_index: usize,
    pub midi_note: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SliceParams {
    pub name: String,
    pub start_offset_ms: f32,
    pub end_offset_ms: f32,
    pub pitch: PitchShift,
    pub gain_db: f32,
    pub pan: f32,
    pub reverse: bool,
    pub playback_mode: PlaybackMode,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub filter_cutoff: f32,
    pub analysis: PitchAnalysis,
}

impl SliceParams {
    pub fn default_for_index(index: u8) -> Self {
        Self {
            name: format!("Slice {index}"),
            start_offset_ms: 0.0,
            end_offset_ms: 0.0,
            pitch: PitchShift {
                semitones: 0,
                cents: 0.0,
            },
            gain_db: 0.0,
            pan: 0.0,
            reverse: false,
            playback_mode: PlaybackMode::OneShot,
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 50.0,
            filter_cutoff: 20_000.0,
            analysis: PitchAnalysis::empty(48_000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackMode {
    OneShot,
    Gated,
    Looped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TuningConfig {
    pub reference_hz: f32,
    pub scale: Scale,
    pub root: NoteName,
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            reference_hz: 440.0,
            scale: Scale::Chromatic,
            root: NoteName::A,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scale {
    Chromatic,
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    PentatonicMajor,
    PentatonicMinor,
    Blues,
    Custom(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteName {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

#[derive(Debug, Default)]
pub struct Linnod {
    setup: ProcessSetup,
    patch: LinnodPatch,
}

impl Linnod {
    pub fn patch(&self) -> &LinnodPatch {
        &self.patch
    }
}

impl AudioPlugin for Linnod {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
    }

    fn process(&mut self, mut context: ProcessContext<'_>) {
        let _ = self.setup;
        context.buffer.clear();
    }

    fn state(&self) -> PluginState {
        PluginState::empty(1)
    }

    fn load_state(&mut self, _state: PluginState) {}
}
