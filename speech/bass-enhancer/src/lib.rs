//! Bass Enhancer: psychoacoustic bass via harmonics of the low band.
//!
//! Ports hot-mic's `Enhance-Bass-Enhancer.md`. The low band is isolated and soft-clipped to
//! generate harmonics; the ear perceives bass from the harmonic series even without low-end
//! power ("missing fundamental"). The blend is keyed by VoicingScore (from the worker) with a
//! baseline so bass is emphasized on voiced speech but not fully gated off. Reuses dsp-utils
//! filters + saturation + the worker.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::saturation::soft_clip;
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::AnalysisWorker;

pub const PARAM_AMOUNT_PCT: u32 = 0;

const LOW_LPF_HZ: f32 = 180.0;
const DRIVE: f32 = 3.0;
const ASYMMETRY: f32 = 0.2;
const VOICING_BASELINE: f32 = 0.4;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 50.0,
    unit: "%",
}];

/// Bass-harmonic blend gain for a given amount and voicing score (baseline-floored).
pub fn bass_gain(amount: f32, voicing: f32) -> f32 {
    amount.max(0.0) * (VOICING_BASELINE + (1.0 - VOICING_BASELINE) * voicing.clamp(0.0, 1.0))
}

/// Voicing-keyed psychoacoustic bass enhancer.
pub struct BassEnhancer {
    amount_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    worker: Option<AnalysisWorker>,
    lpf: Biquad,
}

impl BassEnhancer {
    pub fn new() -> Self {
        Self {
            amount_pct: 50.0,
            bypassed: false,
            sample_rate: 48_000.0,
            worker: None,
            lpf: Biquad::new(BiquadCoefficients::identity()),
        }
    }
}

impl Default for BassEnhancer {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for BassEnhancer {
    fn name(&self) -> &str {
        "Bass Enhancer"
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
        self.lpf
            .set_coefficients(BiquadCoefficients::lowpass(sample_rate, LOW_LPF_HZ, 0.707));
        self.worker = Some(AnalysisWorker::new(sample_rate as u32));
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.push(buffer);
        }
        let voicing = self
            .worker
            .as_ref()
            .map_or(0.0, |w| w.latest().voicing_score);
        let gain = bass_gain(self.amount_pct / 100.0, voicing);
        for sample in buffer.iter_mut() {
            let dry = *sample;
            let harmonics = soft_clip(self.lpf.process(dry), DRIVE, ASYMMETRY);
            *sample = dry + gain * harmonics;
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
        self.lpf.reset();
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
    fn voicing_raises_gain() {
        assert!(bass_gain(1.0, 1.0) > bass_gain(1.0, 0.0));
    }

    #[test]
    fn adds_harmonics_above_fundamental() {
        let mut effect = BassEnhancer::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 16_384;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 100.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let h2_in = windowed_dft_magnitude_at(&input, 48_000.0, 200.0);
        let h2_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 200.0);
        assert!(
            h2_out > h2_in * 50.0 && h2_out > 0.001,
            "no bass harmonics added: {h2_in} -> {h2_out}"
        );
    }
}
