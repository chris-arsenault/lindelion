use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

use crate::processor::ARTICULATION_SLOT_COUNT;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StringPatch {
    pub brightness: f32,
    pub damping: f32,
    pub stiffness: f32,
    pub strike_position: f32,
    pub pickup_position: f32,
    pub body_balance: f32,
    pub output_gain_db: f32,
    pub bow_position: f32,
    pub bow_pressure: f32,
    pub bow_speed: f32,
    pub bow_friction: f32,
    /// One knob scaling the linked physical humanization axes (see
    /// `processor::StringSteadyVariance`). Knob law: 0.5 is the nominal,
    /// intended humanization (the shipped default — a string in human hands
    /// is never machine-still); 1.0 approaches the unmusical (Schelleng axes
    /// brush the crush boundary, intonation wanders to +/-8 cents).
    pub humanize: f32,
    /// One knob for the note-lifecycle phrasing engine (see
    /// `processor`): attack development, sustain swell, delayed-onset
    /// vibrato, and the reactive release taper. Knob law: 0.5 is the nominal
    /// musical phrasing (the shipped default); 1.0 approaches the theatrical;
    /// 0 is the static, lifecycle-free instrument.
    pub phrasing: f32,
    /// Vibrato depth, separated from the rest of the phrasing so the note
    /// shape can be auditioned and played without pitch motion. Knob law:
    /// 0.5 nominal, 1.0 theatrical, 0 none.
    pub vibrato: f32,
    pub driver: DriverSelection,
    pub body: BodySelection,
    pub switches: ModelSwitches,
    pub selected_articulation: usize,
    pub articulations: [ArticulationSlot; ARTICULATION_SLOT_COUNT],
}

impl Default for StringPatch {
    fn default() -> Self {
        Self {
            brightness: 0.62,
            damping: 0.34,
            stiffness: 0.72,
            strike_position: 0.36,
            pickup_position: 0.82,
            body_balance: 0.38,
            output_gain_db: -8.0,
            bow_position: 0.12,
            bow_pressure: 0.48,
            bow_speed: 0.45,
            bow_friction: 0.45,
            humanize: 0.5,
            phrasing: 0.5,
            vibrato: 0.5,
            driver: DriverSelection::Pick,
            body: BodySelection::Guitar,
            switches: ModelSwitches::default(),
            selected_articulation: 0,
            articulations: std::array::from_fn(|_| ArticulationSlot::default()),
        }
    }
}

impl StringPatch {
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        self.brightness = unit(self.brightness, defaults.brightness);
        self.damping = unit(self.damping, defaults.damping);
        self.stiffness = unit(self.stiffness, defaults.stiffness);
        self.strike_position = unit(self.strike_position, defaults.strike_position);
        self.pickup_position = unit(self.pickup_position, defaults.pickup_position);
        self.body_balance = unit(self.body_balance, defaults.body_balance);
        self.bow_position = unit(self.bow_position, defaults.bow_position);
        self.bow_pressure = unit(self.bow_pressure, defaults.bow_pressure);
        self.bow_speed = unit(self.bow_speed, defaults.bow_speed);
        self.bow_friction = unit(self.bow_friction, defaults.bow_friction);
        self.humanize = unit(self.humanize, defaults.humanize);
        self.phrasing = unit(self.phrasing, defaults.phrasing);
        self.vibrato = unit(self.vibrato, defaults.vibrato);
        self.output_gain_db = if self.output_gain_db.is_finite() {
            self.output_gain_db.clamp(-24.0, 12.0)
        } else {
            defaults.output_gain_db
        };
        if self.selected_articulation >= ARTICULATION_SLOT_COUNT {
            self.selected_articulation = defaults.selected_articulation;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverSelection {
    None,
    #[default]
    Pick,
    Bow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodySelection {
    Disabled,
    #[default]
    Guitar,
    Violin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSwitches {
    pub body_contact: bool,
    pub bow_drive: bool,
    pub tension: bool,
}

impl Default for ModelSwitches {
    fn default() -> Self {
        Self {
            body_contact: true,
            bow_drive: true,
            tension: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ArticulationSlot {
    pub sample: Option<SampleReference>,
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}
