//! Articulation excitation data, source descriptors, and the per-note injector.

use super::{
    ARTICULATION_SLOT_COUNT, BUILTIN_EXCITATION_SAMPLE_RATE, KEYSWITCH_BASE_NOTE,
    sanitize_sample_rate,
};

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
pub(super) struct Injector<'a> {
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
    pub(super) fn trigger(
        &mut self,
        source: ExcitationSource<'a>,
        gain: f32,
        model_sample_rate: f32,
    ) {
        self.source = source;
        self.position = 0.0;
        self.step = source.sample_rate / sanitize_sample_rate(model_sample_rate);
        self.gain = gain.clamp(0.0, 2.0);
    }

    pub(super) fn clear(&mut self) {
        self.gain = 0.0;
        self.position = 0.0;
    }

    pub(super) fn process(&mut self) -> f32 {
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

pub(super) fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(KEYSWITCH_BASE_NOTE)? as usize;
    (slot < ARTICULATION_SLOT_COUNT).then_some(slot)
}

pub(super) const fn builtin_samples(slot: usize) -> &'static [f32] {
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
