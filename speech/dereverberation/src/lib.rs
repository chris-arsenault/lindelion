//! Dereverberation: spectral suppression of late-reverb energy.
//!
//! Ports hot-mic's `Enhance-Dereverberation.md` with a Lebart-style late-reverb estimator: late
//! reverberation is modeled per bin as a decayed copy of the spectral power from `DELAY_S` ago
//! (`~64 ms`, past the direct sound and early reflections), scaled by the decay an assumed-`T60`
//! room would apply over that gap. That estimate is power-subtracted from the current frame
//! (phase kept). Onsets pass by construction (current power dwarfs the delayed estimate); steady
//! speech loses at most ~1–2 dB (the decayed self-estimate); tails that ring on slower than the
//! assumed decay — actual reverberation — are pushed toward the gain floor. (The previous form
//! subtracted the *previous* frame, one 5.3 ms hop ago with 75 % window overlap — i.e. the direct
//! sound itself — which attenuated all sustained speech by ~7 dB at the default amount.)
//! Runs in the shared allocation-free STFT.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::stft::StftProcessor;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_AMOUNT_PCT: u32 = 0;

const FRAME_SIZE: usize = 1_024;
const BINS: usize = FRAME_SIZE / 2 + 1;
const HOP: usize = FRAME_SIZE / 4;
/// How far back the late-reverb estimate looks — past the direct sound / early reflections.
const DELAY_S: f32 = 0.064;
/// Upper bound on the estimate delay in frames (sizes the preallocated history ring).
const MAX_DELAY_FRAMES: usize = 32;
/// Assumed room T60 used to decay the delayed power (60 dB per `T60` seconds).
const ASSUMED_T60_S: f32 = 0.5;
/// Amount 100 % maps to this oversubtraction factor (1.0 = exact subtraction of the estimate).
const MAX_SUBTRACTION: f32 = 2.0;
const GAIN_FLOOR: f32 = 0.1;

/// Spectral dereverberation.
pub struct Dereverberation {
    amount_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    stft: StftProcessor,
    /// Ring of per-bin spectral power for the last `delay_frames` frames.
    power_history: Vec<f32>,
    history_pos: usize,
    history_filled: usize,
    /// Estimate delay in frames (`DELAY_S` at the prepared sample rate).
    delay_frames: usize,
    /// Power decay the assumed room applies across `delay_frames`.
    decay_power: f32,
}

impl Dereverberation {
    pub fn new() -> Self {
        let mut effect = Self {
            amount_pct: 60.0,
            bypassed: false,
            sample_rate: 48_000.0,
            stft: StftProcessor::new(FRAME_SIZE),
            power_history: vec![0.0; MAX_DELAY_FRAMES * BINS],
            history_pos: 0,
            history_filled: 0,
            delay_frames: 1,
            decay_power: 0.0,
        };
        effect.reconfigure();
        effect
    }

    fn reconfigure(&mut self) {
        let hop_s = HOP as f32 / self.sample_rate.max(1.0);
        self.delay_frames = ((DELAY_S / hop_s).round() as usize).clamp(1, MAX_DELAY_FRAMES);
        // Amplitude decays by e^(-6.91 t / T60) (60 dB over T60); power is the square.
        let gap_s = self.delay_frames as f32 * hop_s;
        self.decay_power = (-2.0 * 6.91 * gap_s / ASSUMED_T60_S).exp();
    }

    fn subtraction_strength(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_SUBTRACTION
    }

    fn clear_history(&mut self) {
        self.power_history.iter_mut().for_each(|p| *p = 0.0);
        self.history_pos = 0;
        self.history_filled = 0;
    }
}

