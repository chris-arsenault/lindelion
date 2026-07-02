//! Limiter: lookahead brickwall limiter.
//!
//! Ports hot-mic's `LimiterPlugin` (Ceiling, Release). Per sample, a target gain
//! `min(1, ceiling/|x|)` is release-smoothed (instant down, one-pole up), passed through a
//! sliding-window **minimum** over the lookahead, then a box average over the lookahead; the
//! signal is delayed by the lookahead and multiplied by the result. Every term of the box average
//! is a windowed minimum whose window contains the emerging sample, so the applied gain is
//! provably `<= ceiling/|x|` for the sample it multiplies — a true brickwall — while the box
//! average turns gain onsets into linear ramps across the lookahead instead of steps. (The
//! previous form let the envelope release *while the peak was still inside the delay*, overshooting
//! the ceiling by up to ~1.3 dB at short release times, and applied instant gain steps.)
//! Reported latency equals the lookahead.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_dsp_utils::delay::DelayLine;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_CEILING_DB: u32 = 0;
pub const PARAM_RELEASE_MS: u32 = 1;

const LOOKAHEAD_MS: f32 = 1.5;
const DELAY_CAPACITY: usize = 2_048;
/// Capacity of the gain-window rings; bounds the lookahead (1.5 ms up to ~340 kHz).
const WINDOW_CAPACITY: usize = 512;

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
    delay: DelayLine,
    lookahead: usize,
    /// Sliding-min / box-average window length (`lookahead + 1`).
    window: usize,
    ceiling_lin: f32,
    /// One-pole rise coefficient for the release-smoothed gain envelope.
    release_coeff: f32,
    /// Release-smoothed gain envelope (drops instantly, rises with the release time).
    envelope: f32,
    /// Monotonic sample counter for the sliding-min deque.
    sample_index: u64,
    /// Monotonic deque (increasing values) over the last `window` envelope samples.
    deque_value: Vec<f32>,
    deque_index: Vec<u64>,
    deque_head: usize,
    deque_len: usize,
    /// Box-average ring over the last `window` sliding-min samples.
    box_ring: Vec<f32>,
    box_pos: usize,
    box_sum: f64,
}

impl Limiter {
    pub fn new() -> Self {
        let mut limiter = Self {
            ceiling_db: -1.0,
            release_ms: 50.0,
            bypassed: false,
            sample_rate: 48_000.0,
            delay: DelayLine::new(DELAY_CAPACITY),
            lookahead: 0,
            window: 1,
            ceiling_lin: db_to_gain(-1.0),
            release_coeff: 0.0,
            envelope: 1.0,
            sample_index: 0,
            deque_value: vec![1.0; WINDOW_CAPACITY],
            deque_index: vec![0; WINDOW_CAPACITY],
            deque_head: 0,
            deque_len: 0,
            box_ring: vec![1.0; WINDOW_CAPACITY],
            box_pos: 0,
            box_sum: 1.0,
        };
        limiter.reconfigure();
        limiter.resize_window();
        limiter
    }

    /// Update the parameter-derived values that are safe to change mid-stream.
    fn reconfigure(&mut self) {
        self.ceiling_lin = db_to_gain(self.ceiling_db);
        let release_s = self.release_ms / 1_000.0;
        self.release_coeff = if release_s <= 0.0 || self.sample_rate <= 0.0 {
            0.0
        } else {
            (-1.0 / (release_s * self.sample_rate)).exp()
        };
    }

    /// Recompute the lookahead window for the sample rate and clear the gain state (prepare-time
    /// only — resizing mid-stream would step the gain).
    fn resize_window(&mut self) {
        self.lookahead = (((LOOKAHEAD_MS / 1_000.0) * self.sample_rate).round() as usize)
            .min(WINDOW_CAPACITY - 1);
        self.window = self.lookahead + 1;
        self.reset_gain_state();
    }

    fn reset_gain_state(&mut self) {
        self.envelope = 1.0;
        self.sample_index = 0;
        self.deque_head = 0;
        self.deque_len = 0;
        self.box_ring[..self.window]
            .iter_mut()
            .for_each(|s| *s = 1.0);
        self.box_pos = 0;
        self.box_sum = self.window as f64;
    }

