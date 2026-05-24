# ModalBank

Bank of $N$ parallel second-order resonant filters, one per vibrational mode of a struck or blown idiophone.

## 1. Purpose

Polyphonic-friendly modal resonator. Excitation samples (breath transients, plucks, mallet strikes, key clicks) drive a bank of independent second-order resonators; the sum is the synthesized tone. Mode frequencies, decays, and gains derive from a chosen preset (Marimba, Kalimba, Bell, Glass Bowl, Metal Bar, Woodblock, GenericStrike) and are shaped by inharmonicity, brightness, decay, decay-tilt, and strike-position controls.

## 2. Theory

**Per-mode resonator (one `ModalMode`).** Each mode is a second-order IIR with complex-conjugate poles inside the unit circle:

$$y[n] = g \cdot x[n] + (2r \cos\omega) \cdot y[n-1] - r^2 \cdot y[n-2]$$

where

$$\omega = \frac{2\pi f_\mathrm{mode}}{f_s}, \quad r = \exp\left(\frac{-1}{T_\mathrm{decay} \cdot f_s}\right)$$

$f_\mathrm{mode}$ is the mode frequency, $T_\mathrm{decay}$ is the $1/e$ decay time, $g$ is the per-mode gain, and $r$ is the pole radius. The pole pair sits on a circle of radius $r$ at angle $\pm\omega$, giving exponential decay with envelope $r^n$ and an oscillation at $f_\mathrm{mode}$.

**Bank output**

$$y_\mathrm{bank}[n] = \frac{1}{N}\sum_{k=1}^{N} y_k[n]$$

where $N$ is the active mode count after Nyquist filtering. The $1/N$ scale keeps perceived loudness roughly constant as mode count changes.

**Mode frequency derivation.**

$$f_k = f_\mathrm{fundamental} \cdot r_k \cdot \mathrm{stretch}_k$$

$$\mathrm{stretch}_k = 1 + \mathit{inharmonicity} \cdot \left(\frac{k}{N}\right)^2$$

$r_k$ is the preset's mode ratio (special ratios for the first few modes, then $r_k = (k+1)^h$ for the generic tail where $h$ is the preset's harmonicity exponent).

**Per-mode gain.**

$$g_k = g_\mathrm{template} \cdot \big|\sin(\pi \cdot (k+1) \cdot p)\big| \cdot 10^{(b - 0.5) \cdot u_k \cdot 2}$$

where $g_\mathrm{template}$ is the preset's gain envelope, $p \in [0, 1]$ is the strike position, $b$ is brightness, and $u_k = k/(N-1)$ is the normalized mode index. The $\big|\sin(\cdot)\big|$ term is the spatial excitation factor for a strike at fractional position $p$ along the resonator (nodes at $p = k/n$ for mode $k$ are silenced).

**Per-mode decay.**

$$T_{\mathrm{decay},k} = T_\mathrm{template} \cdot \mathit{decay\_global} \cdot (1 - 0.75 \cdot u_k \cdot \mathit{decay\_tilt})$$

Decay tilt lets higher modes die faster than lower modes, modelling the natural dissipation of high-frequency energy.

**Nyquist filter.** Modes with $f_k \geq 0.95 \cdot f_s/2$ are skipped at `configure()` time, never allocated into the bank.

**Stability.** $r < 1$ for any $T_\mathrm{decay} > 0$; the pole sits strictly inside the unit circle. Decay is clamped to $\geq 0.001\,\mathrm{s}$.

## 3. Algorithm

```rust
// One mode (second-order resonator with denormal flush on state).
let output = input * self.gain + self.coefficient * self.y1
           - self.radius_squared * self.y2;
self.y2 = self.y1;
self.y1 = snap_to_zero(output);
self.y1
```

```rust
// Bank: sum N modes, scale by 1/N.
let sum: f32 = self.modes.iter_mut()
    .map(|mode| mode.process_sample(input))
    .sum();
snap_to_zero(sum * self.output_scale)
```

## 4. Parameters

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| `fundamental_hz` | `f32` | Hz | clamped per mode to $f_s \cdot 0.475$ | 220 | Each mode's $f_k$ is independently clamped |
| `mode_count` | `usize` | modes | $[1, 256]$ | 64 | Hard cap; modes near Nyquist are skipped at configure |
| `preset` | `ModalPreset` | enum | 7 variants | `Marimba` | Kalimba, Marimba, Bell, GlassBowl, MetalBar, Woodblock, GenericStrike |
| `inharmonicity` | `f32` | dimensionless | $[0, \infty)$ | 0.0 | Adds quadratic stretch to mode ratios |
| `brightness` | `f32` | 0..1 | clamped to $[0, 1]$ | 0.5 | Higher modes louder above 0.5, quieter below |
| `decay_global` | `f32` | dimensionless | $\geq 0.01$ | 1.0 | Multiplier on all mode decay times |
| `decay_tilt` | `f32` | 0..1 | clamped to $[0, 1]$ | 0.5 | Higher modes decay faster as this rises |
| `position_of_strike` | `f32` | 0..1 | clamped per `STRIKE_POSITION` | preset-defined | Spatial excitation factor (nodes at $p = k/n$) |

**Presets** (`lamath::ModalPreset`):

