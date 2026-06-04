use lindelion_dsp_utils::{db_to_gain, math::midi_note_to_hz};
use lindelion_plugin_shell::{MidiEvent, NoteEvent};
use lindelion_wind::{ReedDriver, ReedParams, ReedTube, ReedTubeParams, ReedTubeSwitches};

use crate::patch::TubePatch;

pub const ARTICULATION_SLOT_COUNT: usize = 8;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const OVERSAMPLE_FACTOR: usize = 2;
const KEYSWITCH_BASE_NOTE: u8 = 0;
const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
const GATE_RAMP_SECONDS: f32 = 0.006;
const MIN_LOOP_GAIN: f32 = 0.90;
const MAX_LOOP_GAIN: f32 = 0.995;
const MIN_BRIGHTNESS_HZ: f32 = 1_200.0;
const MAX_BRIGHTNESS_HZ: f32 = 14_000.0;
const BELL_MIX_EXPONENT: f32 = 1.514_573_2;

const TONGUE: &[f32] = &[
    0.00, 0.46, -0.30, 0.18, -0.12, 0.08, -0.055, 0.038, -0.026, 0.018, -0.012, 0.008, -0.005,
    0.003, -0.0018, 0.001, 0.0,
];
const SFORZANDO: &[f32] = &[
    0.00, 0.86, -0.62, 0.44, -0.34, 0.26, -0.20, 0.16, -0.12, 0.095, -0.073, 0.056, -0.043, 0.032,
    -0.024, 0.017, -0.012, 0.008, -0.005, 0.002, 0.0,
];
const LEGATO: &[f32] = &[0.0, 0.035, -0.018, 0.009, -0.004, 0.0];
const STACCATO: &[f32] = &[0.0, 0.62, -0.48, 0.22, -0.08, 0.018, 0.0];
const MARCATO: &[f32] = &[
    0.0, 0.58, -0.36, 0.23, -0.17, 0.13, -0.10, 0.076, -0.055, 0.038, -0.024, 0.012, 0.0,
];
const BREATH: &[f32] = &[
    0.0, 0.08, -0.03, 0.07, -0.02, 0.05, -0.015, 0.035, -0.012, 0.025, -0.01, 0.016, -0.006, 0.0,
];
const ACCENT: &[f32] = &[
    0.0, 0.70, -0.42, 0.18, -0.06, 0.04, -0.03, 0.02, -0.012, 0.006, 0.0,
];
const SLUR: &[f32] = &[0.0, 0.12, -0.04, 0.028, -0.014, 0.006, 0.0];

pub const ARTICULATION_NAMES: [&str; ARTICULATION_SLOT_COUNT] = [
    "Tongue",
    "Sforzando",
    "Legato",
    "Staccato",
    "Marcato",
    "Breath",
    "Accent",
    "Slur",
];

#[derive(Debug, Clone, Copy)]
pub struct ExcitationSource<'a> {
    samples: &'a [f32],
    sample_rate: f32,
}

impl<'a> ExcitationSource<'a> {
    pub const fn builtin(slot: usize) -> Self {
        Self {
            samples: builtin_samples(slot),
            sample_rate: BUILTIN_EXCITATION_SAMPLE_RATE,
        }
    }

    pub fn from_samples(samples: &'a [f32], sample_rate: f32, slot: usize) -> Self {
        if samples.is_empty() {
            return Self::builtin(slot);
        }
        Self {
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
        self.step = source.sample_rate / sanitize_sample_rate(model_sample_rate);
        self.gain = gain.clamp(0.0, 2.0);
    }

    fn clear(&mut self) {
        self.gain = 0.0;
        self.position = 0.0;
    }

    fn process(&mut self) -> f32 {
        if self.gain <= 0.0 {
            return 0.0;
        }
        let index = self.position.floor() as usize;
        if index >= self.source.samples.len() {
            self.clear();
            return 0.0;
        }
        let next = (index + 1).min(self.source.samples.len() - 1);
        let fraction = self.position - index as f32;
        let a = self.source.samples[index];
        let b = self.source.samples[next];
        self.position += self.step.max(0.000_001);
        (a + (b - a) * fraction) * self.gain
    }
}

#[derive(Debug)]
pub struct TubeProcessor<'a> {
    sample_rate: f32,
    model_sample_rate: f32,
    patch: TubePatch,
    tube: ReedTube,
    reed: ReedDriver,
    sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    injector: Injector<'a>,
    selected_slot: usize,
    current_note: Option<u8>,
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    drive_target: f32,
    gate_coeff: f32,
}

