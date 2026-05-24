# Biquad

Direct-Form I biquad with RBJ Audio EQ Cookbook coefficients. Low-pass, high-pass, and band-pass shapes.

## 1. Purpose

Second-order IIR filter implemented in Direct-Form I, fed by `BiquadCoefficients` produced from the Audio EQ Cookbook (Bristow-Johnson). Carries two input-history and two output-history samples per channel.

## 2. Theory

**Difference equation** (normalized so $a_0 = 1$, Smith/scipy sign convention):

$$y[n] = b_0 \cdot x[n] + b_1 \cdot x[n-1] + b_2 \cdot x[n-2] - a_1 \cdot y[n-1] - a_2 \cdot y[n-2]$$

**Transfer function**

$$H(z) = \frac{b_0 + b_1 z^{-1} + b_2 z^{-2}}{1 + a_1 z^{-1} + a_2 z^{-2}}$$

**Discretization.** Bilinear transform with prewarp. The cookbook formulas compute coefficients from analog prototypes by mapping the analog cutoff to the digital cutoff via $\omega = 2\pi f_c / f_s$ and the intermediate $\alpha = \sin(\omega)/(2Q)$, then dividing every coefficient by $a_0 = 1 + \alpha$.

**Topology.** Direct-Form I. State is two past inputs ($x_1, x_2$) and two past outputs ($y_1, y_2$). Direct-Form I is chosen over Transposed-II because the per-sample arithmetic count is identical and DFI is easier to reason about when modulating coefficients between samples — both history buffers are visible at the read site.

**Stability.** The cookbook formulas produce stable poles for any $Q \geq 0.05$ and $f_c \in (0, f_s/2)$. Q values below 0.05 are clamped up at coefficient generation time.

**Valid parameter range.** $f_c \in [20\,\mathrm{Hz},\, f_s \cdot 0.45]$, $Q \in [0.05, \infty)$. Both clamped inside `BiquadCoefficients::{lowpass, highpass, bandpass}`.

## 3. Algorithm

```rust
// Direct-Form I biquad with denormal flushing across all state.
let input = snap_to_zero(input);
self.x1 = snap_to_zero(self.x1);
self.x2 = snap_to_zero(self.x2);
self.y1 = snap_to_zero(self.y1);
self.y2 = snap_to_zero(self.y2);

let output = c.b0 * input + c.b1 * self.x1 + c.b2 * self.x2
           - c.a1 * self.y1 - c.a2 * self.y2;

self.x2 = self.x1;
self.x1 = input;
self.y2 = self.y1;
self.y1 = snap_to_zero(output);
self.y1
```

## 4. Parameters

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| `sample_rate` | `f32` | Hz | $\geq 1000$ (else 48000) | — | Validated at coefficient generation |
| `cutoff_hz` | `f32` | Hz | $[20,\ f_s \cdot 0.45]$ | — | Clamped at coefficient generation |
| `q` | `f32` | dimensionless | $\geq 0.05$ | 0.707 (Butterworth) | Clamped at coefficient generation |

Coefficient flavors:

| Flavor | Magnitude at DC | Magnitude at Nyquist | Behavior |
| ---- | ---- | ---- | ---- |
| `lowpass` | unity | $-\infty\,\mathrm{dB}$ | $-12\,\mathrm{dB/oct}$ rolloff above $f_c$ |
| `highpass` | $-\infty\,\mathrm{dB}$ | unity | $-12\,\mathrm{dB/oct}$ rolloff below $f_c$ |
| `bandpass` | 0 | 0 | Peak at $f_c$, bandwidth controlled by $Q$ |

## 5. Response plots

| Plot | Status |
| ---- | ---- |
| Magnitude (dB) on log frequency, three flavors at $Q = 0.707$ | Pending plot-data infrastructure |
| Phase (degrees) on log frequency | Pending |
| Pole-zero on the unit circle for each flavor | Pending |
| Impulse response, three flavors | Pending |

The expected pole-zero for the low-pass: two complex-conjugate poles inside the unit circle (angle $\omega = 2\pi f_c / f_s$, radius determined by $Q$); two zeros at $z = -1$ (DC mirror).

## 6. Realtime contract

- **Allocation.** Allocation-free; state is five `f32` fields plus a `BiquadCoefficients` (also five `f32`). No heap.
- **Denormals.** All five state slots flushed each sample via `snap_to_zero`; the output is flushed before assignment to `y1`.
- **Reset.** `reset()` zeros `x1`, `x2`, `y1`, `y2`. `set_coefficients()` swaps the coefficient block without allocation; existing state continues with the new shape (a click is possible — consumers should smooth or schedule swaps on note boundaries when audible).
- **Thread safety.** `process()` and `set_coefficients()` are not safe to call concurrently.
- **Bounded work.** $O(1)$ per sample; 5 multiplies and 4 adds per sample.
- **Finite output.** Five `snap_to_zero` calls per sample bound the worst case from non-finite inputs or coefficients.
- **SIMD.** Scalar. Vectorization is appropriate when running independent channels in parallel; not done at this level.

## 7. Test coverage

- `lindelion_dsp_utils::filters::tests::lowpass_reduces_high_frequency_more_than_low_frequency` — verifies $-20\,\mathrm{dB}$ separation between a 250 Hz tone and an 8 kHz tone after 1 kHz low-pass.

## 8. Usage example

```rust
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};

let sample_rate = 48_000.0;
let coefficients = BiquadCoefficients::lowpass(sample_rate, 1_000.0, 0.707);
let mut filter = Biquad::new(coefficients);

for sample in audio_block.iter_mut() {
    *sample = filter.process(*sample);
}
```

Updating the cutoff mid-stream:

```rust
filter.set_coefficients(BiquadCoefficients::lowpass(sample_rate, 5_000.0, 0.707));
```

## 9. References

- Robert Bristow-Johnson — [Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/) (W3C TR).
- Julius O. Smith — [Introduction to Digital Filters: Direct-Form I](https://ccrma.stanford.edu/~jos/filters/Direct_Form_I.html).
- Source: [`crates/lindelion-dsp-utils/src/filters.rs`](../../crates/lindelion-dsp-utils/src/filters.rs).
- Sibling module: SVF (state-variable filter) in the same source file is documented separately.
- ADR-0001: [Allocation-free audio thread](../adr/0001-allocation-free-audio-thread.md).