| Preset | Harmonicity | Base decay (s) | Special first ratios |
| ---- | ---- | ---- | ---- |
| `Kalimba` | 1.35 | 1.4 | 1.0, 2.76, 5.4, 8.93, 13.34 |
| `Marimba` | 1.9 | 0.9 | 1.0, 3.99, 10.65 |
| `Bell` | 1.45 | 3.5 | 0.56, 0.92, 1.19, 1.71, 2.0, 2.74, 3.0, 3.76 |
| `GlassBowl` | 1.18 | 4.0 | 1.0, 1.72, 2.41, 3.16, 4.02, 5.35 |
| `MetalBar` | 1.75 | 2.2 | 1.0, 2.76, 5.4, 8.93 |
| `Woodblock` | 2.2 | 0.25 | 1.0, 2.1, 3.9, 6.8 |
| `GenericStrike` | 1.0 | 1.0 | (no special ratios; pure harmonic series) |

## 5. Response plots

| Plot | Status |
| ---- | ---- |
| Impulse response, 8 modes at $f_0 = 220\,\mathrm{Hz}$, Marimba preset | Pending plot-data infrastructure |
| Magnitude spectrum at $f_s = 48\,\mathrm{kHz}$, GenericStrike preset | Pending |
| Parameter sweep over fundamental $\in \{40, 110, 440, 1760, 4000\}\,\mathrm{Hz}$ at Bell preset | Pending |
| Brightness sweep ($b \in \{0.1, 0.3, 0.5, 0.7, 0.9\}$) | Pending |
| Strike-position sweep ($p \in \{0.1, 0.25, 0.5, 0.75\}$) | Pending |

## 6. Realtime contract

- **Allocation.** `with_capacity(sample_rate, max_modes, params)` preallocates the `Vec<ModalMode>` capacity once. `configure()` and `retune()` mutate in place; `process_sample()` allocates nothing. `MAX_MODE_COUNT = 256`.
- **Denormals.** Each `ModalMode` flushes `y1` post-update via `snap_to_zero`. The bank output is also `snap_to_zero`-flushed after the $1/N$ scale.
- **Reset.** `reset()` zeros every mode's `y1`/`y2`. `configure()` rebuilds the mode list (clearing previous state); use `retune()` instead when changing fundamental/preset/brightness without resetting ring-down.
- **Thread safety.** `process_sample`, `configure`, `retune`, and `reset` are not safe to call concurrently. The host serializes them at the voice level.
- **Bounded work.** $O(N)$ per sample where $N$ is the active mode count after Nyquist filtering. Worst case 256 modes per sample.
- **Finite output.** `snap_to_zero` covers both per-mode state and bank output. Decays clamped to $\geq 0.001\,\mathrm{s}$ keep $r < 1$. Frequency clamped per mode to $f_s \cdot 0.49$ keeps $\omega$ valid.
- **SIMD.** Scalar today. Per-mode arithmetic is the obvious vectorization target if profiling indicates the bank is the bottleneck.

## 7. Test coverage

- `lamath::dsp::modal::tests::single_mode_impulse_rings_near_configured_frequency` — feeds a unit impulse into one `ModalMode` at 440 Hz, estimates ringing frequency from zero-crossings, asserts within 5 Hz.
- `lamath::dsp::modal::tests::modal_bank_has_energy_at_fundamental` — feeds an impulse into an 8-mode `ModalBank` at 220 Hz, asserts non-trivial DFT magnitude at the fundamental.
- `lamath::dsp::modal::tests::modal_bank_stays_finite_across_parameter_sweep` — sweeps fundamental over $\{40, 110, 440, 1760, 4000\}\,\mathrm{Hz}$ at Bell preset with extreme parameter combinations; asserts finite output and peak $< 20$.

## 8. Usage example

```rust
use lamath::{ModalPreset, dsp::modal::{ModalBank, ModalBankParams}};

let sample_rate = 48_000.0;
let mut bank = ModalBank::with_capacity(sample_rate, 64, ModalBankParams {
    fundamental_hz: 220.0,
    mode_count: 32,
    preset: ModalPreset::Marimba,
    inharmonicity: 0.05,
    brightness: 0.6,
    decay_global: 1.2,
    decay_tilt: 0.4,
    position_of_strike: 0.21,
});

// Strike with a unit impulse, then render the ring-down.
let mut output = vec![0.0; 8192];
output[0] = bank.process_sample(1.0);
for sample in output[1..].iter_mut() {
    *sample = bank.process_sample(0.0);
}
```

## 9. References

- Julius O. Smith — [Physical Audio Signal Processing: Modal Synthesis](https://ccrma.stanford.edu/~jos/pasp/Modal_Synthesis.html).
- Perry Cook — *Real Sound Synthesis for Interactive Applications* (A K Peters, 2002), chapters on physical modeling of bars and bells.
- Source: [`plugins/lamath/src/dsp/modal.rs`](../../plugins/lamath/src/dsp/modal.rs).
- ADR-0001: [Allocation-free audio thread](../adr/0001-allocation-free-audio-thread.md).
- Related: WaveguideResonator in [`plugins/lamath/src/dsp/waveguide.rs`](../../plugins/lamath/src/dsp/waveguide.rs) (separate doc pending).
