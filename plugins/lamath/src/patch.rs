use lindelion_audio_expression::{
    AudioExpressionMapping, AudioNoteDetectionConfig, DEFAULT_PITCH_BEND_RANGE_SEMITONES,
};
use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::dsp::{
    WaveguideStyle,
    constants::{
        FILTER_RESONANCE, MASTER_GAIN_DB, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION, TUBE_BOUNDARY,
        WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ, WAVEGUIDE_LOOP_GAIN,
    },
};

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
    #[serde(default)]
    pub audio_input: AudioInputConfig,
    #[serde(default)]
    pub audio_expression: AudioExpressionConfig,
    #[serde(default)]
    pub note_detection: AudioNoteDetectionConfig,
    #[serde(default)]
    pub live_excitation: LiveExcitationConfig,
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
            audio_input: AudioInputConfig::default(),
            audio_expression: AudioExpressionConfig::default(),
            note_detection: AudioNoteDetectionConfig::default(),
            live_excitation: LiveExcitationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioInputConfig {
    pub mode: AudioInputMode,
}

impl Default for AudioInputConfig {
    fn default() -> Self {
        Self {
            mode: AudioInputMode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioInputMode {
    Off,
    AudioCreatesNotes,
    MidiPlusAudioCreatesNotes,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioExpressionConfig {
    pub enabled: bool,
    pub mapping: AudioExpressionMapping,
}

impl Default for AudioExpressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mapping: AudioExpressionMapping::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LiveExcitationConfig {
    pub mode: LiveExcitationMode,
    pub gain_db: f32,
    pub latch_window_ms: f32,
    pub latch_pre_roll_ms: f32,
    pub latch_fade_ms: f32,
}

impl Default for LiveExcitationConfig {
    fn default() -> Self {
        Self {
            mode: LiveExcitationMode::Off,
            gain_db: 0.0,
            latch_window_ms: 120.0,
            latch_pre_roll_ms: 20.0,
            latch_fade_ms: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveExcitationMode {
    Off,
    Continuous,
    NoteLatched,
    ContinuousAndNoteLatched,
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
            position_of_strike: STRIKE_POSITION.default,
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
            loop_filter_cutoff: WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ.default,
            loop_filter_resonance: FILTER_RESONANCE.default,
            loop_gain: WAVEGUIDE_LOOP_GAIN.default,
            loop_nonlinearity: 0.0,
            position_of_strike: STRIKE_POSITION.default,
            boundary_reflection: default_boundary_reflection(),
        }
    }
}

pub(crate) const fn default_boundary_reflection() -> f32 {
    TUBE_BOUNDARY.reflection.default
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
            filter_cutoff: OUTPUT_FILTER_CUTOFF_HZ.default,
            filter_resonance: FILTER_RESONANCE.default,
            saturation_drive: 0.0,
            master_gain_db: MASTER_GAIN_DB.default,
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

impl From<EnvelopeConfig> for lindelion_dsp_utils::envelope::Adsr {
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
            pitch_bend_range_semitones: DEFAULT_PITCH_BEND_RANGE_SEMITONES,
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
