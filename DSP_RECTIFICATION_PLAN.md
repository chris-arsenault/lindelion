# DSP Rectification Plan

Enumerated, single-file-at-a-time path to resolve every finding from the DSP
algorithm review (filters, resampling, modal, waveguide, pitch detection, onset
detection, pitch shifting, expression/MIDI). Items are ordered so each can be
approached independently; later items do not depend on earlier ones except where
noted. Each step states the location, the defect, the reference-correct fix, and
the verification to add.

**Conventions**
- Severity: Critical / Major / Minor / Nit.
- "Verified" = confirmed line-by-line against source during review.
- Run `make ci` after each step. Where a step says "add test", the test must fail
  on the current code and pass after the fix.
- Do not change a named DSP algorithm's semantics without explicit approval
  (AGENTS.md: required DSP algorithms are product requirements). Steps marked
  **[DECISION]** need a product call before implementing.

---

## Phase 1 — Confirmed programming error (do first)

### 1. Fix `FirstOrderAllpass` — not an allpass `[Critical, verified]`
- **File:** `crates/lindelion-dsp-utils/src/delay.rs:108-110`
- **Defect:** Output line computes `c*(input - z1) + z1 = c*x + (1-c)*z1`, giving
  `H(z) = (c + (1-c)z⁻¹)/(1 + c(1-c)z⁻¹)` — magnitude **not** flat (≈0.43–0.86).
- **Fix:** Replace the two output lines with the standard one-multiply allpass:
  ```rust
  let output = coefficient * input + self.z1;
  self.z1 = math::snap_to_zero(input - coefficient * output);
  ```
  (State-update line is already correct; only the output expression changes.)
- **Verify:** Add a magnitude-flatness test — sweep frequencies, assert
  `|H(e^{jω})| == 1.0 ± 1e-6` for several coefficients. Add an impulse-response
  test pinning the corrected response so it cannot regress silently.
- **Blast radius:** This filter is the waveguide fractional-delay tuner
  (`waveguide.rs:71`) and the dispersion building block (`dispersion.rs:33-34`).
  Steps 6 and 7 depend on this being fixed first.

---

## Phase 2 — Algorithm shortcuts / mislabeling

Each of these implements a simpler approximation than the named reference. Some
are bugs; some are **[DECISION]** points about whether the simpler form is
acceptable.

### 2. SwiftF0 resampler aliases on downsample `[Major]`
- **File:** `crates/lindelion-pitch-detect/src/swiftf0.rs:181, 323-338`
- **Defect:** Resampling source audio to the model's 16 kHz uses
  `interpolation::linear` with no anti-alias filter. Downsampling (e.g. 48k→16k)
  folds content above 8 kHz into the spectrogram the CNN consumes (measured:
  10 kHz tone → full-magnitude 6 kHz alias).
- **Fix:** Route the 16 kHz conversion through the existing band-limited
  windowed-sinc resampler in `crates/lindelion-dsp-utils/src/resampling.rs`
  instead of linear interpolation. No new algorithm required — reuse what exists.
- **Verify:** Add a test that downsamples a signal containing a >8 kHz tone and
  asserts the alias band is attenuated (current code fails this).

### 3. Spectral-flux onset threshold is global, not adaptive `[Major]` `[DECISION]`
- **File:** `crates/lindelion-onset-detect/src/spectral_flux.rs:548-564`
- **Defect:** Peak-picking uses a single global `mean + k*std` threshold over the
  whole novelty function. Reference (Böck) uses a moving local median/mean window
  plus a delta offset, so the threshold tracks the local noise floor. Global
  threshold under/over-detects on non-stationary loudness.
- **Fix:** Replace `flux_peak_threshold` with a moving-window local threshold
  (e.g. local mean/median over ±N frames + delta). Keep the existing local-max
  (`lookback_peak`) and min-gap logic.
- **Verify:** Test with a signal whose loudness rises across its length; assert
  onsets are detected in both the quiet and loud regions.
- **[DECISION]:** Confirm the moving-window length / delta defaults before coding.

