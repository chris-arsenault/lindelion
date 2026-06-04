use lindelion_dsp_utils::{db_to_gain, energy::EnergyFollower, math::midi_note_to_hz};
use lindelion_plugin_shell::{MidiEvent, NoteEvent};
use lindelion_string::{
    BowParams, PickParams, StringBodyMode, StringDriver, StringDriverMode, StringModel,
    StringModelParams, StringModelSwitches,
};

use crate::patch::{BodySelection, DriverSelection, StringPatch};

pub const ARTICULATION_SLOT_COUNT: usize = 8;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const BUILTIN_SAMPLE_RATE: f32 = 48_000.0;
const OVERSAMPLE_FACTOR: usize = 2;
const KEYSWITCH_BASE_NOTE: u8 = 0;
const GATE_RAMP_SECONDS: f32 = 0.006;
const MIN_BRIGHTNESS_HZ: f32 = 900.0;
const MAX_BRIGHTNESS_HZ: f32 = 12_500.0;
const MIN_LOOP_GAIN: f32 = 0.90;
const MAX_LOOP_GAIN: f32 = 0.996;
const STRING_OUTPUT_LIMIT: f32 = 1.0;

pub const ARTICULATION_NAMES: [&str; ARTICULATION_SLOT_COUNT] = [
    "Pick",
    "Sforzando",
    "Legato",
    "Staccato",
    "Marcato",
    "Tremolo",
    "Harmonic",
    "Soft",
];

#[derive(Debug, Clone, Copy)]
pub enum ExcitationSource<'a> {
    BuiltIn {
        slot: usize,
    },
    Loaded {
        samples: &'a [f32],
        sample_rate: f32,
    },
}

impl<'a> ExcitationSource<'a> {
    pub const fn builtin(slot: usize) -> Self {
        Self::BuiltIn { slot }
    }

    pub fn from_samples(samples: &'a [f32], sample_rate: f32, fallback_slot: usize) -> Self {
        if samples.is_empty() {
            return Self::builtin(fallback_slot);
        }
        Self::Loaded {
            samples,
            sample_rate: sanitize_sample_rate(sample_rate),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Injector<'a> {
    source: ExcitationSource<'a>,
    position: f32,
    step: f32,
    gain: f32,
}

impl Default for Injector<'_> {
    fn default() -> Self {
        Self {
            source: ExcitationSource::builtin(0),
            position: 0.0,
            step: 1.0,
            gain: 0.0,
        }
    }
}

impl<'a> Injector<'a> {
    fn trigger(&mut self, source: ExcitationSource<'a>, gain: f32, model_sample_rate: f32) {
        self.source = source;
        self.position = 0.0;
        self.step = match source {
            ExcitationSource::BuiltIn { .. } => {
                BUILTIN_SAMPLE_RATE / sanitize_sample_rate(model_sample_rate)
            }
            ExcitationSource::Loaded { sample_rate, .. } => {
                sample_rate / sanitize_sample_rate(model_sample_rate)
            }
        };
        self.gain = gain.clamp(0.0, 2.0);
    }

    fn clear(&mut self) {
        self.position = 0.0;
        self.gain = 0.0;
    }

    fn process(&mut self) -> f32 {
        if self.gain <= 0.0 {
            return 0.0;
        }
        let sample = match self.source {
            ExcitationSource::BuiltIn { slot } => builtin_sample(slot, self.position),
            ExcitationSource::Loaded { samples, .. } => loaded_sample(samples, self.position),
        };
        let Some(sample) = sample else {
            self.clear();
            return 0.0;
        };
        self.position += self.step.max(0.000_001);
        sample * self.gain
    }
}

#[derive(Debug)]
pub struct StringProcessor<'a> {
    sample_rate: f32,
    model_sample_rate: f32,
    patch: StringPatch,
    string: StringModel,
    driver: StringDriver,
    driver_mode: DriverSelection,
    sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    injector: Injector<'a>,
    selected_slot: usize,
    current_note: Option<u8>,
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    drive_target: f32,
    gate_coeff: f32,
    energy: EnergyFollower,
    energy_value: f32,
}

