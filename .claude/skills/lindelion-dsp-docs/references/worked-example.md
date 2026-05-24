# Worked example — `OnePoleLowpass`

A complete DSP-module doc, presented exactly as it should appear in `docs/dsp/onepolelowpass.md`. Use this as a fill-in template when documenting a new module — start by copying this file and substituting names, equations, parameters, and citations.

---

# OnePoleLowpass

Single-pole IIR low-pass filter with sample-rate-aware cutoff clamping.

## 1. Purpose

First-order one-pole infinite-impulse-response low-pass filter. Implements a simple smoothing stage suitable for parameter ramps, denoising upstream signal paths, or rolling off high-frequency content before downstream processing.

## 2. Theory

**Difference equation**

$$y[n] = \alpha \cdot x[n] + (1 - \alpha) \cdot y[n-1]$$

where

$$\alpha = 1 - \exp\left(\frac{-2\pi \cdot f_c}{f_s}\right)$$

for cutoff `f_c` (Hz) and sample rate `f_s` (Hz).

**Transfer function**

$$H(z) = \frac{\alpha}{1 - (1 - \alpha) z^{-1}}$$

**Pole.** `p = 1 − α`, on the positive real axis. Stability requires `0 < α < 1`, satisfied for all valid `f_c ∈ (0, f_s/2)`.

**Discretization.** Matched-z (exponential mapping of the analog `1 / (1 + sτ)` prototype), not BLT. No prewarp is needed because the analog prototype is single-pole and the matched-z mapping preserves the pole location.

**Valid parameter range.** `1 Hz ≤ f_c ≤ 0.49 · f_s`. Clamped in `OnePoleLowpass::new` via `cutoff_for_sample_rate()`.

## 3. Algorithm

```rust
// One-pole low-pass: y[n] = α·x[n] + (1-α)·y[n-1]
let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate).exp();
let one_minus_alpha = 1.0 - alpha;
self.state = alpha * input + one_minus_alpha * self.state;
self.state = lindelion_dsp_utils::math::snap_to_zero(self.state);
self.state
```

## 4. Parameters

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| `cutoff_hz` | `f32` | Hz | `1.0 .. (fs · 0.49)` | `1000.0` | Clamped at construction via `cutoff_for_sample_rate()` in `lindelion-dsp-utils::filters` |

## 5. Response plots

- ![Magnitude response](../plots/onepolelowpass_mag.svg) — magnitude in dB on log-frequency axis, four cutoffs (100 Hz, 1 kHz, 5 kHz, 20 kHz) at `fs = 48 kHz`.
- ![Impulse response](../plots/onepolelowpass_impulse.svg) — exponential decay starting at α; settling time visible.

Plot data is committed under `docs/plots/data/onepolelowpass_*.csv` (see `crates/lindelion-dsp-utils/tests/freqz_export.rs`).

## 6. Realtime contract

- **Allocation.** Allocation-free; state is a single `f32` field on `Self`.
- **Denormals.** Flushed every sample via `lindelion_dsp_utils::math::snap_to_zero`.
- **Reset.** `reset()` zeros `self.state`. `set_cutoff(fc, fs)` recomputes `α` without allocation.
- **Thread safety.** `process()` and `set_cutoff()` are not safe to call concurrently; the host serializes them.
- **Bounded work.** O(1) per sample. No loops.
- **Finite output.** `lindelion_dsp_utils::analysis::assert_all_finite` covers a parameter sweep in tests.
- **SIMD.** Scalar. One-pole filters rarely warrant vectorization at the single-channel level; vector dispatch happens at the consumer's block-level wrapper.

## 7. Test coverage

- `lindelion_dsp_utils::filters::tests::one_pole_lowpass_attenuates_above_cutoff` — verify magnitude response at `f > f_c`.
- `lindelion_dsp_utils::filters::tests::one_pole_lowpass_passes_below_cutoff` — verify magnitude response at `f < f_c`.
- `lindelion_dsp_utils::filters::tests::one_pole_lowpass_stays_finite_across_parameter_sweep` — finiteness over `f_c ∈ [40, 110, 440, 1760, 4000, 20000] Hz`.

`OnePoleLowpass` has no audio-thread interface of its own; `assert_no_allocations!` coverage lives in the consuming plugin's audio-thread test (e.g., `lamath::dsp::engine::tests::note_on_and_render_do_not_allocate`).

## 8. Usage example

```rust
use lindelion_dsp_utils::filters::OnePoleLowpass;

let sample_rate = 48_000.0;
let mut lp = OnePoleLowpass::new(sample_rate, 1_000.0);

for sample in audio_block.iter_mut() {
    *sample = lp.process(*sample);
}
```

## 9. References

- Julius O. Smith, [Introduction to Digital Filters — One-Pole Low-Pass](https://ccrma.stanford.edu/~jos/filters/One_Pole_Lowpass.html).
- Source: `crates/lindelion-dsp-utils/src/filters.rs`.
- Workspace performance contract: `docs/performance.md`.
