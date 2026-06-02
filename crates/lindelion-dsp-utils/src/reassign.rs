//! Allocation-free streaming **reassignment** analyzer (method of reassignment, Auger–Flandrin /
//! Fulop–Fitz).
//!
//! Per analysis hop it runs **three** forward STFTs of the same windowed frame — with the analysis
//! window `h`, the time-ramped window `Th[m] = (m − (N−1)/2)·h[m]`, and the window derivative
//! `Dh[m] = h′[m]` — and from their ratios derives, per bin `k` (guarding `|X_h|² > ε`):
//!
//! - **magnitude** `|X_h(k)|` (the same quantity a plain STFT shows);
//! - **frequency offset** as a *fractional bin* added to `k`: `−(N/2π)·Im(X_Dh/X_h)` (channelized
//!   instantaneous frequency — reassigned bin = `k + freq_offsets[k]`);
//! - **time offset** in *samples* relative to the window center: `Re(X_Th/X_h)` (local group
//!   delay — reassigned time = window-center + `time_offsets[k]`).
//!
//! This is **forward-only** (no inverse, no overlap-add) — distinct from the WOLA
//! [`crate::stft::StftProcessor`], whose framing shape it mirrors. The analysis window is a plain
//! Hann ([`crate::window::hann_f64`]); there is no resynthesis, so the sqrt-Hann WOLA uses is not
//! needed. All buffers and FFT scratch are preallocated, so [`ReassignStft::process`] does not
//! allocate (uses realfft `process_with_scratch`).

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::window;

/// `|X_h|²` floor below which a bin carries no usable phase information: emit zero offsets so the
/// energy stays at its bin/center rather than being thrown by division noise.
const ENERGY_EPS: f32 = 1.0e-12;

/// One frame of reassignment output: three per-bin lanes, each length `frame_size/2 + 1`.
pub struct ReassignFrame<'a> {
    /// `|X_h(k)|` — the plain STFT magnitude.
    pub magnitudes: &'a [f32],
    /// Fractional-bin frequency offset added to bin `k` (reassigned bin = `k + freq_offsets[k]`).
    pub freq_offsets: &'a [f32],
    /// Time offset in samples relative to the window center (reassigned time = center + this).
    pub time_offsets: &'a [f32],
}

/// Forward-only streaming STFT that emits reassignment coordinates per hop (75 % overlap).
pub struct ReassignStft {
    frame_size: usize,
    hop: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    win_h: Vec<f32>,
    win_th: Vec<f32>,
    win_dh: Vec<f32>,
    in_ring: Vec<f32>,
    in_pos: usize,
    hop_countdown: usize,
    frame_h: Vec<f32>,
    frame_th: Vec<f32>,
    frame_dh: Vec<f32>,
    spec_h: Vec<Complex32>,
    spec_th: Vec<Complex32>,
    spec_dh: Vec<Complex32>,
    scratch: Vec<Complex32>,
    mags: Vec<f32>,
    freq_offsets: Vec<f32>,
    time_offsets: Vec<f32>,
}

impl ReassignStft {
    /// Create an analyzer with the given `frame_size` (power of two) and 75 % overlap.
    pub fn new(frame_size: usize) -> Self {
        let hop = frame_size / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(frame_size);

        let n = frame_size;
        // `(N-1)` for the Hann phase and its derivative; the ramp is centered on the frame.
        let denom = (n.max(2) - 1) as f64;
        let center = (n as f64 - 1.0) / 2.0;
        let win_h: Vec<f32> = (0..n).map(|i| window::hann_f64(i, n) as f32).collect();
        let win_th: Vec<f32> = (0..n)
            .map(|i| ((i as f64 - center) * window::hann_f64(i, n)) as f32)
            .collect();
        // Exact derivative of `0.5 - 0.5·cos(2π i /(N-1))` w.r.t. `i`: `(π/(N-1))·sin(2π i /(N-1))`.
        let win_dh: Vec<f32> = (0..n)
            .map(|i| {
                (std::f64::consts::PI / denom * (std::f64::consts::TAU * i as f64 / denom).sin())
                    as f32
            })
            .collect();

        let spec_h = forward.make_output_vec();
        let spec_th = forward.make_output_vec();
        let spec_dh = forward.make_output_vec();
        let scratch = forward.make_scratch_vec();
        let bins = n / 2 + 1;

        Self {
            frame_size,
            hop,
            forward,
            win_h,
            win_th,
            win_dh,
            in_ring: vec![0.0; frame_size],
            in_pos: 0,
            hop_countdown: hop,
            frame_h: vec![0.0; frame_size],
            frame_th: vec![0.0; frame_size],
            frame_dh: vec![0.0; frame_size],
            spec_h,
            spec_th,
            spec_dh,
            scratch,
            mags: vec![0.0; bins],
            freq_offsets: vec![0.0; bins],
            time_offsets: vec![0.0; bins],
        }
    }