    /// Push one envelope sample into the sliding-min deque and return the current window minimum.
    fn sliding_min(&mut self, value: f32) -> f32 {
        let index = self.sample_index;
        self.sample_index += 1;
        // Drop back entries that can never be the minimum again.
        while self.deque_len > 0 {
            let back = (self.deque_head + self.deque_len - 1) % WINDOW_CAPACITY;
            if self.deque_value[back] >= value {
                self.deque_len -= 1;
            } else {
                break;
            }
        }
        let slot = (self.deque_head + self.deque_len) % WINDOW_CAPACITY;
        self.deque_value[slot] = value;
        self.deque_index[slot] = index;
        self.deque_len += 1;
        // Expire the front entry once it falls out of the window.
        while self.deque_index[self.deque_head] + self.window as u64 <= index {
            self.deque_head = (self.deque_head + 1) % WINDOW_CAPACITY;
            self.deque_len -= 1;
        }
        self.deque_value[self.deque_head]
    }

    /// Push one sliding-min sample into the box average and return the current mean.
    fn box_average(&mut self, value: f32) -> f32 {
        self.box_sum += value as f64 - self.box_ring[self.box_pos] as f64;
        self.box_ring[self.box_pos] = value;
        self.box_pos += 1;
        if self.box_pos == self.window {
            self.box_pos = 0;
        }
        (self.box_sum / self.window as f64) as f32
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
        self.resize_window();
        self.delay.clear();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        for sample in buffer.iter_mut() {
            let input = *sample;
            self.delay.push(input);
            // Target gain for this incoming sample: at or under it, the sample meets the ceiling.
            let magnitude = input.abs();
            let target = if magnitude > self.ceiling_lin {
                self.ceiling_lin / magnitude
            } else {
                1.0
            };
            // Release smoothing: drop instantly, recover toward unity with the release time. The
            // envelope never exceeds the target, so the brickwall bound below is preserved.
            let recovered = 1.0 + (self.envelope - 1.0) * self.release_coeff;
            self.envelope = target.min(recovered);
            // Sliding min then box average over the lookahead window: by the time the sample
            // leaves the delay, every averaged term had it in its min window, so the applied gain
            // is <= its target — the ceiling holds — and gain onsets ramp instead of stepping.
            let floor = self.sliding_min(self.envelope);
            let gain = self.box_average(floor);
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
        self.delay.clear();
        self.reset_gain_state();
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
            out_peak <= ceiling * 1.001,
            "output {out_peak} exceeds ceiling {ceiling}"
        );
        assert!(
            out_peak > ceiling * 0.9,
            "limiter is not engaging: {out_peak}"
        );
    }

    #[test]
    fn transient_never_overshoots_at_the_shortest_release() {
        // Regression: the previous envelope released while the peak was still inside the
        // lookahead delay, so an isolated transient overshot the ceiling by ~1.3 dB at the
        // 10 ms minimum release. The windowed-min gain must hold the ceiling for any release.
        let mut limiter = Limiter::new();
        limiter.set_parameter(PARAM_RELEASE_MS, 10.0);
        limiter.prepare(48_000.0, 1_024);
        let mut buffer = vec![0.0_f32; 4_096];
        buffer[1_000] = 1.0; // isolated full-scale click
        buffer[2_500] = -0.9;
        limiter.process(&mut buffer);
        let ceiling = db_to_gain(-1.0);
        let peak = peak_abs(&buffer);
        assert!(
            peak <= ceiling * 1.001,
            "transient overshoots ceiling: {peak} > {ceiling}"
        );
    }

    #[test]
    fn gain_onsets_ramp_instead_of_stepping() {
        // The gain applied to the (quiet) samples just before a limited peak must descend
        // gradually across the lookahead, not jump in one sample — steps there are audible
        // clicks. A constant 0.5 pre-fill makes the applied gain directly observable.
        let mut limiter = Limiter::new();
        limiter.prepare(48_000.0, 1_024);
        let n = 4_096;
        let spike = 2_000;
        let mut buffer = vec![0.5_f32; n];
        buffer[spike] = 4.0; // demands gain ~ceiling/4
        limiter.process(&mut buffer);
        let lookahead = limiter.latency_samples();
        // Observe the applied gain over the ramp region before the delayed spike emerges.
        let emerge = spike + lookahead;
        let mut max_step = 0.0_f32;
        for i in (emerge - lookahead + 1)..emerge {
            let step = (buffer[i] - buffer[i - 1]).abs();
            max_step = max_step.max(step);
        }
        // A single-sample step of the full reduction (0.5 -> ~0.11) would be ~0.39; the box
        // average must spread it to ~reduction/lookahead per sample (~0.006).
        assert!(
            max_step < 0.02,
            "gain ramp steps too hard: max per-sample step {max_step}"
        );
    }
}
