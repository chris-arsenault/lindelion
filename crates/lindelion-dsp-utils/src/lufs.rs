//! ITU-R BS.1770-4 loudness meter: K-weighting + mean-square with momentary (400 ms), short-term
//! (3 s), and gated integrated readouts, in LKFS/LUFS. Stereo (L/R, channel gain 1.0).
//!
//! K-weighting is two cascaded biquads — a high-shelf "pre-filter" then an "RLB" high-pass — whose
//! coefficients are derived from the BS.1770-4 analog prototypes by bilinear transform (reproducing
//! the standard's 48 kHz reference table and correct at any sample rate). The integrated measurement
//! gates 400 ms blocks (75 % overlap) with the −70 LKFS absolute and −10 LU relative gates; block
//! loudnesses are accumulated in a bounded preallocated histogram so `push` never allocates.

use crate::filters::{Biquad, BiquadCoefficients};

const LOUDNESS_OFFSET: f64 = -0.691;
const ABSOLUTE_GATE_LKFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;
/// Reported when there is not yet enough signal to measure.
pub const SILENCE_FLOOR_LKFS: f32 = -70.0;

const SUB_BLOCK_MS: f64 = 100.0; // gating hop: a 400 ms block at 75 % overlap advances every 100 ms
const MOMENTARY_SUB_BLOCKS: usize = 4; // 400 ms
const SHORT_TERM_SUB_BLOCKS: usize = 30; // 3 s
const GATING_SUB_BLOCKS: usize = 4; // 400 ms gating block

// Integrated histogram: gating-block loudness binned over [−70, +5) LKFS in 0.1 LU steps.
const HIST_MIN_LKFS: f64 = -70.0;
const HIST_MAX_LKFS: f64 = 5.0;
const HIST_STEP_LKFS: f64 = 0.1;
const HIST_BINS: usize = 750; // (HIST_MAX_LKFS − HIST_MIN_LKFS) / HIST_STEP_LKFS

pub struct LufsMeter {
    pre: [Biquad; 2],
    rlb: [Biquad; 2],
    sub_block_len: usize,
    acc_sq: [f64; 2],
    acc_count: usize,
    sub_sq: Box<[[f64; 2]]>,
    sub_write: usize,
    sub_filled: usize,
    hist: Box<[u64]>,
}

impl LufsMeter {
    pub fn new(sample_rate: f32) -> Self {
        let mut meter = Self {
            pre: [Biquad::new(BiquadCoefficients::identity()); 2],
            rlb: [Biquad::new(BiquadCoefficients::identity()); 2],
            sub_block_len: 1,
            acc_sq: [0.0; 2],
            acc_count: 0,
            sub_sq: vec![[0.0; 2]; SHORT_TERM_SUB_BLOCKS].into_boxed_slice(),
            sub_write: 0,
            sub_filled: 0,
            hist: vec![0; HIST_BINS].into_boxed_slice(),
        };
        meter.reset(sample_rate);
        meter
    }

    /// Recompute K-weighting coefficients for `sample_rate` and clear all measurement state.
    /// Allocation-free (buffers are sized once in `new`).
    pub fn reset(&mut self, sample_rate: f32) {
        let fs = f64::from(sample_rate).max(1.0);
        let pre = kweighting_prefilter(fs);
        let rlb = kweighting_rlb(fs);
        for channel in 0..2 {
            self.pre[channel] = Biquad::new(pre);
            self.rlb[channel] = Biquad::new(rlb);
        }
        self.sub_block_len = (fs * SUB_BLOCK_MS / 1000.0).round().max(1.0) as usize;
        self.acc_sq = [0.0; 2];
        self.acc_count = 0;
        self.sub_sq.fill([0.0; 2]);
        self.sub_write = 0;
        self.sub_filled = 0;
        self.hist.fill(0);
    }