### 4. Batch vs streaming onset thresholds diverge `[Major]`
- **File:** `crates/lindelion-onset-detect/src/spectral_flux.rs:286` (streaming)
  vs `:523` (batch)
- **Defect:** Batch path thresholds *normalized* flux (`normalize_flux` →
  `[0,1]`); streaming path thresholds *un-normalized* growing history. The two
  detectors produce different onsets on identical audio.
- **Fix:** Make both paths threshold the same domain. Fold into step 3 (define one
  threshold routine and call it from both paths over the same units).
- **Verify:** Feed identical audio through batch and streaming; assert onset sets
  match within a small tolerance.
- **Note:** Do steps 3 and 4 together — they share the threshold routine.

### 5. `complex_flux` double-counts the magnitude term `[Minor]`
- **File:** `crates/lindelion-onset-detect/src/spectral_flux.rs:411`
- **Defect:** Adds half-wave-rectified magnitude flux *plus* the complex-domain
  distance (which already incorporates magnitude), double-counting magnitude vs
  the canonical complex ODF (Bello et al.).
- **Fix:** Use the complex-domain distance alone (optionally half-wave rectified
  by `|X_t| >= |X_{t-1}|`), or drop the separate magnitude term.
- **Verify:** Unit test the ODF value on a synthetic frame pair against the
  reference complex-flux formula.

---

## Phase 3 — Waveguide (depends on step 1)

### 6. Dispersion cascade is not unity-gain `[Major]` `[depends on #1]`
- **File:** `plugins/lamath/src/dsp/waveguide/dispersion.rs:50-60, 74-80`
- **Defect:** With the non-allpass `FirstOrderAllpass` and negative coefficients,
  the cascade peaks ~+12 dB at the string fundamental — inside the feedback loop,
  uncompensated (`core.rs` `loop_damping` only compensates the biquad peak).
- **Fix:** First land step 1 (makes the sections true unity-gain allpasses). Then
  re-verify the cascade magnitude is flat (allpass redistributes phase only).
  Confirm `delay_compensation_samples` (dispersion.rs:93-107), which already uses
  the *ideal* allpass group-delay formula, now matches the filter actually running.
- **Verify:** Add a cascade-magnitude test (`|H| == 1 ± tol` across band) and a
  loop-stability test at high `loop_gain` + max dispersion asserting bounded,
  decaying output (tighten the existing `peak_abs < 10.0` bound).

### 7. Production String is a single-delay comb, not two-rail `[Major]` `[DECISION]`
- **File:** `plugins/lamath/src/dsp/waveguide.rs:16-17, 107-149`
- **Defect:** `WaveguideResonator` String branch is a Karplus-Strong single-delay
  loop; the real two-rail `String1d` (correct inverting reflections, left/right
  rails, physical pickup/strike positions) is `#[cfg(test)]` — dead in production.
  "Pickup"/"strike position" on the comb only produce comb coloration / onset
  timing, not two-rail position behavior. (The Tube path *does* use two rails.)
- **[DECISION]:** Decide whether String must use the physical two-rail model
  (then wire `String1d` into production via `TravelingWavePair`, matching the Tube
  path) or whether the comb is acceptable (then stop describing it as two-rail and
  remove/relabel the dead `String1d`).
- **Verify (if wiring in two-rail):** Pitch-tracking test (autocorrelation within
  cents tolerance), and a pickup-position test showing the expected comb-notch
  pattern shifts with position.

### 8. String waveguide frequency-sanitizer mismatch `[Minor]`
- **File:** `plugins/lamath/src/dsp/waveguide/core.rs:94-100` vs `:155-157`
- **Defect:** `delay_tuning` clamps frequency to `[min_frequency, sr*0.45]`;
  `tuned_frequency` uses a fixed `220.0` fallback and `[1.0, sr*0.45]`. The
  filter-delay compensation can use a slightly different frequency than the delay
  for very low notes near buffer capacity.
- **Fix:** Use one shared frequency-sanitizing helper for both the delay length
  and the filter-delay compensation.