impl<'a> StringProcessor<'a> {
    pub fn new(
        sample_rate: f32,
        patch: StringPatch,
        sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    ) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        let patch = patch.sanitized();
        let driver_mode = active_driver_mode(&patch);
        Self {
            sample_rate,
            model_sample_rate,
            string: StringModel::new(model_sample_rate),
            driver: string_driver(driver_mode, &patch, model_sample_rate),
            driver_mode,
            selected_slot: patch.selected_articulation,
            patch,
            sources,
            injector: Injector::default(),
            current_note: None,
            frequency_hz: midi_note_to_hz(60.0),
            effort: 0.0,
            drive_gate: 0.0,
            drive_target: 0.0,
            gate_coeff: gate_coeff(model_sample_rate),
            energy: EnergyFollower::new(model_sample_rate),
            energy_value: 0.0,
        }
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sanitize_sample_rate(sample_rate);
        self.model_sample_rate = self.sample_rate * OVERSAMPLE_FACTOR as f32;
        self.string = StringModel::new(self.model_sample_rate);
        self.driver_mode = active_driver_mode(&self.patch);
        self.driver = string_driver(self.driver_mode, &self.patch, self.model_sample_rate);
        self.injector.clear();
        self.current_note = None;
        self.frequency_hz = midi_note_to_hz(60.0);
        self.effort = 0.0;
        self.drive_gate = 0.0;
        self.drive_target = 0.0;
        self.gate_coeff = gate_coeff(self.model_sample_rate);
        self.energy = EnergyFollower::new(self.model_sample_rate);
        self.energy_value = 0.0;
    }

    pub fn set_patch(&mut self, patch: StringPatch) {
        self.patch = patch.sanitized();
        self.selected_slot = self.patch.selected_articulation;
        let next_mode = active_driver_mode(&self.patch);
        if next_mode != self.driver_mode {
            self.driver = string_driver(next_mode, &self.patch, self.model_sample_rate);
            self.driver_mode = next_mode;
        }
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let output_gain = db_to_gain(self.patch.output_gain_db);
        let len = left.len().min(right.len());
        for frame in 0..len {
            let mut sample = 0.0;
            for _ in 0..OVERSAMPLE_FACTOR {
                sample += self.process_model_sample();
            }
            sample = soft_limit(sample * output_gain / OVERSAMPLE_FACTOR as f32);
            left[frame] = sample;
            right[frame] = sample;
        }
    }

    #[cfg(test)]
    pub fn selected_slot(&self) -> usize {
        self.selected_slot
    }

    fn handle_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            let MidiEvent::Note(note) = *event else {
                continue;
            };
            match note {
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 => {
                    if let Some(slot) = keyswitch_slot(note) {
                        self.select_slot(slot);
                    } else {
                        self.note_on(note, velocity);
                    }
                }
                NoteEvent::On { note, .. } | NoteEvent::Off { note, .. } => self.note_off(note),
            }
        }
    }

    fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot.min(ARTICULATION_SLOT_COUNT - 1);
        self.patch.selected_articulation = self.selected_slot;
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.current_note = Some(note);
        self.frequency_hz = midi_note_to_hz(note as f32);
        self.effort = velocity.clamp(0.0, 1.0);
        self.drive_target = 1.0;
        let source = self.sources[self.selected_slot];
        self.injector.trigger(
            source,
            articulation_gain(self.selected_slot, self.effort),
            self.model_sample_rate,
        );
    }

    fn note_off(&mut self, note: u8) {
        if self.current_note == Some(note) {
            self.current_note = None;
            self.drive_target = 0.0;
        }
    }

    fn process_model_sample(&mut self) -> f32 {
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        self.string.set_tension_drive(self.energy_value);
        self.string.set_balance_drive(self.energy_value);

        let excitation = self.injector.process();
        let feedback = self.string.driven_feedback();
        let drive = self
            .driver
            .process(excitation, self.effort, feedback, self.drive_gate);
        let output = self
            .string
            .process(drive, string_params(&self.patch, self.frequency_hz));
        self.energy_value = self.energy.observe(output);
        output
    }
}

fn string_params(patch: &StringPatch, frequency_hz: f32) -> StringModelParams {
    StringModelParams {
        frequency_hz,
        loop_filter_cutoff_hz: brightness_hz(patch.brightness),
        loop_filter_resonance: 0.0,
        loop_gain: loop_gain_from_damping(patch.damping),
        loop_nonlinearity: 0.0,
        dispersion: patch.stiffness,
        strike_position: patch.strike_position,
        pickup_position: patch.pickup_position,
        excitation_spread: 0.0,
        source_body_balance: patch.body_balance,
        body_mode: body_mode(patch.body),
        switches: StringModelSwitches {
            body_contact_enabled: patch.switches.body_contact,
            tension_modulation_enabled: patch.switches.tension,
            source_body_balance_enabled: true,
        },
    }
}