    /// Feed a stereo block (any length). Allocation-free.
    pub fn push(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len());
        for i in 0..n {
            let kl = f64::from(self.rlb[0].process(self.pre[0].process(left[i])));
            let kr = f64::from(self.rlb[1].process(self.pre[1].process(right[i])));
            self.acc_sq[0] += kl * kl;
            self.acc_sq[1] += kr * kr;
            self.acc_count += 1;
            if self.acc_count >= self.sub_block_len {
                self.finalize_sub_block();
            }
        }
    }

    pub fn momentary(&self) -> f32 {
        self.window_loudness(MOMENTARY_SUB_BLOCKS)
            .map_or(SILENCE_FLOOR_LKFS, |l| l as f32)
    }

    pub fn short_term(&self) -> f32 {
        self.window_loudness(SHORT_TERM_SUB_BLOCKS)
            .map_or(SILENCE_FLOOR_LKFS, |l| l as f32)
    }

    pub fn integrated(&self) -> f32 {
        self.integrated_loudness()
            .map_or(SILENCE_FLOOR_LKFS, |l| l as f32)
    }

    fn finalize_sub_block(&mut self) {
        self.sub_sq[self.sub_write] = self.acc_sq;
        self.sub_write = (self.sub_write + 1) % SHORT_TERM_SUB_BLOCKS;
        self.sub_filled = (self.sub_filled + 1).min(SHORT_TERM_SUB_BLOCKS);
        self.acc_sq = [0.0; 2];
        self.acc_count = 0;

        if self.sub_filled >= GATING_SUB_BLOCKS
            && let Some(loudness) = self.window_loudness(GATING_SUB_BLOCKS)
            && loudness >= ABSOLUTE_GATE_LKFS
        {
            self.hist[hist_bin(loudness)] += 1;
        }
    }

    /// K-weighted loudness over the most recent `num` sub-blocks, or `None` if fewer have been
    /// accumulated or the window is silent.
    fn window_loudness(&self, num: usize) -> Option<f64> {
        if self.sub_filled < num {
            return None;
        }
        let mut sum = [0.0f64; 2];
        for k in 0..num {
            let idx = (self.sub_write + SHORT_TERM_SUB_BLOCKS - 1 - k) % SHORT_TERM_SUB_BLOCKS;
            sum[0] += self.sub_sq[idx][0];
            sum[1] += self.sub_sq[idx][1];
        }
        let denom = (num * self.sub_block_len) as f64;
        let z = sum[0] / denom + sum[1] / denom;
        (z > 0.0).then(|| LOUDNESS_OFFSET + 10.0 * z.log10())
    }

    /// Two-pass gated integrated loudness over the histogram of 400 ms gating blocks.
    fn integrated_loudness(&self) -> Option<f64> {
        let (count, energy) = self.gated_energy(ABSOLUTE_GATE_LKFS);
        if count == 0 {
            return None;
        }
        let relative_threshold =
            (LOUDNESS_OFFSET + 10.0 * (energy / count as f64).log10()) + RELATIVE_GATE_LU;
        let (count2, energy2) = self.gated_energy(relative_threshold);
        (count2 > 0).then(|| LOUDNESS_OFFSET + 10.0 * (energy2 / count2 as f64).log10())
    }

    /// Sum of (block count, block energy) over histogram bins whose loudness is ≥ `threshold`.
    fn gated_energy(&self, threshold: f64) -> (u64, f64) {
        let mut count = 0u64;
        let mut energy = 0.0f64;
        for (bin, &c) in self.hist.iter().enumerate() {
            if c == 0 || bin_loudness(bin) < threshold {
                continue;
            }
            energy += bin_energy(bin) * c as f64;
            count += c;
        }
        (count, energy)
    }
}

fn hist_bin(loudness: f64) -> usize {
    let clamped = loudness.clamp(HIST_MIN_LKFS, HIST_MAX_LKFS - HIST_STEP_LKFS);
    (((clamped - HIST_MIN_LKFS) / HIST_STEP_LKFS) as usize).min(HIST_BINS - 1)
}

