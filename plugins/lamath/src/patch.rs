use lindelion_audio_expression::{
    AudioExpressionMapping, AudioNoteDetectionConfig, DEFAULT_PITCH_BEND_RANGE_SEMITONES,
};
use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::dsp::{
    WaveguideStyle,
    constants::{
        FILTER_RESONANCE, MASTER_GAIN_DB, OUTPUT_FILTER_CUTOFF_HZ, STRIKE_POSITION, TUBE_BOUNDARY,
        WAVEGUIDE_DISPERSION, WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ, WAVEGUIDE_LOOP_GAIN,
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
    #[serde(default)]
    pub driver: DriverConfig,
    #[serde(default)]
    pub contact: ContactConfig,
    #[serde(default)]
    pub surrounding: SurroundingConfig,
    #[serde(default)]
    pub shared_body: SharedBodyConfig,
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
            driver: DriverConfig::default(),
            contact: ContactConfig::default(),
            surrounding: SurroundingConfig::default(),
            shared_body: SharedBodyConfig::default(),
        }
    }
}

impl ResonatorSynthPatch {
    pub(crate) fn normalize_routing_for_resonator_models(&mut self) {
        self.routing = normalize_routing_for_resonator_models(
            self.routing,
            self.resonator_a,
            self.resonator_b,
        );
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioExpressionConfig {
    pub enabled: bool,
    pub mapping: AudioExpressionMapping,
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
    Mesh(MeshConfig),
}

impl ResonatorConfig {
    /// The idiophone resonator families (struck bodies) the shared body mirrors:
    /// `Modal` and `Mesh`. `Waveguide` (String/Tube) is not an idiophone and the
    /// shared-body toggle no-ops for it (ADR-0031, decision 2).
    pub fn is_idiophone(&self) -> bool {
        matches!(self, ResonatorConfig::Modal(_) | ResonatorConfig::Mesh(_))
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
    #[serde(default = "default_waveguide_dispersion")]
    pub dispersion: f32,
    pub position_of_strike: f32,
    #[serde(default = "default_boundary_reflection")]
    pub boundary_reflection: f32,
    /// Energy-dependent source↔body balance depth `0..1` for the String output (M9; soft→
    /// warm, loud→bright per M11 P8). String only; defaults 0.5 (P10), alive out of the box.
    #[serde(default = "default_source_body_balance")]
    pub source_body_balance: f32,
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
            dispersion: default_waveguide_dispersion(),
            position_of_strike: STRIKE_POSITION.default,
            boundary_reflection: default_boundary_reflection(),
            source_body_balance: default_source_body_balance(),
        }
    }
}

/// Physical controls for the 2D-mesh (plate/membrane) resonator model. Every
/// control except the pitch offsets is normalised `0..1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeshConfig {
    pub semitone_offset: i8,
    pub cent_offset: f32,
    pub material: f32,
    pub size: f32,
    pub damping: f32,
    pub tension: f32,
    pub position_of_strike: f32,
    pub pickup_spread: f32,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            semitone_offset: 0,
            cent_offset: 0.0,
            material: 0.5,
            size: 0.5,
            damping: 0.3,
            tension: 0.5,
            position_of_strike: STRIKE_POSITION.default,
            pickup_spread: 0.3,
        }
    }
}

pub(crate) const fn default_waveguide_dispersion() -> f32 {
    WAVEGUIDE_DISPERSION.default
}

pub(crate) const fn default_boundary_reflection() -> f32 {
    TUBE_BOUNDARY.reflection.default
}

/// M11 P10 factory source↔body balance depth — non-zero so a default String is alive.
pub(crate) const fn default_source_body_balance() -> f32 {
    0.5
}

// Physical driver / contact configs (M8/M9) live in a separate file to keep this one
// under the repository file-size limit; `include!` keeps them in this module verbatim.
include!("patch/driver_configs.rs");

/// Effort/energy-scaled surrounding effects (M10) — the last "surrounding" link of
/// the dynamic-response chain (ADR-0014). Each depth is `0..1` and defaults to `0`
/// (defeated), so a default patch is unchanged. `mechanical_noise` and
/// `radiation_brightness` are per-voice; `sympathetic` drives a shared, global
/// sympathetic-resonance bank excited by the whole mix.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SurroundingConfig {
    /// Pick/breath mechanical-noise depth `0..1`: a force-shaped attack noise burst,
    /// scaled by playing effort. `0` is silent.
    #[serde(default)]
    pub mechanical_noise: f32,
    /// Radiation-brightening depth `0..1`: an energy-scaled high-shelf, so a more
    /// energetically-sounding note radiates brighter. `0` is a flat (0 dB) shelf.
    #[serde(default)]
    pub radiation_brightness: f32,
    /// Sympathetic-resonance depth `0..1` for the shared global bank excited by the
    /// mix. `0` adds no sympathetic ringing.
    #[serde(default)]
    pub sympathetic: f32,
}

/// Shared-body idiophone mode: when enabled, note-ons re-strike a single
/// runtime-owned persistent body instead of allocating per-note voices
/// ([ADR-0031](../../docs/adr/0031-shared-body-idiophone-mode.md)). This struct is
/// the control surface only; no runtime DSP reads it yet (M0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SharedBodyConfig {
    /// `false` (default) keeps today's per-voice idiophone behavior, bit-identical.
    /// `true` promotes the idiophone resonator to the shared struck body.
    #[serde(default)]
    pub enabled: bool,
    /// Lowest MIDI note (inclusive) of the damp/choke key-switch range. Notes in
    /// `[damp_key_low, damp_key_high]` damp the body instead of striking it.
    #[serde(default)]
    pub damp_key_low: u8,
    /// Highest MIDI note (inclusive) of the damp/choke key-switch range.
    #[serde(default = "default_damp_key_high")]
    pub damp_key_high: u8,
}

fn default_damp_key_high() -> u8 {
    11
}

impl Default for SharedBodyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            damp_key_low: 0,
            damp_key_high: default_damp_key_high(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonatorRouting {
    Parallel { mix_a: f32, mix_b: f32 },
    Series { mix_a: f32, mix_b: f32 },
    BodyColor { mix_a: f32, mix_b: f32 },
}

pub(crate) fn normalize_routing_for_resonator_models(
    routing: ResonatorRouting,
    resonator_a: ResonatorConfig,
    resonator_b: ResonatorConfig,
) -> ResonatorRouting {
    match (routing, resonator_a, resonator_b) {
        (
            ResonatorRouting::Series { mix_a, mix_b },
            ResonatorConfig::Modal(_),
            ResonatorConfig::Modal(_),
        ) => ResonatorRouting::BodyColor { mix_a, mix_b },
        (routing, _, _) => routing,
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

#[cfg(test)]
#[path = "patch/tests.rs"]
mod tests;