impl Default for Dereverberation {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Dereverberation {
    fn name(&self) -> &str {
        "Dereverberation"
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
        self.stft.reset();
        self.clear_history();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let strength = self.subtraction_strength();
        let decay = self.decay_power;
        let Self {
            stft,
            power_history,
            history_pos,
            history_filled,
            delay_frames,
            ..
        } = self;
        stft.process(buffer, |spectrum| {
            // The slot about to be overwritten is exactly `delay_frames` frames old once the ring
            // is full; until then the estimate is zero (passthrough), which is correct at startup.
            let slot = &mut power_history[*history_pos * BINS..(*history_pos + 1) * BINS];
            let ready = *history_filled >= *delay_frames;
            for (bin, delayed) in spectrum.iter_mut().zip(slot.iter_mut()) {
                let power = bin.norm_sqr();
                if ready {
                    let reverb = strength * decay * *delayed;
                    let gain = ((power - reverb) / (power + 1.0e-12))
                        .max(GAIN_FLOOR * GAIN_FLOOR)
                        .sqrt();
                    *bin *= gain;
                }
                *delayed = power;
            }
            *history_pos = (*history_pos + 1) % *delay_frames;
            *history_filled = (*history_filled + 1).min(*delay_frames);
        });
    }

    fn latency_samples(&self) -> usize {
        self.stft.latency_samples()
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.stft.reset();
        self.clear_history();
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

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 60.0,
    unit: "%",
}];

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::rms;

    fn burst_then_tail(n: usize, half: usize, tail_tau: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let env = if i < half {
                    0.5
                } else {
                    0.5 * (-((i - half) as f32) / tail_tau).exp()
                };
                env * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin()
            })
            .collect()
    }

    #[test]
    fn decaying_tail_reduced_more_than_steady() {
        let mut effect = Dereverberation::new();
        effect.prepare(48_000.0, 1_024);
        let n = 48_000;
        let half = n / 2;
        // A reverberant tail (tau 4 000 samples ~ T60 0.6 s), ringing on at roughly the assumed
        // room decay — the signature of actual late reverberation.
        let input = burst_then_tail(n, half, 4_000.0);
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let lat = effect.latency_samples();
        let steady = (n / 4, half - 2_000);
        let tail = (half + 4_000, n - 2_000);
        let ratio = |r: (usize, usize)| {
            rms(&buffer[r.0 + lat..r.1 + lat]) / rms(&input[r.0..r.1]).max(f32::MIN_POSITIVE)
        };
        let steady_ratio = ratio(steady);
        let tail_ratio = ratio(tail);
        assert!(
            tail_ratio < steady_ratio * 0.9,
            "tail not reduced relative to direct: steady {steady_ratio} vs tail {tail_ratio}"
        );
    }

    #[test]
    fn sustained_speech_is_barely_attenuated() {
        // Regression: the previous per-frame subtraction cut all sustained sound by ~7 dB at the
        // default amount. The delayed, decay-scaled estimate must leave steady content within
        // ~2 dB of unity.
        let mut effect = Dereverberation::new();
        effect.prepare(48_000.0, 1_024);
        let n = 48_000;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let lat = effect.latency_samples();
        let out = rms(&buffer[n / 2 + lat..n - 2_000 + lat]);
        let reference = rms(&input[n / 2..n - 2_000]);
        let ratio_db = 20.0 * (out / reference).log10();
        assert!(
            ratio_db > -2.0,
            "steady tone attenuated too much: {ratio_db} dB"
        );
        assert!(ratio_db < 0.5, "steady tone boosted: {ratio_db} dB");
    }

    #[test]
    fn onsets_pass_unattenuated() {
        // A burst arriving out of silence has no delayed energy behind it: the estimate is ~0 and
        // the attack must pass at (near) unity.
        let mut effect = Dereverberation::new();
        effect.prepare(48_000.0, 1_024);
        let n = 24_000;
        let onset = 12_000;
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
        let lat = effect.latency_samples();
        // The first ~40 ms after the onset predate the delayed estimate catching up.
        let region = (onset + 512, onset + 2_000);
        let out = rms(&buffer[region.0 + lat..region.1 + lat]);
        let reference = rms(&input[region.0..region.1]);
        assert!(
            out > reference * 0.85,
            "onset attenuated: {reference} -> {out}"
        );
    }
}