fn bin_loudness(bin: usize) -> f64 {
    HIST_MIN_LKFS + (bin as f64 + 0.5) * HIST_STEP_LKFS
}

fn bin_energy(bin: usize) -> f64 {
    10f64.powf((bin_loudness(bin) - LOUDNESS_OFFSET) / 10.0)
}

/// BS.1770-4 stage-1 "pre-filter" high-shelf, from the analog prototype by bilinear transform.
fn kweighting_prefilter(fs: f64) -> BiquadCoefficients {
    let f0 = 1681.974450955533;
    let gain_db = 3.999843853973347;
    let q = 0.7071752369554196;
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    BiquadCoefficients {
        b0: ((vh + vb * k / q + k * k) / a0) as f32,
        b1: (2.0 * (k * k - vh) / a0) as f32,
        b2: ((vh - vb * k / q + k * k) / a0) as f32,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    }
}

/// BS.1770-4 stage-2 "RLB" high-pass, from the analog prototype by bilinear transform.
fn kweighting_rlb(fs: f64) -> BiquadCoefficients {
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let denom = 1.0 + k / q + k * k;
    BiquadCoefficients {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: (2.0 * (k * k - 1.0) / denom) as f32,
        a2: ((1.0 - k / q + k * k) / denom) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(freq: f32, amp: f32, seconds: f32) -> Vec<f32> {
        let n = (SR * seconds) as usize;
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn silence_floors_all_readouts() {
        let mut meter = LufsMeter::new(SR);
        let silence = vec![0.0f32; (SR * 3.5) as usize];
        meter.push(&silence, &silence);
        assert_eq!(meter.momentary(), SILENCE_FLOOR_LKFS);
        assert_eq!(meter.short_term(), SILENCE_FLOOR_LKFS);
        assert_eq!(meter.integrated(), SILENCE_FLOOR_LKFS);
    }

    #[test]
    fn stationary_sine_momentary_short_and_integrated_agree() {
        let mut meter = LufsMeter::new(SR);
        let s = sine(1_000.0, 0.5, 4.0);
        meter.push(&s, &s);
        let m = meter.momentary();
        let st = meter.short_term();
        let i = meter.integrated();
        assert!((m - st).abs() < 0.1, "momentary {m} vs short-term {st}");
        assert!((m - i).abs() < 0.2, "momentary {m} vs integrated {i}");
    }

    #[test]
    fn halving_amplitude_drops_momentary_by_six_lu() {
        let mut meter = LufsMeter::new(SR);
        let loud = sine(1_000.0, 0.5, 1.0);
        meter.push(&loud, &loud);
        let m_loud = meter.momentary();
        let quiet = sine(1_000.0, 0.25, 1.0);
        meter.push(&quiet, &quiet);
        let m_quiet = meter.momentary();
        assert!(
            (m_loud - m_quiet - 6.0206).abs() < 0.2,
            "expected ~6.02 LU drop, got {}",
            m_loud - m_quiet
        );
    }

    #[test]
    fn full_scale_sine_reads_plausible_loudness() {
        let mut meter = LufsMeter::new(SR);
        // A 0 dBFS 1 kHz sine: ungated 10·log10(0.5) − 0.691 ≈ −3.7 LKFS plus the small K-gain.
        let s = sine(1_000.0, 1.0, 1.0);
        meter.push(&s, &s);
        let m = meter.momentary();
        assert!((-5.0..=1.0).contains(&m), "0 dBFS sine momentary {m} LKFS");
    }

    #[test]
    fn push_is_allocation_free() {
        let mut meter = LufsMeter::new(SR);
        let s = sine(1_000.0, 0.5, 0.5);
        lindelion_test_allocator::assert_no_allocations("lufs push", || {
            meter.push(&s, &s);
        });
    }
}