fn string_driver(mode: DriverSelection, patch: &StringPatch, sample_rate: f32) -> StringDriver {
    StringDriver::from_mode(
        match mode {
            DriverSelection::None => StringDriverMode::None,
            DriverSelection::Pick => StringDriverMode::Pick,
            DriverSelection::Bow => StringDriverMode::Bow,
        },
        PickParams {
            hardness: 0.25 + 0.75 * patch.brightness,
            contact_time: 0.15 + 0.55 * (1.0 - patch.stiffness),
        },
        BowParams {
            pressure_depth: 0.25 + 0.65 * patch.body_balance,
            speed: 0.25 + 0.65 * patch.brightness,
            friction: 0.75 - 0.45 * patch.damping,
        },
        sample_rate,
    )
}

fn active_driver_mode(patch: &StringPatch) -> DriverSelection {
    if patch.driver == DriverSelection::Bow && !patch.switches.bow_drive {
        DriverSelection::None
    } else {
        patch.driver
    }
}

fn body_mode(body: BodySelection) -> StringBodyMode {
    match body {
        BodySelection::Disabled => StringBodyMode::Disabled,
        BodySelection::Guitar => StringBodyMode::Guitar,
        BodySelection::Violin => StringBodyMode::Violin,
    }
}

fn brightness_hz(brightness: f32) -> f32 {
    let brightness = brightness.clamp(0.0, 1.0);
    MIN_BRIGHTNESS_HZ * (MAX_BRIGHTNESS_HZ / MIN_BRIGHTNESS_HZ).powf(brightness)
}

fn loop_gain_from_damping(damping: f32) -> f32 {
    let damping = damping.clamp(0.0, 1.0);
    MAX_LOOP_GAIN + (MIN_LOOP_GAIN - MAX_LOOP_GAIN) * damping
}

fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(KEYSWITCH_BASE_NOTE)? as usize;
    (slot < ARTICULATION_SLOT_COUNT).then_some(slot)
}

fn articulation_gain(slot: usize, effort: f32) -> f32 {
    let accent = match slot {
        1 | 4 => 1.3,
        3 | 6 => 0.85,
        7 => 0.65,
        _ => 1.0,
    };
    accent * effort.clamp(0.0, 1.0).sqrt()
}

fn loaded_sample(samples: &[f32], position: f32) -> Option<f32> {
    let index = position.floor() as usize;
    if index >= samples.len() {
        return None;
    }
    let next = (index + 1).min(samples.len() - 1);
    let fraction = position - index as f32;
    Some(samples[index] + (samples[next] - samples[index]) * fraction)
}

fn builtin_sample(slot: usize, position: f32) -> Option<f32> {
    let index = position.floor() as usize;
    let len = builtin_len(slot);
    if index >= len {
        return None;
    }
    let t = index as f32 / BUILTIN_SAMPLE_RATE;
    let sample = match slot {
        0 => decayed_ping(t, 0.0022, 0.0, 1.0),
        1 => decayed_ping(t, 0.0038, 0.5, 1.55),
        2 => decayed_ping(t, 0.0085, 0.15, 0.42),
        3 => decayed_ping(t, 0.0013, 0.9, 1.2),
        4 => decayed_ping(t, 0.0028, 0.35, 1.35),
        5 => tremolo_burst(t),
        6 => decayed_ping(t, 0.0017, 1.4, 0.72),
        _ => decayed_ping(t, 0.0065, 0.05, 0.34),
    };
    Some(sample)
}

fn builtin_len(slot: usize) -> usize {
    match slot {
        2 | 5 | 7 => 720,
        1 | 4 => 360,
        _ => 220,
    }
}

fn decayed_ping(t: f32, decay: f32, phase: f32, gain: f32) -> f32 {
    let carrier = (std::f32::consts::TAU * (780.0 * t + phase)).sin();
    let overtone = (std::f32::consts::TAU * (2_350.0 * t + phase * 0.37)).sin();
    let envelope = (-t / decay.max(0.000_2)).exp();
    gain * envelope * (0.72 * carrier + 0.28 * overtone)
}

fn tremolo_burst(t: f32) -> f32 {
    let pulse = (std::f32::consts::TAU * 42.0 * t).sin().abs();
    decayed_ping(t, 0.012, 0.2, 0.55 * pulse)
}

fn gate_coeff(sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (GATE_RAMP_SECONDS * sanitize_sample_rate(sample_rate))).exp()
}

fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DEFAULT_SAMPLE_RATE
    }
}

fn soft_limit(sample: f32) -> f32 {
    if sample.is_finite() {
        (sample / STRING_OUTPUT_LIMIT).tanh() * STRING_OUTPUT_LIMIT
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
