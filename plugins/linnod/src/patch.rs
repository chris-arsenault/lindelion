use lindelion_dsp_utils::math::finite_clamp;
use lindelion_midi::{RootNote, Scale};
use lindelion_onset_detect::{
    DetectionConfig, SliceMarker, SliceRegion, slice_region_at_sample, slice_regions_from_markers,
};
use lindelion_pitch_shift::PitchShiftRatios;
use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

pub use crate::patch_detection::DetectionEdit;

pub const SLICE_COUNT: usize = 16;
pub const FIRST_PAD_MIDI_NOTE: u8 = 36;
pub const DEFAULT_MASTER_GAIN_DB: f32 = 0.0;
pub const MIN_MASTER_GAIN_DB: f32 = -60.0;
pub const MAX_MASTER_GAIN_DB: f32 = 12.0;
pub const DEFAULT_TUNING_REFERENCE_HZ: f32 = 440.0;
pub const MIN_TUNING_REFERENCE_HZ: f32 = 400.0;
pub const MAX_TUNING_REFERENCE_HZ: f32 = 480.0;
pub const MIN_SLICE_GAIN_DB: f32 = -60.0;
pub const MAX_SLICE_GAIN_DB: f32 = 24.0;
pub const DEFAULT_FILTER_CUTOFF_HZ: f32 = 20_000.0;
pub const MIN_FILTER_CUTOFF_HZ: f32 = 20.0;
pub const MAX_FILTER_CUTOFF_HZ: f32 = 20_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinnodPatch {
    pub name: String,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub playback: PlaybackConfig,
    #[serde(default)]
    pub source_sample: Option<SampleReference>,
    #[serde(default)]
    pub detection: DetectionConfig,
    #[serde(default)]
    pub markers: Vec<SliceMarker>,
    #[serde(default = "default_slices")]
    pub slices: Vec<SliceParams>,
    #[serde(default)]
    pub tuning: TuningConfig,
    #[serde(default)]
    pub trigger_mode: TriggerMode,
    #[serde(default)]
    pub active_chromatic_pad: PadId,
    #[serde(default = "default_pad_assignments")]
    pub pad_map: Vec<PadAssignment>,
}

impl Default for LinnodPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            output: OutputConfig::default(),
            playback: PlaybackConfig::default(),
            source_sample: None,
            detection: DetectionConfig::default(),
            markers: Vec::new(),
            slices: default_slices(),
            tuning: TuningConfig::default(),
            trigger_mode: TriggerMode::Pad,
            active_chromatic_pad: PadId::default(),
            pad_map: default_pad_assignments(),
        }
    }
}

impl LinnodPatch {
    pub fn slice(&self, index: usize) -> Option<&SliceParams> {
        self.slices.get(index)
    }

    pub fn slice_mut(&mut self, index: usize) -> Option<&mut SliceParams> {
        self.slices.get_mut(index)
    }

    pub fn apply_slice_edit(&mut self, index: usize, edit: SliceEdit) -> bool {
        let Some(slice) = self.slice_mut(index) else {
            return false;
        };
        slice.apply_edit(edit);
        true
    }

    pub fn apply_detection_edit(&mut self, edit: DetectionEdit) -> bool {
        self.detection = crate::patch_detection::detection_config_after_edit(self.detection, edit);
        true
    }

    pub fn apply_playback_edit(&mut self, edit: PlaybackEdit) -> bool {
        self.playback.apply_edit(edit);
        true
    }

    pub fn effective_playback_config(&self, slice_index: usize) -> PlaybackConfig {
        let global = self.playback.sanitized();
        self.slice(slice_index)
            .map(|slice| slice.effective_playback_config(global))
            .unwrap_or(global)
    }

    pub fn normalize_layout(&mut self) {
        self.playback = self.playback.sanitized();
        if self.slices.len() < SLICE_COUNT {
            let start = self.slices.len() + 1;
            self.slices
                .extend((start..=SLICE_COUNT).map(SliceParams::default_for_index));
        }
        self.slices.truncate(SLICE_COUNT);
        self.pad_map = normalize_pad_assignments(&self.pad_map);
        self.active_chromatic_pad = self.active_chromatic_pad.sanitized();
    }

    pub fn selected_slice_index(&self) -> Option<usize> {
        slice_index_for_pad(&self.pad_map, self.active_chromatic_pad)
    }

    pub fn slice_region(&self, index: usize, source_len: usize) -> Option<SliceRegion> {
        slice_regions_from_markers(&self.markers, source_len)
            .into_iter()
            .find(|region| region.index == index)
    }

