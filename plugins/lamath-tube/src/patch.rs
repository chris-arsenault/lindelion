use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::processor::ARTICULATION_SLOT_COUNT;

pub const OUTPUT_GAIN_MIN_DB: f32 = -24.0;
pub const OUTPUT_GAIN_MAX_DB: f32 = 12.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TubePatch {
    pub pressure: f32,
    pub reed_stiffness: f32,
    pub embouchure: f32,
    pub reed_aperture_inertia: f32,
    pub body_formant: f32,
    pub body_odd_mode_projection: f32,
    pub body_upper_odd_modes: f32,
    pub humanize: f32,
    /// Note-lifecycle phrasing depth (shared engine; knob law: 0.5 nominal,
    /// 1.0 theatrical, 0 static). Ships at 0 like `humanize`: the wind voice
    /// sits near the reed's oscillation margin, so breath modulation is
    /// opt-in until the voice is re-margined.
    pub phrasing: f32,
    /// Breath-vibrato depth, separate from the rest of the phrasing (knob
    /// law). Wind vibrato is breath modulation, not pitch.
    pub vibrato: f32,
    pub register_break_note: f32,
    pub brightness: f32,
    pub damping: f32,
    pub bell: f32,
    pub bell_radiation_shape: f32,
    pub output_gain_db: f32,
    pub switches: TubeModelSwitchPatch,
    pub selected_articulation: usize,
    pub articulations: [TubeArticulationPatch; ARTICULATION_SLOT_COUNT],
}

impl Default for TubePatch {
    fn default() -> Self {
        Self {
            pressure: 0.58,
            reed_stiffness: 0.48,
            embouchure: 0.52,
            reed_aperture_inertia: 1.0,
            body_formant: 1.0,
            body_odd_mode_projection: 1.0,
            body_upper_odd_modes: 1.0,
            humanize: 0.0,
            phrasing: 0.0,
            vibrato: 0.0,
            register_break_note: 69.0,
            brightness: 0.52,
            damping: 0.28,
            bell: 0.5,
            bell_radiation_shape: 0.0,
            output_gain_db: -8.0,
            switches: TubeModelSwitchPatch::default(),
            selected_articulation: 0,
            articulations: std::array::from_fn(|_| TubeArticulationPatch::default()),
        }
    }
}

impl TubePatch {
    pub fn sanitized(mut self) -> Self {
        let fallback = Self::default();
        self.pressure = unit(self.pressure, fallback.pressure);
        self.reed_stiffness = unit(self.reed_stiffness, fallback.reed_stiffness);
        self.embouchure = unit(self.embouchure, fallback.embouchure);
        self.reed_aperture_inertia =
            unit(self.reed_aperture_inertia, fallback.reed_aperture_inertia);
        self.body_formant = unit(self.body_formant, fallback.body_formant);
        self.body_odd_mode_projection = unit(
            self.body_odd_mode_projection,
            fallback.body_odd_mode_projection,
        );
        self.body_upper_odd_modes = unit(self.body_upper_odd_modes, fallback.body_upper_odd_modes);
        self.humanize = unit(self.humanize, fallback.humanize);
        self.phrasing = unit(self.phrasing, fallback.phrasing);
        self.vibrato = unit(self.vibrato, fallback.vibrato);
        self.register_break_note = if self.register_break_note.is_finite() {
            self.register_break_note.clamp(48.0, 96.0).round()
        } else {
            fallback.register_break_note
        };
        self.brightness = unit(self.brightness, fallback.brightness);
        self.damping = unit(self.damping, fallback.damping);
        self.bell = unit(self.bell, fallback.bell);
        self.bell_radiation_shape = unit(self.bell_radiation_shape, fallback.bell_radiation_shape);
        self.output_gain_db = if self.output_gain_db.is_finite() {
            self.output_gain_db
                .clamp(OUTPUT_GAIN_MIN_DB, OUTPUT_GAIN_MAX_DB)
        } else {
            fallback.output_gain_db
        };
        self.switches.sanitize();
        if self.selected_articulation >= ARTICULATION_SLOT_COUNT {
            self.selected_articulation = fallback.selected_articulation;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TubeModelSwitchPatch {
    pub bell_enabled: bool,
    pub bore_steepening_enabled: bool,
    pub body_enabled: bool,
    pub reed_radiation_enabled: bool,
    pub clarinet_contour_enabled: bool,
}

impl Default for TubeModelSwitchPatch {
    fn default() -> Self {
        Self {
            bell_enabled: true,
            bore_steepening_enabled: true,
            body_enabled: true,
            reed_radiation_enabled: true,
            clarinet_contour_enabled: false,
        }
    }
}

impl TubeModelSwitchPatch {
    fn sanitize(&mut self) {}
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TubeArticulationPatch {
    pub sample: Option<SampleReference>,
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}
