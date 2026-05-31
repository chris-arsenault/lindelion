//! Upward Expander: multiband upward expansion of quiet speech detail, gated by speech presence.
//!
//! Ports hot-mic's `UpwardExpanderPlugin` (Amount, Scale, Threshold, Low/High Split, Attack,
//! Release, Gate Strength). The signal is split into low / mid / high bands; per-band energy
//! below threshold is boosted (upward expansion) by up to Amount, gated by SpeechPresence
//! (inline) and VoicingState (from the worker, so silence is never expanded). Bands recombine to
//! unity when no boost is applied.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::{db_to_gain, gain_to_db};
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::{AnalysisWorker, SpeechPresence};

pub const PARAM_AMOUNT_PCT: u32 = 0;
pub const PARAM_THRESHOLD_DB: u32 = 1;
pub const PARAM_LOW_SPLIT_HZ: u32 = 2;
pub const PARAM_HIGH_SPLIT_HZ: u32 = 3;
pub const PARAM_ATTACK_MS: u32 = 4;
pub const PARAM_RELEASE_MS: u32 = 5;
pub const PARAM_GATE_STRENGTH: u32 = 6;

const RANGE_DB: f32 = 20.0;
const MAX_BOOST_DB: f32 = 12.0;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_AMOUNT_PCT,
        name: "Amount",
        min: 0.0,
        max: 100.0,
        default: 20.0,
        unit: "%",
    },
    EffectParam {
        index: PARAM_THRESHOLD_DB,
        name: "Threshold",
        min: -60.0,
        max: -10.0,
        default: -35.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_LOW_SPLIT_HZ,
        name: "Low Split",
        min: 80.0,
        max: 400.0,
        default: 200.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_HIGH_SPLIT_HZ,
        name: "High Split",
        min: 1_500.0,
        max: 8_000.0,
        default: 3_500.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_ATTACK_MS,
        name: "Attack",
        min: 2.0,
        max: 50.0,
        default: 8.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_RELEASE_MS,
        name: "Release",
        min: 30.0,
        max: 300.0,
        default: 120.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_GATE_STRENGTH,
        name: "Gate Strength",
        min: 0.0,
        max: 1.0,
        default: 0.8,
        unit: "",
    },
];

/// Upward-expansion gain (dB, >= 0) for a band `env_db` below `threshold_db`, scaled by `gate`.
pub fn upward_gain_db(amount_db: f32, threshold_db: f32, env_db: f32, gate: f32) -> f32 {
    let below = (threshold_db - env_db).clamp(0.0, RANGE_DB);
    amount_db * (below / RANGE_DB) * gate.clamp(0.0, 1.0)
}

/// Multiband upward expander.
pub struct UpwardExpander {
    amount_pct: f32,
    threshold_db: f32,
    low_split_hz: f32,
    high_split_hz: f32,
    attack_ms: f32,
    release_ms: f32,
    gate_strength: f32,
    bypassed: bool,
    sample_rate: f32,
    worker: Option<AnalysisWorker>,
    presence: SpeechPresence,
    low_lpf: Biquad,
    high_hpf: Biquad,
    low_env: EnvelopeFollower,
    mid_env: EnvelopeFollower,
    high_env: EnvelopeFollower,
}

impl UpwardExpander {
    pub fn new() -> Self {
        Self {
            amount_pct: 20.0,
            threshold_db: -35.0,
            low_split_hz: 200.0,
            high_split_hz: 3_500.0,
            attack_ms: 8.0,
            release_ms: 120.0,
            gate_strength: 0.8,
            bypassed: false,
            sample_rate: 48_000.0,
            worker: None,
            presence: SpeechPresence::new(),
            low_lpf: Biquad::new(BiquadCoefficients::identity()),
            high_hpf: Biquad::new(BiquadCoefficients::identity()),
            low_env: EnvelopeFollower::new(DetectorMode::Rms),
            mid_env: EnvelopeFollower::new(DetectorMode::Rms),
            high_env: EnvelopeFollower::new(DetectorMode::Rms),
        }
    }

