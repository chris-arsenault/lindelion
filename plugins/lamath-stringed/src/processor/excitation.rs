//! Excitation sources for the string articulations: the per-note injector,
//! loaded-sample interpolation, and the built-in articulation pings.

#![allow(clippy::wildcard_imports)]

use super::*;

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

    pub(super) fn clear(&mut self) {
        self.position = 0.0;
        self.gain = 0.0;
    }

    pub(super) fn process(&mut self) -> f32 {
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
