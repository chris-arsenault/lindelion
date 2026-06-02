# Reassignment STFT (`ReassignStft`)

Source: `crates/lindelion-dsp-utils/src/reassign.rs`

## 1. Purpose

`ReassignStft` is a forward-only streaming short-time Fourier analyzer that computes the **method of
reassignment**: per analysis frame it emits, for each frequency bin, the magnitude **and** the
time-frequency coordinates the bin's energy *should* be displayed at. Reassignment sharpens a
spectrogram — tones collapse onto their true frequency and transients onto their true time — by
correcting the smearing the STFT window introduces. It is Cenedril's single spectral tap: the
magnitude is a byproduct of the same analysis that produces the reassigned view
([cenedril.md](../plugins/cenedril.md), [ADR-0040](../adr/0040-cenedril-analysis-and-editor-delivery.md)).

It is **analysis only** — there is no resynthesis (distinct from the WOLA
[`StftProcessor`](../../crates/lindelion-dsp-utils/src/stft.rs)).

## 2. Theory

For a frame windowed by `h[m]`, the standard STFT is `X_h(k) = Σ_m x[m]·h[m]·e^{−j2πkm/N}`
(realfft's `e^{−j}` convention; frequency in rad/sample over `[0, π]`). The method of reassignment
(Auger & Flandrin 1995; Fulop & Fitz 2006) computes the reassigned coordinates from two auxiliary
transforms of the **same** frame under derived windows:

- `X_Th` — the **time-ramped** window `Th[m] = t_m · h[m]`, with `t_m = m − (N−1)/2` (samples from
  the window center).
- `X_Dh` — the window **derivative** `Dh[m] = h′[m]`.

Per bin `k`, guarding `|X_h(k)|² > ε`:

- **Frequency offset** (channelized instantaneous frequency), as a fractional-bin correction added to
  `k`:
  `freq_offset(k) = −(N / 2π) · Im( X_Dh(k) · conj(X_h(k)) / |X_h(k)|² )`.
  The reassigned bin is `k + freq_offset(k)`. (Identity: for a pure tone, `X_Dh/X_h ≈ jΩ`, so the
  imaginary part recovers the bin-to-true-frequency offset.)
- **Time offset** (local group delay), in samples relative to the window center:
  `time_offset(k) = Re( X_Th(k) · conj(X_h(k)) / |X_h(k)|² )`.
  The reassigned time is `window_center + time_offset(k)`. (For an impulse at frame position `m₀`,
  `X_Th/X_h = m₀ − (N−1)/2`, exactly the offset from center.)

The analysis window is a plain **Hann** (`window::hann_f64`); with no resynthesis the sqrt-Hann WOLA
uses is not required. The Hann derivative has the closed form
`h′[m] = (π/(N−1))·sin(2π m/(N−1))`.

## 3. Algorithm

Streaming, mirroring `StftProcessor`'s framing but forward-only (no inverse, no overlap-add). Hann,
time-ramp, and derivative windows are precomputed in `new`. Per hop (`hop = frame_size/4`, 75 %
overlap):

1. Read the frame oldest→newest from the input ring; window it three ways (`h`, `Th`, `Dh`).
2. Run three forward real FFTs → `spec_h`, `spec_th`, `spec_dh` (length `frame_size/2 + 1`).
3. Per bin, compute `magnitudes`, `freq_offsets`, `time_offsets` per §2; bins below the energy floor
   emit zero offsets (energy stays at its bin/center).
4. Invoke the caller closure with the three per-bin lanes (`ReassignFrame`).

All FFT scratch, the three windowed frames, the three spectra, and the output lanes are preallocated
in `new`, so `process` does not allocate (realfft `process_with_scratch`).

## 4. Parameters

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| `frame_size` | usize | samples | power of two | — | `ReassignStft::new(frame_size)`. Cenedril uses 2048. |
| `hop` | usize | samples | derived | `frame_size/4` | 75 % overlap; not separately settable. |
| `bins` | usize | count | derived | `frame_size/2 + 1` | Length of each emitted lane. |
| `ENERGY_EPS` | f32 | — | const | `1e-12` | `|X_h|²` floor below which offsets are zeroed. |

Output lanes (`ReassignFrame`): `magnitudes` (`|X_h|`), `freq_offsets` (fractional bins added to `k`),
`time_offsets` (samples relative to window center).

## 5. Response / verification

Reassignment correctness is pinned quantitatively by the `make ci` tests (the sign conventions in §2
are confirmed empirically there — a flipped sign moves energy the wrong way and fails):

- **Frequency sharpening.** A steady sine deliberately placed mid-way between bins 100 and 101
  reassigns to within < 0.25 bin of its true frequency — strictly closer than the raw bin center
  (`frequency_offset_sharpens_an_offgrid_sine`).
- **Time sharpening.** An off-grid impulse's bins agree (spread < 2 samples) on a reassigned time
  that lands on the impulse's true sample (`time_offset_points_to_an_offgrid_impulse`).
- **Bounded / finite.** Noise yields all-finite lanes; silence yields zero offsets
  (`output_is_finite_and_silence_is_zero`).
- **Display sharpening.** The `ReassignedSpectrogram` scatter concentrates a steady multi-sine into
  strictly fewer rows, and a transient into strictly fewer columns, than the magnitude model
  (`lindelion_ui::cenedril_vizia::reassigned::tests`).

A rendered magnitude-vs-reassigned chirp spectrogram is deferred to the doc-plots backlog; the tests
above are the current quantitative evidence.

## 6. Realtime contract

- **RT-callable:** `process` (audio thread, allocation-free) and `reset`. Cenedril calls `process`
  on its analysis tap ([ADR-0001](../adr/0001-allocation-free-audio-thread.md)).
- **Allocation-free:** all buffers/scratch preallocated in `new`; `process` allocates nothing —
  pinned by `lindelion_dsp_utils::reassign::tests::process_is_allocation_free`.
- `new` allocates (FFT plan, windows, buffers) and is not RT-callable.

## 7. Test coverage

`crates/lindelion-dsp-utils/src/reassign.rs`:

- `lindelion_dsp_utils::reassign::tests::frequency_offset_sharpens_an_offgrid_sine`
- `lindelion_dsp_utils::reassign::tests::time_offset_points_to_an_offgrid_impulse`
- `lindelion_dsp_utils::reassign::tests::output_is_finite_and_silence_is_zero`
- `lindelion_dsp_utils::reassign::tests::process_is_allocation_free`

Display-model scatter (`crates/lindelion-ui/src/cenedril_vizia/reassigned.rs`):

- `lindelion_ui::cenedril_vizia::reassigned::tests::frequency_reassignment_is_sharper_than_magnitude`
- `lindelion_ui::cenedril_vizia::reassigned::tests::time_reassignment_concentrates_a_transient_in_fewer_columns`
- `lindelion_ui::cenedril_vizia::reassigned::tests::silence_is_floor_and_output_is_bounded`

## 8. Usage example

Cenedril's audio-thread tap (one analysis feeds both spectrogram views):

```rust
self.reassign.process(&self.mono[..n], |frame| {
    ring.push_frame(frame.magnitudes, frame.freq_offsets, frame.time_offsets);
});
```

## 9. References

- P. Flandrin, F. Auger, É. Chassande-Mottin, "Time-Frequency Reassignment: From Principles to
  Algorithms" (and Auger & Flandrin, *IEEE Trans. Signal Processing*, 1995).
- S. W. Fulop & K. Fitz, "Algorithms for computing the time-corrected instantaneous frequency
  (reassigned) spectrogram," *J. Acoust. Soc. Am.* 119(1), 2006.
- [Cenedril spec](../plugins/cenedril.md) · [ADR-0040](../adr/0040-cenedril-analysis-and-editor-delivery.md).