- **Verify:** Test a low note near capacity asserting tuned output frequency
  matches target within cents tolerance.

### 9. `string_1d` loop filter applied twice per round trip `[Minor]`
- **File:** `plugins/lamath/src/dsp/waveguide/string_1d.rs:128` (both terminations)
- **Defect:** Loop filter applied at both terminations (twice per round trip)
  while gain compensation assumes once → decay slightly darker/faster than the
  requested T60. Reference STK applies the loop filter once per round trip.
- **Fix:** Apply the loop filter at one termination only (or halve its effect),
  matching how `filter_delay_samples` is measured.
- **Verify:** Measure T60 of the rendered decay against the requested decay time.
- **Note:** Only relevant if step 7 wires `String1d` into production.

### 10. Tube default open-end reflection polarity `[Minor]` `[DECISION]`
- **File:** `plugins/lamath/src/dsp/waveguide/tube_1d.rs` (`TUBE_BOUNDARY.reflection.default = +0.75`)
- **Defect:** Open-end pressure reflection should invert (≈ −1); default is
  positive/non-inverting. User-parameter-driven, so a modeling choice, but the
  default is not the textbook open tube.
- **[DECISION]:** Confirm whether the non-inverting default is intentional voicing.
  If not, set the default reflection negative for the open end.
- **Verify:** If changed, re-pin the tube tuning/timbre tests.

---

## Phase 4 — Modal resonator

### 11. Per-mode peak gain not normalized for pole radius `[Major]` `[verified]` `[DECISION]`
- **File:** `plugins/lamath/src/dsp/modal.rs:88-106, 209`
- **Defect:** The two-pole resonator is correct, but peak gain at resonance
  ∝ `1/(1-r)`, so modes with equal nominal `gain` but different `decay` ring at
  very different amplitudes. The `1/mode_count` output scale addresses mode count,
  not per-mode resonant gain.
- **Fix:** Feed `input * gain * (1 - r)` (unity-peak bandpass normalization), or
  fold `(1 - r)` into the stored `gain` in `new`/`retune`. Apply consistently in
  `process_sample`, `new`, and `retune`.
- **[DECISION]:** Confirm this matches intended voicing — it will change relative
  mode levels (long-decay modes get quieter relative to short-decay modes).
- **Verify:** Test two modes with equal `gain` but different `decay`; assert their
  steady-state resonant peak amplitudes match within tolerance after the fix.

### 12. `retune` clamps out-of-band modes; `configure` skips them `[Minor]`
- **File:** `plugins/lamath/src/dsp/modal.rs:73-76` vs `:118-120`
- **Defect:** `configure` drops modes reaching `nyquist*0.95`; `retune` clamps
  them to `nyquist*0.95`, piling multiple modes onto one near-Nyquist frequency
  (possible loud resonance) and creating a mode-count mismatch that forces full
  reconfigure.
- **Fix:** Make `retune` skip/silence out-of-band modes the same way `configure`
  does (or keep a stable mode set and zero their gain when out of band).
- **Verify:** Test a pitch shift that pushes modes out of band; assert no
  near-Nyquist pileup and stable mode count.

### 13. Bell preset partial series discontinuous past 8 modes `[Minor]`
- **File:** `plugins/lamath/src/dsp/modal.rs:278-283` (and `:239-244`)
- **Defect:** `special_ratios` end at 3.76; the `(index+1)^harmonicity` fallback
  jumps to ~24.6 at index 8 — a large gap in the partial series for
  `mode_count > 8`.
- **Fix:** Extend the special-ratio table or make the fallback continue smoothly
  from the last special ratio (e.g. scale the harmonic series to start near 3.76).
- **Verify:** Assert the Bell partial series is monotonic / has no large gap for
  `mode_count > 8`.

---

## Phase 5 — Pitch shift

### 14. PSOLA is normalized averaging with 3-period grains `[Major]` `[DECISION]`
- **File:** `crates/lindelion-pitch-shift/src/pitch_synchronous_synthesis.rs:60-103`
- **Defect:** Named PSOLA but implemented as normalized weighted *averaging*
  (`weighted_sum / weight_sum`), not additive COLA-OLA, with grains ~3 periods
  wide (`radius = max(1.5*src, 0.75*tgt)`) instead of 2-period Hann grains.
  Averaging suppresses constructive summation; wide grains blur period detail.
