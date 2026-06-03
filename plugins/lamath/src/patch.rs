use lindelion_audio_expression::{
    AudioExpressionMapping, AudioNoteDetectionConfig, DEFAULT_PITCH_BEND_RANGE_SEMITONES,
};
use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::dsp::constants::{
    FILTER_RESONANCE, MASTER_GAIN_DB, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResonatorSynthPatch {
    pub name: String,
    pub polyphony: u8,
    pub excitation_slots: Vec<ExcitationSlot>,
    pub resonator_a: ModalConfig,
    pub resonator_b: ModalConfig,
    pub routing: ResonatorRouting,
    #[serde(default)]
    pub retrigger_resonators: bool,
    pub output: OutputConfig,
    #[serde(default)]
    pub audio_input: AudioInputConfig,
    #[serde(default)]
    pub audio_expression: AudioExpressionConfig,
    #[serde(default)]
    pub note_detection: AudioNoteDetectionConfig,
    #[serde(default)]
    pub live_excitation: LiveExcitationConfig,
    #[serde(default)]
    pub surrounding: SurroundingConfig,
}

impl Default for ResonatorSynthPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            polyphony: 8,
            excitation_slots: vec![ExcitationSlot::default()],
            resonator_a: ModalConfig::default(),
            resonator_b: ModalConfig {
                preset: ModalPreset::Bell,
                decay_global: 1.5,
                brightness: 0.7,
                ..ModalConfig::default()
            },
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            retrigger_resonators: false,
            output: OutputConfig::default(),
            audio_input: AudioInputConfig::default(),
            audio_expression: AudioExpressionConfig::default(),
            note_detection: AudioNoteDetectionConfig::default(),
            live_excitation: LiveExcitationConfig::default(),
            surrounding: SurroundingConfig::default(),
        }
    }
}

impl ResonatorSynthPatch {
    pub(crate) fn normalize_routing(&mut self) {
        self.routing = sanitize_routing(self.routing);
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
            mapping: AudioExpressionMapping {
                pitch_bend_range_semitones: DEFAULT_PITCH_BEND_RANGE_SEMITONES,
                ..AudioExpressionMapping::default()
            },
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

/// Effort/energy-scaled surrounding effects (M10) — kept intact while Lamath is
/// narrowed back to a dual modal resonator.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SurroundingConfig {
    #[serde(default)]
    pub mechanical_noise: f32,
    #[serde(default)]
    pub radiation_brightness: f32,
    #[serde(default)]
    pub sympathetic: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonatorRouting {
    Parallel { mix_a: f32, mix_b: f32 },
    Series { mix_a: f32, mix_b: f32 },
    BodyColor { mix_a: f32, mix_b: f32 },
}

#[cfg(test)]
pub(crate) fn normalize_routing(routing: ResonatorRouting) -> ResonatorRouting {
    sanitize_routing(routing)
}

fn sanitize_routing(routing: ResonatorRouting) -> ResonatorRouting {
    match routing {
        ResonatorRouting::Parallel { mix_a, mix_b } => ResonatorRouting::Parallel {
            mix_a: sanitize_unit(mix_a, 1.0),
            mix_b: sanitize_unit(mix_b, 0.0),
        },
        ResonatorRouting::Series { mix_a, mix_b } => ResonatorRouting::Series {
            mix_a: sanitize_unit(mix_a, 1.0),
            mix_b: sanitize_unit(mix_b, 1.0),
        },
        ResonatorRouting::BodyColor { mix_a, mix_b } => ResonatorRouting::BodyColor {
            mix_a: sanitize_unit(mix_a, 1.0),
            mix_b: sanitize_unit(mix_b, 1.0),
        },
    }
}

fn sanitize_unit(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        default
    }
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

#[cfg(test)]
#[path = "patch/tests.rs"]
mod tests;