impl<'a> TubeProcessor<'a> {
    pub fn new(
        sample_rate: f32,
        patch: TubePatch,
        sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    ) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        let patch = patch.sanitized();
        let reed = ReedDriver::new(reed_params(&patch), model_sample_rate);
        Self {
            sample_rate,
            model_sample_rate,
            tube: ReedTube::new(model_sample_rate),
            reed,
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
        }
    }

    pub fn reset(&mut self, sample_rate: f32) {
        let sample_rate = sanitize_sample_rate(sample_rate);
        self.sample_rate = sample_rate;
        self.model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        self.tube = ReedTube::new(self.model_sample_rate);
        self.reed = ReedDriver::new(reed_params(&self.patch), self.model_sample_rate);
        self.injector.clear();
        self.current_note = None;
        self.frequency_hz = midi_note_to_hz(60.0);
        self.effort = 0.0;
        self.drive_gate = 0.0;
        self.drive_target = 0.0;
        self.gate_coeff = gate_coeff(self.model_sample_rate);
    }

    pub fn set_patch(&mut self, patch: TubePatch) {
        self.patch = patch.sanitized();
        self.selected_slot = self.patch.selected_articulation;
        self.reed.set_params(reed_params(&self.patch));
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let len = left.len().min(right.len());
        let output_gain = db_to_gain(self.patch.output_gain_db);
        for index in 0..len {
            let mut sample = 0.0;
            for _ in 0..OVERSAMPLE_FACTOR {
                sample += self.process_model_sample();
            }
            sample *= output_gain / OVERSAMPLE_FACTOR as f32;
            let sample = soft_limit(sample);
            left[index] = sample;
            right[index] = sample;
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
                NoteEvent::Off { note, .. } => self.note_off(note),
                NoteEvent::On { note, .. } => self.note_off(note),
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
        let gain = self.effort.sqrt();
        let source = self.sources[self.selected_slot];
        self.injector.trigger(source, gain, self.model_sample_rate);
    }

    fn note_off(&mut self, note: u8) {
        if self.current_note == Some(note) {
            self.current_note = None;
            self.drive_target = 0.0;
        }
    }

    fn process_model_sample(&mut self) -> f32 {
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        let excitation = self.injector.process();
        self.tube.set_brightness_effort(self.effort);
        let mut params = tube_params(&self.patch, self.frequency_hz);
        params.reed_phase_delay_samples = self
            .reed
            .aperture_phase_delay_samples(self.frequency_hz, self.effort);
        let feedback = self.tube.driven_feedback();
        let mouth_wave = self
            .reed
            .process(excitation, self.effort, feedback, self.drive_gate);
        self.tube.process_wind(mouth_wave, params)
    }
}

fn tube_params(patch: &TubePatch, frequency_hz: f32) -> ReedTubeParams {
    ReedTubeParams {
        frequency_hz,
        loop_filter_cutoff_hz: brightness_hz(patch.brightness),
        loop_filter_resonance: 0.0,
        loop_gain: loop_gain_from_damping(patch.damping),
        loop_nonlinearity: 0.0,
        boundary_reflection: -0.75,
        pickup_position: 0.82,
        bell_radiation: bell_radiation_from_mix(patch.bell),
        bell_radiation_shape: patch.bell_radiation_shape,
        body_formant: patch.body_formant,
        reed_phase_delay_samples: 0.0,
        switches: ReedTubeSwitches {
            reed_enabled: true,
            bell_enabled: patch.switches.bell_enabled,
            bore_steepening_enabled: patch.switches.bore_steepening_enabled,
            body_enabled: patch.switches.body_enabled,
        },
    }
}

fn reed_params(patch: &TubePatch) -> ReedParams {
    ReedParams {
        pressure_depth: patch.pressure,
        stiffness: patch.reed_stiffness,
        embouchure: patch.embouchure,
        aperture_inertia: patch.reed_aperture_inertia,
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

fn bell_radiation_from_mix(mix: f32) -> f32 {
    mix.clamp(0.0, 1.0).powf(BELL_MIX_EXPONENT)
}

fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(KEYSWITCH_BASE_NOTE)? as usize;
    (slot < ARTICULATION_SLOT_COUNT).then_some(slot)
}

const fn builtin_samples(slot: usize) -> &'static [f32] {
    match slot {
        0 => TONGUE,
        1 => SFORZANDO,
        2 => LEGATO,
        3 => STACCATO,
        4 => MARCATO,
        5 => BREATH,
        6 => ACCENT,
        _ => SLUR,
    }
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
        sample.tanh()
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
