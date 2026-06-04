use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::processor::ARTICULATION_SLOT_COUNT;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TubePatch {
    pub pressure: f32,
    pub reed_stiffness: f32,
    pub embouchure: f32,
    pub reed_aperture_inertia: f32,
    pub brightness: f32,
    pub damping: f32,
    pub bell: f32,
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
            brightness: 0.52,
            damping: 0.28,
            bell: 1.0,
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
        self.brightness = unit(self.brightness, fallback.brightness);
        self.damping = unit(self.damping, fallback.damping);
        self.bell = unit(self.bell, fallback.bell);
        self.output_gain_db = if self.output_gain_db.is_finite() {
            self.output_gain_db.clamp(-24.0, 12.0)
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
}

impl Default for TubeModelSwitchPatch {
    fn default() -> Self {
        Self {
            bell_enabled: true,
            bore_steepening_enabled: true,
            body_enabled: true,
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