- **Fix (reference PSOLA):** Use 2-period Hann grains (`radius = source_period`)
  and additive overlap-add whose epoch spacing satisfies COLA — no per-sample
  renormalization.
- **[DECISION]:** Confirm whether textbook PSOLA fidelity is required here, or
  whether the current normalized-grain interpolator is acceptable (then relabel).
- **Verify:** Test that a steady voiced tone keeps flat amplitude (COLA) and that
  pitch-shifted output preserves formant/period structure.

### 15. Epoch placement is zero-crossing, not GCI/energy-peak `[Minor]`
- **File:** `crates/lindelion-pitch-shift/src/analyzer.rs:194-235, 265-283`
- **Defect:** Pitch marks placed at strongest positive zero crossing near the
  period grid; reference PSOLA places them at glottal closure instants / waveform
  energy peaks (~quarter-period offset).
- **Fix:** Place epochs at local energy peaks near the expected period grid.
- **Verify:** Test epoch positions land on waveform peaks for a synthetic glottal
  pulse train.
- **Note:** Lower priority; bundle with step 14 if reworking PSOLA.

### 16. Spectral-peak path mislabeled as phase vocoder `[Major-label]`
- **File:** `crates/lindelion-pitch-shift/src/spectral_peak_synthesis.rs:30-74`
  and `synthesis.rs:voiced_harmonic_sample`
- **Defect:** This is additive sinusoidal/peak resynthesis (phase-intercept math
  is correct), not a phase vocoder. No numeric bug — only a labeling/expectation
  issue. The true phase vocoder lives in `resample_pro_*` and is correct.
- **Fix:** Rename/document this path as sinusoidal/peak resynthesis so it is not
  confused with the phase vocoder. No algorithm change.
- **Verify:** Documentation/naming only.

### 17. Harmonic fallback resets phase per region (clicks) `[Minor]`
- **File:** `crates/lindelion-pitch-shift/src/synthesis.rs:544` (region-relative
  `offset_samples`, reset in `render_region_to:397-409`)
- **Defect:** Each region starts every harmonic at phase 0 → phase discontinuity
  / audible click at slice seams. The spectral-peak path avoids this via absolute
  position.
- **Fix:** Drive harmonic phase from absolute sample position (or carry a
  per-harmonic phase intercept across regions), as the peak path does.
- **Verify:** Render adjacent regions of a sustained tone; assert no discontinuity
  at the seam (sample-difference below click threshold).

### 18. Spectral envelope is RMS box-smoothing, not cepstral/LPC `[Minor]` `[RESOLVED — keep RMS box]`
- **File:** `crates/lindelion-pitch-shift/src/spectral.rs`
- **Original finding:** Formant-preservation envelope is moving-RMS magnitude
  smoothing, not cepstral liftering or LPC. Applied in the correct linear-magnitude
  domain (no domain error) — just a cruder estimator than the named reference. Not a
  bug.
- **Outcome:** A full cepstral-liftering replacement was implemented and is more
  accurate at locating formants *in isolation* (peak 941 Hz vs a 1000 Hz formant),
  but integrating it into the shared `spectral_envelope_formant_gain` path (used by
  both the spectral-peak and `resample_pro` synthesis) revealed a **fundamental
  accuracy-vs-residual conflict** that cannot satisfy the existing synthesis
  correctness thresholds simultaneously:
  - cepstral, 256 envelope points (the analysis clamp ceiling), hard lifter →
    spectral-peak shifted-fundamental dominance 4.08× (barely passes `>4×`) but
    `resample_pro` pure-tone residual −38.7 dB (fails `<−45 dB`);
  - adding the lifter taper needed to bring the residual below −45 dB smooths the
    envelope enough to drop the dominance below 4×.
  The envelope's formant sharpness and a low pure-tone resynthesis residual pull
  against each other, and `envelope_points` is already pinned at its 256 clamp.