    fn reconfigure(&mut self) {
        let sr = self.sample_rate;
        self.low_lpf
            .set_coefficients(BiquadCoefficients::lowpass(sr, self.low_split_hz, 0.707));
        self.high_hpf
            .set_coefficients(BiquadCoefficients::highpass(sr, self.high_split_hz, 0.707));
        let (a, r) = (self.attack_ms / 1_000.0, self.release_ms / 1_000.0);
        self.low_env.set_times(a, r, sr);
        self.mid_env.set_times(a, r, sr);
        self.high_env.set_times(a, r, sr);
    }

    fn amount_db(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_BOOST_DB
    }
}

impl Default for UpwardExpander {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for UpwardExpander {
    fn name(&self) -> &str {
        "Upward Expander"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_AMOUNT_PCT => self.amount_pct = value.clamp(0.0, 100.0),
            PARAM_THRESHOLD_DB => self.threshold_db = value.clamp(-60.0, -10.0),
            PARAM_LOW_SPLIT_HZ => self.low_split_hz = value.clamp(80.0, 400.0),
            PARAM_HIGH_SPLIT_HZ => self.high_split_hz = value.clamp(1_500.0, 8_000.0),
            PARAM_ATTACK_MS => self.attack_ms = value.clamp(2.0, 50.0),
            PARAM_RELEASE_MS => self.release_ms = value.clamp(30.0, 300.0),
            PARAM_GATE_STRENGTH => self.gate_strength = value.clamp(0.0, 1.0),
            _ => return,
        }
        self.reconfigure();
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.presence.prepare(sample_rate);
        self.worker = Some(AnalysisWorker::new(sample_rate as u32));
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed || buffer.is_empty() {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.push(buffer);
        }
        let voicing_state = self
            .worker
            .as_ref()
            .map_or(0.0, |w| w.latest().voicing_state);
        let speech_gate = voicing_state >= 0.5; // not silence
        let amount_db = self.amount_db();

        for sample in buffer.iter_mut() {
            let input = *sample;
            let presence = self.presence.process(input);
            let gate = if speech_gate {
                presence * self.gate_strength
            } else {
                0.0
            };

            let low = self.low_lpf.process(input);
            let high = self.high_hpf.process(input);
            let mid = input - low - high;

            let low_g = db_to_gain(upward_gain_db(
                amount_db,
                self.threshold_db,
                gain_to_db(self.low_env.process(low)),
                gate,
            ));
            let mid_g = db_to_gain(upward_gain_db(
                amount_db,
                self.threshold_db,
                gain_to_db(self.mid_env.process(mid)),
                gate,
            ));
            let high_g = db_to_gain(upward_gain_db(
                amount_db,
                self.threshold_db,
                gain_to_db(self.high_env.process(high)),
                gate,
            ));

            *sample = low * low_g + mid * mid_g + high * high_g;
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
        self.presence.reset();
        self.low_lpf.reset();
        self.high_hpf.reset();
        self.low_env.reset();
        self.mid_env.reset();
        self.high_env.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(28);
        for value in [
            self.amount_pct,
            self.threshold_db,
            self.low_split_hz,
            self.high_split_hz,
            self.attack_ms,
            self.release_ms,
            self.gate_strength,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [
            PARAM_AMOUNT_PCT,
            PARAM_THRESHOLD_DB,
            PARAM_LOW_SPLIT_HZ,
            PARAM_HIGH_SPLIT_HZ,
            PARAM_ATTACK_MS,
            PARAM_RELEASE_MS,
            PARAM_GATE_STRENGTH,
        ] {
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

    #[test]
    fn boosts_quiet_when_present_not_in_silence() {
        let amount = (20.0 / 100.0) * MAX_BOOST_DB;
        // 10 dB below threshold, fully gated open -> positive boost.
        assert!(upward_gain_db(amount, -35.0, -45.0, 1.0) > 0.0);
        // Same level, gate closed (silence) -> no boost.
        assert_eq!(upward_gain_db(amount, -35.0, -45.0, 0.0), 0.0);
        // Above threshold -> no boost even when gated open.
        assert_eq!(upward_gain_db(amount, -35.0, -20.0, 1.0), 0.0);
    }
}
