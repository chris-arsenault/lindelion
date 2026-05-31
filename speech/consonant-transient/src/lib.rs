//! Consonant Transient: emphasize consonant attacks, keyed by onset flux.
//!
//! Ports hot-mic's `Enhance-Consonant-Transient.md`. A fast vs. slow envelope detector measures
//! transient strength (the attack overshoot); the signal is boosted during attacks, scaled by
//! OnsetFluxHigh from the worker (baseline-floored). Reuses the shared envelope follower + worker.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::AnalysisWorker;

pub const PARAM_AMOUNT_PCT: u32 = 0;

const FLUX_BASELINE: f32 = 0.5;
const MAX_BOOST: f32 = 4.0;
const TRANSIENT_DEADZONE: f32 = 0.15; // ignore steady-state envelope ripple

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 40.0,
    unit: "%",
}];

/// Onset-flux key in `[BASELINE, 1]` — emphasis rises with onset activity.
pub fn transient_key(flux: f32) -> f32 {
    FLUX_BASELINE + (1.0 - FLUX_BASELINE) * flux.clamp(0.0, 1.0)
}

/// Onset-keyed transient emphasis.
pub struct ConsonantTransient {
    amount_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    worker: Option<AnalysisWorker>,
    fast_env: EnvelopeFollower,
    slow_env: EnvelopeFollower,
}

impl ConsonantTransient {
    pub fn new() -> Self {
        Self {
            amount_pct: 40.0,
            bypassed: false,
            sample_rate: 48_000.0,
            worker: None,
            fast_env: EnvelopeFollower::new(DetectorMode::Rms),
            slow_env: EnvelopeFollower::new(DetectorMode::Rms),
        }
    }

    fn amount(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_BOOST
    }
}

impl Default for ConsonantTransient {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for ConsonantTransient {
    fn name(&self) -> &str {
        "Consonant Transient"
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
        self.fast_env.set_times(0.003, 0.020, sample_rate);
        self.slow_env.set_times(0.030, 0.120, sample_rate);
        self.worker = Some(AnalysisWorker::new(sample_rate as u32));
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.push(buffer);
        }
        let flux = self
            .worker
            .as_ref()
            .map_or(0.0, |w| w.latest().onset_flux_high);
        let key = transient_key(flux);
        let amount = self.amount();
        for sample in buffer.iter_mut() {
            let dry = *sample;
            let fast = self.fast_env.process(dry);
            let slow = self.slow_env.process(dry);
            let transient = ((fast - slow) / (slow + 1.0e-6) - TRANSIENT_DEADZONE).max(0.0);
            *sample = dry * (1.0 + amount * key * transient);
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
        self.fast_env.reset();
        self.slow_env.reset();
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
    use lindelion_dsp_utils::analysis::peak_abs;

    #[test]
    fn key_rises_with_onset_flux() {
        assert!(transient_key(1.0) > transient_key(0.0));
    }

    #[test]
    fn emphasizes_onset_over_steady_state() {
        let mut effect = ConsonantTransient::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 12_000;
        let onset = 4_000;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                if i < onset {
                    0.0
                } else {
                    0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin()
                }
            })
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let onset_in = peak_abs(&input[onset..onset + 800]);
        let onset_out = peak_abs(&buffer[onset..onset + 800]);
        let steady_in = peak_abs(&input[8_000..]);
        let steady_out = peak_abs(&buffer[8_000..]);
        assert!(
            onset_out > onset_in * 1.05,
            "onset not emphasized: {onset_in} -> {onset_out}"
        );
        assert!(
            (steady_out - steady_in).abs() < steady_in * 0.1,
            "steady state changed: {steady_in} -> {steady_out}"
        );
    }
}