    /// Analysis frame size in samples.
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Hop between analysis frames (`frame_size / 4`).
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Number of spectral bins per frame (`frame_size / 2 + 1`).
    pub fn bins(&self) -> usize {
        self.mags.len()
    }

    /// Clear all framing state.
    pub fn reset(&mut self) {
        self.in_ring.iter_mut().for_each(|s| *s = 0.0);
        self.in_pos = 0;
        self.hop_countdown = self.hop;
    }

    /// Feed a block of samples (read-only). `on_frame` is invoked once per hop with the frame's
    /// reassignment lanes. Allocation-free.
    pub fn process(&mut self, input: &[f32], mut on_frame: impl FnMut(&ReassignFrame)) {
        for &sample in input {
            self.in_ring[self.in_pos] = sample;
            self.in_pos = (self.in_pos + 1) % self.frame_size;
            self.hop_countdown -= 1;
            if self.hop_countdown == 0 {
                self.hop_countdown = self.hop;
                self.process_frame(&mut on_frame);
            }
        }
    }

    fn process_frame(&mut self, on_frame: &mut impl FnMut(&ReassignFrame)) {
        // Window the frame oldest-to-newest under all three windows.
        for i in 0..self.frame_size {
            let idx = (self.in_pos + i) % self.frame_size;
            let s = self.in_ring[idx];
            self.frame_h[i] = s * self.win_h[i];
            self.frame_th[i] = s * self.win_th[i];
            self.frame_dh[i] = s * self.win_dh[i];
        }
        let _ = self.forward.process_with_scratch(
            &mut self.frame_h,
            &mut self.spec_h,
            &mut self.scratch,
        );
        let _ = self.forward.process_with_scratch(
            &mut self.frame_th,
            &mut self.spec_th,
            &mut self.scratch,
        );
        let _ = self.forward.process_with_scratch(
            &mut self.frame_dh,
            &mut self.spec_dh,
            &mut self.scratch,
        );

        // Δω = −Im(X_Dh/X_h) [rad/sample] → fractional bins via ·N/(2π).
        let freq_scale = -(self.frame_size as f32) / std::f32::consts::TAU;
        for k in 0..self.mags.len() {
            let xh = self.spec_h[k];
            let denom = xh.norm_sqr();
            self.mags[k] = xh.norm();
            if denom > ENERGY_EPS {
                let inv = 1.0 / denom;
                let conj = xh.conj();
                let num_d = self.spec_dh[k] * conj; // X_Dh · conj(X_h)
                let num_t = self.spec_th[k] * conj; // X_Th · conj(X_h)
                self.freq_offsets[k] = freq_scale * (num_d.im * inv);
                self.time_offsets[k] = num_t.re * inv;
            } else {
                self.freq_offsets[k] = 0.0;
                self.time_offsets[k] = 0.0;
            }
        }

        let frame = ReassignFrame {
            magnitudes: &self.mags,
            freq_offsets: &self.freq_offsets,
            time_offsets: &self.time_offsets,
        };
        on_frame(&frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_offset_sharpens_an_offgrid_sine() {
        let n = 2048;
        let sr = 48_000.0_f32;
        let mut an = ReassignStft::new(n);
        let bin_target = 100.5_f32; // deliberately mid-way between bins 100 and 101
        let freq = bin_target * sr / n as f32;
        let total = n * 4;
        let input: Vec<f32> = (0..total)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / sr).sin())
            .collect();

        let mut last_mag = Vec::new();
        let mut last_df = Vec::new();
        an.process(&input, |f| {
            last_mag = f.magnitudes.to_vec();
            last_df = f.freq_offsets.to_vec();
        });
        assert!(!last_mag.is_empty(), "no frames produced");

        let mut peak = (0usize, 0.0f32);
        for (k, &m) in last_mag.iter().enumerate() {
            if m > peak.1 {
                peak = (k, m);
            }
        }
        let k = peak.0;
        let reassigned = k as f32 + last_df[k];
        let raw_err = (k as f32 - bin_target).abs();
        let re_err = (reassigned - bin_target).abs();
        assert!(
            re_err < 0.25,
            "reassigned bin {reassigned} vs target {bin_target} (err {re_err})"
        );
        assert!(
            re_err < raw_err,
            "reassignment not sharper: re_err {re_err} >= raw_err {raw_err}"
        );
    }