- **Decision:** Keep the RMS-box estimator (it satisfies all synthesis thresholds).
  Reverted the cepstral work. A clean cepstral/LPC swap is **blocked** on this
  conflict; it would require either accepting a relaxed synthesis threshold (decide
  which quality dimension dominates) or a larger rework (raise the `envelope_points`
  clamp above 256 and decouple the formant gain from envelope sharpness). The
  RMS-box envelope stands as the pragmatic, working estimator.

---

## Phase 6 — MIDI / housekeeping (low risk)

### 19. SMF tempo truncation `[Minor]`
- **File:** `crates/lindelion-midi/src/lib.rs:229`
- **Defect:** `60_000_000u32 / bpm` truncates instead of rounding; BPM is already
  `u16`-rounded upstream (`:331`). Sub-µs/qn error, inaudible.
- **Fix:** Round: `((60_000_000.0 / bpm as f64).round()) as u32`. Optionally widen
  `MidiClip` BPM beyond `u16` if precise tempo matters (larger change — defer).
- **Verify:** Unit test tempo value for a non-divisor BPM.

### 20. `nearest_scale_degree` downward tie bias `[Nit]`
- **File:** `crates/lindelion-midi/src/lib.rs:422-446` (strict `<` at `:439`)
- **Defect:** Exact ties (pitch midway between two scale degrees) always snap down;
  `best_note` initializer can hold an out-of-scale note (harmless — always
  overwritten within ±24).
- **Fix:** Define and document the tie rule (round half up, or nearest-even);
  initialize `best_note` from the first in-scale candidate.
- **Verify:** Unit test a pitch exactly between two scale degrees.

### 21. Dead OLA-normalization field `[Nit]`
- **File:** `crates/lindelion-pitch-shift/src/resample_pro_analysis.rs:16-19`
- **Defect:** `window_ola_normalization` (`steady_state_squared_window_sum`) is
  computed but never used in synthesis (runtime path normalizes via
  `accumulate_squared_window_at`). Dead state, not a bug.
- **Fix:** Remove the unused field/computation.
- **Verify:** `make ci` (compile + tests).

---

## Not defects (recorded so they are not re-flagged)

- `filters.rs` (RBJ biquads, one-pole, SVF), `interpolation.rs`, `smoothing.rs`,
  `envelope.rs`, `math.rs`, `window.rs`, `phase.rs`, `ola.rs` — correct.
- `resampling.rs` windowed-sinc resampler — correct (unity-gain kernel,
  Blackman–Harris, Nyquist-scaled cutoff). *This is the resampler step 2 should
  reuse.*
- `analysis.rs` / `measurements.rs` / `artifact.rs` — correct single-bin DFT
  probes.
- The real phase vocoder: `resample_pro_analysis.rs` (expected-phase term,
  principal-arg wrapping, IF), `resample_pro_stretch.rs` (Laroche–Dolson peak
  phase-locking, DC/Nyquist handling), `resample_pro_render.rs` (stretch+resample)
  — correct.
- `varispeed_synthesis.rs` — honest varispeed, correctly labeled.
- The two-pole modal resonator coefficients (radius, ω, difference equation) —
  correct (the only gap is per-mode gain normalization, step 11).
- SwiftF0 decoding (bin→Hz, fmin/fmax, frame/hop, no-normalization, confidence,
  timestamp offset) — correct (only the resampler, step 2, is wrong).
- Loudness (honest RMS, not LUFS), note conversion (`69 + 12*log2(f/440)`, cents,
  velocity clamping), PPQ/tick, samples→beats, dB factors (20*log10 on amplitude)
  — correct.

---

## Suggested execution order

1, then 6 (waveguide dispersion depends on the allpass fix), then the
**[DECISION]** items (3+4, 7, 10, 11, 14, 18) once product calls are made, then
the remaining Minor/Nit items (2, 5, 8, 9, 12, 13, 15, 16, 17, 19, 20, 21) in any
order. Run `make ci` after each numbered step.
