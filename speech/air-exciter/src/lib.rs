//! Air Exciter: keyed high-frequency exciter, de-ess-aware.
//!
//! Ports hot-mic's `Enhance-Air-Exciter.md`. A high-passed copy of the signal is soft-clipped to
//! generate high-frequency harmonics ("air"), blended back in. The excitation is keyed down by
//! SibilanceEnergy so it backs off on sibilants (avoids harsh esses). Reuses dsp-utils filters +
//! saturation + the inline sibilance signal.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::saturation::soft_clip;
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::SibilanceEnergy;

pub const PARAM_AMOUNT_PCT: u32 = 0;

const AIR_HPF_HZ: f32 = 4_000.0;
const DRIVE: f32 = 4.0;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 40.0,
    unit: "%",
}];

/// Excitation blend gain for a given amount and sibilance activity (keyed down by sibilance).
pub fn excitation_gain(amount: f32, sibilance: f32) -> f32 {
    amount.max(0.0) * (1.0 - sibilance.clamp(0.0, 1.0))
}

/// De-ess-aware air exciter.
pub struct AirExciter {
    amount_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    hpf: Biquad,
    sibilance: SibilanceEnergy,
}

impl AirExciter {
    pub fn new() -> Self {
        Self {
            amount_pct: 40.0,
            bypassed: false,
            sample_rate: 48_000.0,
            hpf: Biquad::new(BiquadCoefficients::identity()),
            sibilance: SibilanceEnergy::new(),
        }
    }

    fn reconfigure(&mut self) {
        self.hpf.set_coefficients(BiquadCoefficients::highpass(
            self.sample_rate,
            AIR_HPF_HZ,
            0.707,
        ));
        self.sibilance.prepare(self.sample_rate);
    }
}

impl Default for AirExciter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for AirExciter {
    fn name(&self) -> &str {
        "Air Exciter"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        if index == PARAM_AMOUNT_PCT {
            self.amount_pct = value.clamp(0.0, 100.0);
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let amount = self.amount_pct / 100.0;
        for sample in buffer.iter_mut() {
            let dry = *sample;
            let sibilance = self.sibilance.process(dry);
            let air = soft_clip(self.hpf.process(dry), DRIVE, 0.0);
            *sample = dry + excitation_gain(amount, sibilance) * air;
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
        self.hpf.reset();
        self.sibilance.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        self.amount_pct.to_le_bytes().to_vec()
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.set_parameter(
                PARAM_AMOUNT_PCT,
                f32::from_le_bytes([state[0], state[1], state[2], state[3]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::windowed_dft_magnitude_at;

    #[test]
    fn keyed_down_by_sibilance() {
        assert!(excitation_gain(1.0, 0.0) > excitation_gain(1.0, 1.0));
        assert_eq!(excitation_gain(1.0, 1.0), 0.0);
    }

    #[test]
    fn adds_high_frequency_harmonics() {
        let mut effect = AirExciter::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 4_500.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let h3_in = windowed_dft_magnitude_at(&input, 48_000.0, 13_500.0);
        let h3_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 13_500.0);
        assert!(
            h3_out > h3_in + 0.005,
            "no HF harmonics added: {h3_in} -> {h3_out}"
        );
    }
}
