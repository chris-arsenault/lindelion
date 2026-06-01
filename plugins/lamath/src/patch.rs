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
    /// Energy-dependent source↔body balance depth `0..1` for the String output (M9).
    /// `0` reproduces the pre-M9 fixed pickup/body blend; higher leans the mix to the
    /// warm body at low dynamics and the direct pickup at high. String only.
    #[serde(default)]
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
            source_body_balance: 0.0,
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

/// Selectable physical driver feeding the waveguide resonator (M8, ADR-0017). The
/// default `Sample` driver is a transparent pass-through of the existing sample /
/// sidechain excitation, so a patch without a driver behaves exactly as before.
/// `Pick` is a feed-forward contact transient; `Reed` is a self-oscillating wind
/// driver coupled two-way to the bore. Each variant's controls are normalised `0..1`
/// and mapped to their physical range inside the driver DSP (M11 calibrates ranges).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum DriverConfig {
    #[default]
    Sample,
    Pick(PickConfig),
    Reed(ReedConfig),
    Bow(BowConfig),
}

/// Pick/hammer contact driver: a force-shaped contact transient that brightens with
/// playing effort. The strike location stays the waveguide's own strike-position
/// control; this shapes the contact itself.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PickConfig {
    /// Contact hardness `0..1`: softer rounds the contact (darker), harder sharpens it.
    pub hardness: f32,
    /// Contact time `0..1`: longer spreads the contact pulse for a mellower attack.
    pub contact_time: f32,
}

impl Default for PickConfig {
    fn default() -> Self {
        Self {
            hardness: 0.5,
            contact_time: 0.5,
        }
    }
}

/// Reed/lip pressure-flow driver: a self-oscillating wind driver coupled two-way to
/// the bore. Mouth pressure is mapped from the effort bus; below a pressure threshold
/// the reed is quiescent, above it the bore self-oscillates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReedConfig {
    /// How much playing effort drives mouth pressure `0..1`.
    pub pressure_depth: f32,
    /// Reed stiffness `0..1`: sets the reed's natural cutoff (brighter when stiffer).
    pub stiffness: f32,
    /// Embouchure `0..1`: the reed's rest opening / bias toward the closing regime.
    pub embouchure: f32,
}

impl Default for ReedConfig {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            stiffness: 0.5,
            embouchure: 0.5,
        }
    }
}

/// Bow friction driver: a continuous stick-slip friction excitation coupled to the
/// string's velocity at the contact, so a held note sustains a bowed (Helmholtz)
/// tone. Mouth-pressure has no analogue here — the player effort sets the bow normal
/// force; below enough force the string is barely driven, above it a stable limit
/// cycle builds. (Ranges are calibrated in M11 P10.)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BowConfig {
    /// How much playing effort drives the bow normal force `0..1` (heavier = louder,
    /// brighter, more locked-in).
    pub pressure_depth: f32,
    /// Bow speed `0..1`: the bow's velocity magnitude, setting the limit-cycle
    /// amplitude and brightness (faster = brighter/louder).
    pub bow_speed: f32,
    /// Friction sharpness `0..1`: the stick-slip transition width. Smoother is a
    /// pure sustained tone; sharper is a scratchier, more articulate attack.
    pub friction: f32,
}

impl Default for BowConfig {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            bow_speed: 0.5,
            friction: 0.5,
        }
    }
}

/// Coupling/contact stage between the driver and the resonator (M9), shaping a strike
/// into a pick (tight) or a strum (spread). Both controls default to the transparent
/// pre-M9 values, so a default patch is unchanged; waveguide String/Tube path only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContactConfig {
    /// Excitation spread `0..1`: `0` is a tight pick (the narrow pre-M9 contact), `1`
    /// is a wide strum. Playing effort widens it further (the gesture half of M9).
    pub spread: f32,
    /// Contact time `0..1`: `0` is an instant contact (sharp onset, transparent),
    /// higher spreads the contact in time for a mellower, darker attack.
    pub contact_time: f32,
}

impl Default for ContactConfig {
    fn default() -> Self {
        Self {
            spread: 0.0,
            contact_time: 0.0,
        }
    }
}

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
