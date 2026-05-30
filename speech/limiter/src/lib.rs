//! Limiter: lookahead brickwall limiter.
//!
//! Ports hot-mic's `LimiterPlugin` (Ceiling, Release). A peak detector with instant attack and a
//! release tail (reusing the shared envelope follower with a zero attack time) drives the gain;
//! the signal is delayed by the lookahead so the gain is already reduced when a peak reaches the
//! output, keeping output at or under the ceiling. Reported latency equals the lookahead.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_dsp_utils::delay::DelayLine;
use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_CEILING_DB: u32 = 0;
pub const PARAM_RELEASE_MS: u32 = 1;

const LOOKAHEAD_MS: f32 = 1.5;
const DELAY_CAPACITY: usize = 2_048;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_CEILING_DB,
        name: "Ceiling",
        min: -3.0,
        max: 0.0,
        default: -1.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_RELEASE_MS,
        name: "Release",
        min: 10.0,
        max: 200.0,
        default: 50.0,
        unit: "ms",
    },
];

/// Lookahead brickwall limiter.
pub struct Limiter {
    ceiling_db: f32,
    release_ms: f32,
    bypassed: bool,
    sample_rate: f32,
    detector: EnvelopeFollower,
    delay: DelayLine,
    lookahead: usize,
    ceiling_lin: f32,
}

impl Limiter {
    pub fn new() -> Self {
        let mut limiter = Self {
            ceiling_db: -1.0,
            release_ms: 50.0,
            bypassed: false,
            sample_rate: 48_000.0,
            detector: EnvelopeFollower::new(DetectorMode::Peak),
            delay: DelayLine::new(DELAY_CAPACITY),
            lookahead: 0,
            ceiling_lin: db_to_gain(-1.0),
        };
        limiter.reconfigure();
        limiter
    }

    fn reconfigure(&mut self) {
        self.detector
            .set_times(0.0, self.release_ms / 1_000.0, self.sample_rate);
        self.lookahead = ((LOOKAHEAD_MS / 1_000.0) * self.sample_rate).round() as usize;
        self.ceiling_lin = db_to_gain(self.ceiling_db);
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Limiter {
    fn name(&self) -> &str {
        "Limiter"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_CEILING_DB => self.ceiling_db = value.clamp(-3.0, 0.0),
            PARAM_RELEASE_MS => self.release_ms = value.clamp(10.0, 200.0),
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
        for sample in buffer.iter_mut() {
            let input = *sample;
            self.delay.push(input);
            let env = self.detector.process(input);
            let gain = if env > self.ceiling_lin {
                self.ceiling_lin / env
            } else {
                1.0
            };
            *sample = self.delay.read(self.lookahead as f32) * gain;
        }
    }

    fn latency_samples(&self) -> usize {
        self.lookahead
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.delay.clear();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.ceiling_db.to_le_bytes());
        bytes.extend_from_slice(&self.release_ms.to_le_bytes());
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.set_parameter(
                PARAM_CEILING_DB,
                f32::from_le_bytes([state[0], state[1], state[2], state[3]]),
            );
        }
        if state.len() >= 8 {
            self.set_parameter(
                PARAM_RELEASE_MS,
                f32::from_le_bytes([state[4], state[5], state[6], state[7]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::peak_abs;

    #[test]
    fn output_stays_under_ceiling() {
        let mut limiter = Limiter::new();
        limiter.prepare(48_000.0, 1_024);
        let n = 20_000;
        let input: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * 300.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        limiter.process(&mut buffer);
        let tail = n / 2;
        let out_peak = peak_abs(&buffer[tail..]);
        let ceiling = db_to_gain(-1.0);
        assert!(
            out_peak <= ceiling * 1.02,
            "output {out_peak} exceeds ceiling {ceiling}"
        );
        assert!(
            out_peak > ceiling * 0.9,
            "limiter is not engaging: {out_peak}"
        );
    }
}