    #[test]
    fn time_offset_points_to_an_offgrid_impulse() {
        let n = 2048;
        let mut an = ReassignStft::new(n);
        let impulse_at = 3000usize; // off the 512-sample hop grid
        let total = 8192usize;
        let mut input = vec![0.0f32; total];
        input[impulse_at] = 1.0;

        let mut frames: Vec<(Vec<f32>, Vec<f32>)> = Vec::new();
        an.process(&input, |f| {
            frames.push((f.magnitudes.to_vec(), f.time_offsets.to_vec()))
        });
        assert!(!frames.is_empty(), "no frames produced");

        // Frame c (0-indexed) ends at sample hop*(c+1)-1; its window center is that minus (N-1)/2.
        let hop = n / 4;
        let center_of = |c: usize| -> f32 { (hop * (c + 1) - 1) as f32 - (n as f32 - 1.0) / 2.0 };

        // Highest-energy frame: the impulse is nearest its window center there.
        let best = frames
            .iter()
            .enumerate()
            .max_by(|a, b| {
                let sa: f32 = a.1.0.iter().sum();
                let sb: f32 = b.1.0.iter().sum();
                sa.partial_cmp(&sb).unwrap()
            })
            .map(|(i, _)| i)
            .unwrap();
        let (mags, dts) = &frames[best];
        let maxmag = mags.iter().copied().fold(0.0f32, f32::max);

        // Bins carrying real energy agree on the same reassigned time (consistency).
        let mut vals: Vec<f32> = mags
            .iter()
            .zip(dts)
            .filter(|(m, _)| **m > 0.5 * maxmag)
            .map(|(_, d)| *d)
            .collect();
        assert!(!vals.is_empty());
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let spread = vals.last().unwrap() - vals.first().unwrap();
        assert!(
            spread < 2.0,
            "dt not consistent across bins: spread {spread}"
        );

        let median = vals[vals.len() / 2];
        let reassigned_global = center_of(best) + median;
        assert!(
            (reassigned_global - impulse_at as f32).abs() < 2.0,
            "reassigned time {reassigned_global} vs impulse {impulse_at}"
        );
    }

    #[test]
    fn output_is_finite_and_silence_is_zero() {
        let n = 1024;
        let mut an = ReassignStft::new(n);

        // Deterministic pseudo-noise (LCG) — no wall-clock / RNG dependence.
        let mut state = 0x1234_5678u32;
        let mut noise = vec![0.0f32; n * 4];
        for s in noise.iter_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s = (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        }
        an.process(&noise, |f| {
            for k in 0..f.magnitudes.len() {
                assert!(f.magnitudes[k].is_finite(), "mag[{k}] not finite");
                assert!(f.freq_offsets[k].is_finite(), "df[{k}] not finite");
                assert!(f.time_offsets[k].is_finite(), "dt[{k}] not finite");
            }
        });

        let mut an2 = ReassignStft::new(n);
        let silence = vec![0.0f32; n * 4];
        an2.process(&silence, |f| {
            assert!(
                f.magnitudes.iter().all(|&v| v == 0.0),
                "silence has magnitude"
            );
            assert!(
                f.freq_offsets.iter().all(|&v| v == 0.0),
                "silence has freq offset"
            );
            assert!(
                f.time_offsets.iter().all(|&v| v == 0.0),
                "silence has time offset"
            );
        });
    }

    #[test]
    fn process_is_allocation_free() {
        let mut an = ReassignStft::new(1024);
        let buf = vec![0.1f32; 512];
        an.process(&buf, |_f| {}); // warm up outside the measured region
        lindelion_test_allocator::assert_no_allocations("reassign process", || {
            an.process(&buf, |_f| {});
        });
    }
}
