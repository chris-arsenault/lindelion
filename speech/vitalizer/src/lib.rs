//! Vitalizer Mk2-T: psychoacoustic bass/treble shaping with tube saturation (mono).
//!
//! Ports hot-mic's `Vitalizer-Mk2T.md` (Tube approximation, mono — no stereo expander). A bass
//! low-shelf and a treble high-shelf shape the spectrum, followed by an asymmetric tanh "tube"
//! saturation stage. Reuses dsp-utils shelves + saturation.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::saturation::soft_clip;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_BASS_DB: u32 = 0;
pub const PARAM_TREBLE_DB: u32 = 1;
pub const PARAM_DRIVE_PCT: u32 = 2;

const BASS_HZ: f32 = 120.0;
const TREBLE_HZ: f32 = 6_000.0;
const MAX_DRIVE: f32 = 3.0;
const TUBE_ASYMMETRY: f32 = 0.15;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_BASS_DB,
        name: "Bass",
        min: -6.0,
        max: 12.0,
        default: 4.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_TREBLE_DB,
        name: "Treble",
        min: -6.0,
        max: 12.0,
        default: 3.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_DRIVE_PCT,
        name: "Drive",
        min: 0.0,
        max: 100.0,
        default: 30.0,
        unit: "%",
    },
];

/// Psychoacoustic bass/treble shaper with tube saturation.
pub struct Vitalizer {
    bass_db: f32,
    treble_db: f32,
    drive_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    bass_shelf: Biquad,
    treble_shelf: Biquad,
}

impl Vitalizer {
    pub fn new() -> Self {
        let mut v = Self {
            bass_db: 4.0,
            treble_db: 3.0,
            drive_pct: 30.0,
            bypassed: false,
            sample_rate: 48_000.0,
            bass_shelf: Biquad::new(BiquadCoefficients::identity()),
            treble_shelf: Biquad::new(BiquadCoefficients::identity()),
        };
        v.reconfigure();
        v
    }

    fn reconfigure(&mut self) {
        self.bass_shelf
            .set_coefficients(BiquadCoefficients::low_shelf(
                self.sample_rate,
                BASS_HZ,
                self.bass_db,
            ));
        self.treble_shelf
            .set_coefficients(BiquadCoefficients::high_shelf(
                self.sample_rate,
                TREBLE_HZ,
                self.treble_db,
            ));
    }

    fn drive(&self) -> f32 {
        1.0 + (self.drive_pct / 100.0) * MAX_DRIVE
    }
}

impl Default for Vitalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Vitalizer {
    fn name(&self) -> &str {
        "Vitalizer Mk2-T"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_BASS_DB => self.bass_db = value.clamp(-6.0, 12.0),
            PARAM_TREBLE_DB => self.treble_db = value.clamp(-6.0, 12.0),
            PARAM_DRIVE_PCT => self.drive_pct = value.clamp(0.0, 100.0),
            _ => return,
        }
        self.reconfigure();
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let drive = self.drive();
        for sample in buffer.iter_mut() {
            let shaped = self.treble_shelf.process(self.bass_shelf.process(*sample));
            *sample = soft_clip(shaped, drive, TUBE_ASYMMETRY);
        }
    }

    fn latency_samples(&self) -> usize {
        0
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.bass_shelf.reset();
        self.treble_shelf.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12);
        for value in [self.bass_db, self.treble_db, self.drive_pct] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [PARAM_BASS_DB, PARAM_TREBLE_DB, PARAM_DRIVE_PCT] {
            if state.len() >= idx + 4 {
                let v = f32::from_le_bytes([
                    state[idx],
                    state[idx + 1],
                    state[idx + 2],
                    state[idx + 3],
                ]);
                self.set_parameter(setter, v);
                idx += 4;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{peak_abs, windowed_dft_magnitude_at};

    fn response_db(freq: f32, amp: f32) -> f32 {
        let mut v = Vitalizer::new();
        v.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        v.process(&mut buffer);
        let half = n / 2;
        20.0 * (peak_abs(&buffer[half..]) / peak_abs(&input[half..])).log10()
    }

    #[test]
    fn shapes_bass_and_treble() {
        let mid = response_db(2_000.0, 0.05);
        assert!(response_db(80.0, 0.05) > mid + 2.0, "bass not boosted");
        assert!(response_db(8_000.0, 0.05) > mid + 1.0, "treble not boosted");
    }

    #[test]
    fn tube_saturation_adds_harmonics() {
        let mut v = Vitalizer::new();
        v.set_parameter(PARAM_DRIVE_PCT, 100.0);
        v.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        v.process(&mut buffer);
        let h3_in = windowed_dft_magnitude_at(&input, 48_000.0, 3_000.0);
        let h3_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 3_000.0);
        assert!(
            h3_out > h3_in + 0.01,
            "no harmonics added: {h3_in} -> {h3_out}"
        );
    }
}