    pub fn slice_index_at_source_sample(
        &self,
        source_len: usize,
        position_samples: usize,
    ) -> Option<usize> {
        slice_region_at_sample(&self.markers, source_len, position_samples)
            .map(|region| region.index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub master_gain_db: f32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            master_gain_db: DEFAULT_MASTER_GAIN_DB,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadAssignment {
    pub pad: PadId,
    pub slice_index: usize,
    pub midi_note: u8,
    #[serde(default)]
    pub choke_group: Option<ChokeGroupId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PadId(pub u8);

impl PadId {
    pub const fn new(index: u8) -> Option<Self> {
        if index >= 1 && index <= SLICE_COUNT as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn index(self) -> usize {
        self.0.saturating_sub(1) as usize
    }

    pub const fn sanitized(self) -> Self {
        if self.0 < 1 {
            Self(1)
        } else if self.0 > SLICE_COUNT as u8 {
            Self(SLICE_COUNT as u8)
        } else {
            self
        }
    }
}

impl Default for PadId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChokeGroupId(pub u8);

impl ChokeGroupId {
    pub const fn new(index: u8) -> Option<Self> {
        if index >= 1 && index <= SLICE_COUNT as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn sanitized(self) -> Self {
        if self.0 < 1 {
            Self(1)
        } else if self.0 > SLICE_COUNT as u8 {
            Self(SLICE_COUNT as u8)
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    #[default]
    Pad,
    Chromatic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceParams {
    pub name: String,
    pub start_offset_ms: f32,
    pub end_offset_ms: f32,
    pub pitch: PitchOffset,
    pub gain_db: f32,
    pub pan: f32,
    pub reverse: bool,
    #[serde(default = "loaded_slice_uses_playback_override")]
    pub use_playback_override: bool,
    pub playback_mode: PlaybackMode,
    pub envelope: EnvelopeConfig,
    pub filter_cutoff: f32,
}

impl SliceParams {
    pub fn default_for_index(index: usize) -> Self {
        Self {
            name: format!("Slice {index}"),
            start_offset_ms: 0.0,
            end_offset_ms: 0.0,
            pitch: PitchOffset::default(),
            gain_db: 0.0,
            pan: 0.0,
            reverse: false,
            use_playback_override: false,
            playback_mode: PlaybackMode::OneShot,
            envelope: EnvelopeConfig::default(),
            filter_cutoff: DEFAULT_FILTER_CUTOFF_HZ,
        }
    }

    pub fn apply_edit(&mut self, edit: SliceEdit) {
        match edit {
            SliceEdit::Name(name) => self.name = name,
            SliceEdit::Offsets {
                start_offset_ms,
                end_offset_ms,
            } => {
                self.start_offset_ms = sanitize_non_negative_ms(start_offset_ms);
                self.end_offset_ms = sanitize_non_negative_ms(end_offset_ms);
            }
            SliceEdit::Pitch(pitch) => self.pitch = pitch.sanitized(),
            SliceEdit::GainDb(gain_db) => {
                self.gain_db = finite_clamp(gain_db, MIN_SLICE_GAIN_DB, MAX_SLICE_GAIN_DB, 0.0);
            }
            SliceEdit::Pan(pan) => self.pan = finite_clamp(pan, -1.0, 1.0, 0.0),
            SliceEdit::Reverse(reverse) => self.reverse = reverse,
            SliceEdit::PlaybackOverride(enabled) => self.use_playback_override = enabled,
            SliceEdit::PlaybackMode(playback_mode) => self.playback_mode = playback_mode,
            SliceEdit::Envelope(envelope) => self.envelope = envelope.sanitized(),
            SliceEdit::FilterCutoff(cutoff) => {
                self.filter_cutoff = finite_clamp(
                    cutoff,
                    MIN_FILTER_CUTOFF_HZ,
                    MAX_FILTER_CUTOFF_HZ,
                    DEFAULT_FILTER_CUTOFF_HZ,
                );
            }
        }
    }

    pub fn effective_playback_config(&self, global: PlaybackConfig) -> PlaybackConfig {
        if self.use_playback_override {
            PlaybackConfig {
                mode: self.playback_mode,
                envelope: self.envelope,
            }
            .sanitized()
        } else {
            global
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SliceEdit {
    Name(String),
    Offsets {
        start_offset_ms: f32,
        end_offset_ms: f32,
    },
    Pitch(PitchOffset),
    GainDb(f32),
    Pan(f32),
    Reverse(bool),
    PlaybackOverride(bool),
    PlaybackMode(PlaybackMode),
    Envelope(EnvelopeConfig),
    FilterCutoff(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackEdit {
    Mode(PlaybackMode),
    Envelope(EnvelopeConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadEdit {
    ChokeGroup(Option<ChokeGroupId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitchOffset {
    pub semitones: i32,
    pub cents: f32,
}

impl PitchOffset {
    pub fn ratio(self) -> f32 {
        PitchShiftRatios::from_semitones_cents(self.semitones as f32, self.cents).pitch_ratio
    }

    pub fn from_frequency_ratio(ratio: f32) -> Self {
        if ratio <= 0.0 || !ratio.is_finite() {
            return Self::default();
        }
        let total_cents = 1200.0 * ratio.log2();
        let semitones = (total_cents / 100.0).round() as i32;
        let cents = total_cents - semitones as f32 * 100.0;
        Self { semitones, cents }.sanitized()
    }

    pub fn sanitized(self) -> Self {
        Self {
            semitones: self.semitones.clamp(-48, 48),
            cents: finite_clamp(self.cents, -100.0, 100.0, 0.0),
        }
    }
}

impl Default for PitchOffset {
    fn default() -> Self {
        Self {
            semitones: 0,
            cents: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackMode {
    #[default]
    OneShot,
    Gated,
    Looped,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlaybackConfig {
    #[serde(default)]
    pub mode: PlaybackMode,
    #[serde(default)]
    pub envelope: EnvelopeConfig,
}

impl PlaybackConfig {
    pub fn sanitized(self) -> Self {
        Self {
            mode: self.mode,
            envelope: self.envelope.sanitized(),
        }
    }

    pub fn apply_edit(&mut self, edit: PlaybackEdit) {
        match edit {
            PlaybackEdit::Mode(mode) => self.mode = mode,
            PlaybackEdit::Envelope(envelope) => self.envelope = envelope.sanitized(),
        }
    }
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            mode: PlaybackMode::OneShot,
            envelope: EnvelopeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeConfig {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl EnvelopeConfig {
    pub fn sanitized(self) -> Self {
        Self {
            attack_ms: sanitize_non_negative_ms(self.attack_ms),
            decay_ms: sanitize_non_negative_ms(self.decay_ms),
            sustain: finite_clamp(self.sustain, 0.0, 1.0, 1.0),
            release_ms: sanitize_non_negative_ms(self.release_ms),
        }
    }
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuningConfig {
    pub reference_hz: f32,
    pub scale: Scale,
    pub root: RootNote,
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            reference_hz: DEFAULT_TUNING_REFERENCE_HZ,
            scale: Scale::Chromatic,
            root: RootNote::A,
        }
    }
}

fn default_slices() -> Vec<SliceParams> {
    (1..=SLICE_COUNT)
        .map(SliceParams::default_for_index)
        .collect()
}

fn loaded_slice_uses_playback_override() -> bool {
    true
}

pub fn default_pad_assignments() -> Vec<PadAssignment> {
    (1..=SLICE_COUNT)
        .map(|index| PadAssignment {
            pad: PadId(index as u8),
            slice_index: index - 1,
            midi_note: FIRST_PAD_MIDI_NOTE + index as u8 - 1,
            choke_group: None,
        })
        .collect()
}

pub fn normalize_pad_assignments(input: &[PadAssignment]) -> Vec<PadAssignment> {
    let mut pad_map = default_pad_assignments();
    for assignment in input {
        let pad = assignment.pad.sanitized();
        let index = pad.index();
        pad_map[index] = PadAssignment {
            pad,
            slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
            midi_note: assignment.midi_note,
            choke_group: assignment.choke_group.map(ChokeGroupId::sanitized),
        };
    }
    pad_map
}

pub fn pad_assignment_for_note(pad_map: &[PadAssignment], note: u8) -> Option<PadAssignment> {
    pad_map
        .iter()
        .find(|assignment| assignment.midi_note == note)
        .map(normalize_pad_assignment)
}

pub fn slice_index_for_pad(pad_map: &[PadAssignment], pad: PadId) -> Option<usize> {
    let pad = pad.sanitized();
    pad_map
        .iter()
        .find(|assignment| assignment.pad.sanitized() == pad)
        .map(|assignment| assignment.slice_index.min(SLICE_COUNT - 1))
        .or_else(|| {
            default_pad_assignments()
                .get(pad.index())
                .map(|assignment| assignment.slice_index)
        })
}

impl LinnodPatch {
    pub fn apply_pad_edit(&mut self, pad: PadId, edit: PadEdit) -> bool {
        let pad = pad.sanitized();
        self.normalize_layout();
        let Some(assignment) = self
            .pad_map
            .iter_mut()
            .find(|assignment| assignment.pad.sanitized() == pad)
        else {
            return false;
        };
        match edit {
            PadEdit::ChokeGroup(group) => {
                assignment.choke_group = group.map(ChokeGroupId::sanitized);
            }
        }
        true
    }
}

fn normalize_pad_assignment(assignment: &PadAssignment) -> PadAssignment {
    PadAssignment {
        pad: assignment.pad.sanitized(),
        slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
        midi_note: assignment.midi_note,
        choke_group: assignment.choke_group.map(ChokeGroupId::sanitized),
    }
}

fn sanitize_non_negative_ms(value: f32) -> f32 {
    finite_clamp(value, 0.0, 60_000.0, 0.0)
}

#[cfg(test)]
#[path = "patch_tests.rs"]
mod tests;
