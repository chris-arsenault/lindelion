# Speech-chain + Galad bug fixes (working tracker)

Findings from the 2026-07-02 review of the Calóma speech effects and the Galad host.
Status: all fixed; verification below.

## Calóma / speech DSP

- [x] **B1 — `soft_clip` small-signal gain equals `drive`** (`crates/lindelion-dsp-utils/src/saturation.rs`)
  `tanh(k·x)` had small-signal gain `k = drive·(1±asym)`: every saturation stage was also a volume
  knob, with a discontinuity at drive→0 (drive ε ≈ −26 dB vs. exact 0 = passthrough) and
  asymmetric *linear* gain per half-wave. Fixed: normalized to `tanh(k·x)/k` per half — unity
  small-signal gain at any drive, continuous at 0, asymmetry affects curvature only. New unit test
  `small_signal_gain_is_unity_for_any_drive_and_asymmetry`. Affected users now level-neutral:
  `speech/saturation` (Warmth was ±14 dB of gain), `speech/vitalizer` (+6 dB at default),
  `speech/air-exciter`, `speech/bass-enhancer` (band boost now bounded by Amount, guarded by
  `band_boost_is_bounded_by_amount`).

- [x] **B2 — Limiter was not a brickwall and stepped its gain** (`speech/limiter/src/lib.rs`)
  The envelope released while the peak was still inside the lookahead delay (transients overshot
  the ceiling ~+1.3 dB at release 10 ms) and instant attack applied hard gain steps. Rewritten:
  per-sample target gain → release-smoothed envelope (instant down, one-pole up) → sliding-window
  minimum over the lookahead (monotonic deque) → box average over the lookahead. Every averaged
  term had the emerging sample in its min window, so applied gain ≤ that sample's target —
  provably at/under the ceiling — and onsets ramp linearly across the lookahead. Allocation-free
  per block (rings preallocated; covered by the existing fidelity no-alloc test). New tests:
  `transient_never_overshoots_at_the_shortest_release`, `gain_onsets_ramp_instead_of_stepping`.

- [x] **B3 — Dereverberation subtracted the direct sound** (`speech/dereverberation/src/lib.rs`)
  The "reverb estimate" was the previous STFT frame (one 5.3 ms hop, 75 % overlap — the current
  direct sound), so sustained vowels lost a constant −6.7 dB at the default amount. Rewritten
  Lebart-style: power-domain subtraction of `amount · ρ² · P[n−D]`, with D ≈ 64 ms (per-frame ring,
  sample-rate aware) and ρ the decay an assumed-T60 (0.5 s) room applies over that gap. Steady
  speech now loses ≲1 dB at the default; tails ringing at/slower than the assumed decay are pushed
  to the gain floor; onsets pass by construction. New tests: `sustained_speech_is_barely_attenuated`,
  `onsets_pass_unattenuated`, plus the retuned tail-vs-steady test. Real-speech claim
  (`suppresses_reverb_tails_without_artifacts`) passes.

- [x] **B4 — Bass Enhancer injected DC** (`speech/bass-enhancer/src/lib.rs`)
  Asymmetric clipping of the low band rectifies (nonzero mean) and nothing downstream blocked it
  before the limiter. Fixed: 20 Hz DC-blocking high-pass on the harmonics tap before the blend.
  New test `output_carries_no_dc_offset`.

- [x] **B5 — Air Exciter aliased and over-shelved** (`speech/air-exciter/src/lib.rs`)
  Soft-clipping a band open to Nyquist folded harmonics of >8 kHz content back as inharmonic fizz,
  and the un-normalized shaper made Amount a ~+14 dB shelf. Fixed: the shaper input is band-limited
  4–8 kHz (3rd-order products stay under Nyquist at 48 kHz) and the normalized tap bounds Amount to
  ≤ +6 dB of presence plus harmonics. *Design note:* a harmonics-only tap (linear band subtracted)
  was tried first and measured inaudible on real speech (+0 % HF) — the ported design blends the
  soft-clipped band copy, and the real-speech claim test confirms the blended form.

- [x] **B6 — HighPass reset its filter state on every parameter change** (`speech/high-pass/src/lib.rs`)
  Cutoff sweeps clicked. Fixed: coefficients swap without clearing state; only stages newly
  activated by a slope increase reset (their state is stale); full reset in `prepare`. New test
  `cutoff_sweep_does_not_click`.

- [x] **B7 — Cascaded high-pass used Q = 0.707 for every stage** (`speech/high-pass/src/lib.rs`)
  −3 dB × N droop at cutoff instead of the Butterworth −3 dB. Fixed: proper Butterworth pole Qs
  per stage count (0.541/1.307 for 24 dB/oct, …). New test `cutoff_sits_at_butterworth_minus_3_db`.

- [x] **B11 — Patch knob params were never applied to the DSP** (`plugins/caloma/src/chain_effect.rs`,
  `plugins/caloma/src/runtime.rs`) — *found by evaluating the tuner after the first fix round.*
  The typed `slot_params` structs were serialized, persisted, and "tuned", but no code ever called
  `Effect::set_parameter`: every effect ran at its crate defaults forever, loaded patches' knob
  values were decorative, and the default-tuning search's objective was **flat** (bit-identical
  score for an EQ shelf at −2 vs +5 dB — the searched dims were exactly the params that never
  reached the chain, so `make tune-defaults` silently "confirmed" its starting values). It went
  unnoticed because the struct defaults mirror the effect defaults. Fixed: `apply_slot_params`
  (exhaustive over `SlotId`) pushes each slot's params through `set_parameter`; the runtime
  applies it whenever the patch changes and after every chain build (allocation-free change
  detection — `CalomaPatch` is all-`Copy`). Guards: a fast unit test
  (`patch_knob_params_reach_the_effects`) and a flat-objective assertion inside the tuner itself
  (distinct scores required across the search).

## Galad host

- [x] **B8 — No capture/render sample-rate reconciliation** (`galad/src/audio/wasapi/engine.rs`)
  Streams open at each device's own rate and the transport moves samples 1:1: a 44.1 kHz mic into
  a 48 kHz output played ~8.8 % sharp and chronically under-ran the ring. Fixed: the engine now
  refuses to start with `AudioError::SampleRateMismatch { input_hz, output_hz }`, which the UI's
  start-failure path surfaces. Realtime resampling at the transport boundary remains a possible
  future feature; the bug fixed here is the silent corruption.

- [x] **B9 — Capture ignored `AUDCLNT_BUFFERFLAGS_SILENT`** (`galad/src/audio/wasapi/engine.rs`)
  A packet flagged silent carries undefined data; it was converted and fed to the chain. Fixed:
  the flag is honored and the packet's length of zeros is pushed instead.

- [x] **B10 — Chain slot failure propagated a stale buffer** (`galad/src/vst3_host/chain.rs`)
  `process()` results were ignored and the ping-pong buffers swapped unconditionally, so a failing
  plugin re-emitted the previous block's audio. Fixed: the destination buffers are zeroed before
  each slot and the swap is skipped when the slot's `process()` fails (the slot acts bypassed for
  that block). New test `failing_slot_acts_bypassed_and_never_replays_stale_audio`.

## Verification

- `make ci` green (includes the speech fidelity batteries and Galad's neutral-core tests).
- `make host-windows-check` green (WASAPI engine changes compile for the real target).
- Real-speech claims re-run for the changed effects (`--test integration -- --include-ignored`
  on air-exciter, bass-enhancer, dereverberation): green.
- Calóma heavy chain suite (`--test chain --test chain_e2e --test full_chain_fidelity`) green —
  the committed per-order tuned defaults still pass their fidelity gates with the corrected
  effects (`committed_defaults_pass_per_order_fidelity_gates`, 209 s).

## Open findings from the railed-defaults investigation (2026-07-02, not yet fixed)

Investigated the de-esser threshold rail (−22, the least-active bound, in Clarity + Broadcast).
Measured on `speech_clean_continuous_48k` at the harness's −12 dBFS operating level, the
de-esser's own detector (Q≈3 band → 1 ms/50 ms peak follower) sees: p50 −46, p90 −33.6,
max −22.9 dBFS over speech-active windows. Engagement: thr −40 → 12 % of windows, 2.9 dB mean
reduction on the sibilant decile vs 0.04 dB on vowels (healthy, selective); thr −30 (shipped
default) → 2 % (inert); thr −22 → 0 % (fully inert). The duck mechanism itself is correct.

- [x] **F1 — The tuning objective has no sibilance/harshness term** *(fixed: sixth scored term —
  es-burst prominence over program level, top-decile of 20 ms windows, level-invariant; preserved
  = 0.5, reduced → 1, raised → 0; weights rebalanced to 1/6 each)*, so de-essing can only cost
  (HF-presence/clarity) and never earn: the optimizer's true optimum for the de-esser dimension is
  always "least active", regardless of de-esser health. The rail was the correct argmax of a
  mis-specified objective. Fix: add a scored sibilance term (e.g. output band/full-level ratio or
  es-burst overshoot against a target) — or stop searching the de-esser dimension until one exists.
- [x] **F2 — The de-esser's absolute-dBFS threshold is level-dependent and mis-calibrated for the
  product's operating level**: the searched range (−40…−22) and shipped default (−30) sit
  ~10–20 dB above the detector's actual sibilance levels at −12 dBFS staging, so the *default is
  effectively inert* on realistic material. Established de-esser practice is level-relative
  detection (band level vs. program level — dbx 902-style ratio, FabFilter Pro-DS "relative"
  mode) precisely so one setting works at any gain staging; the existing inline
  `SibilanceEnergy` (band/full normalized) is the natural detector to reuse.

### F1/F2 fix + retune outcome (2026-07-02)

De-esser detection is now **level-relative** (band envelope over a 5 ms/100 ms program envelope,
−60 dBFS silence floor; default −12 dB sits in the measured vowel/ess gap −26 vs −8…+1 dB), with
level-invariance + silence-floor regression tests. Retune with the live objective: Clarity 0.73,
Broadcast 0.73, Light 0.46; full-battery gates green; Clarity/Broadcast integrate at −16 LUFS
(on target). **The de-esser pathology is confirmed fixed**: its optimum reversed from
"least-active rail" to active (Broadcast −24 = most aggressive, Light interior −8, Clarity −22).

Second round (same day): capped `limiter.ceiling` range at −1.0 (the documented −1 dBTP intent —
it railed to −0.5 when allowed), reshaped the clarity reward into a **tent around a +3 dB
presence lift** (monotone reward had made the high shelf a free win), snapped tuner start values
into the search ranges (an out-of-range committed value survived descent otherwise), and raised
descent to 3 early-stopping passes (single-pass left order-dependent, run-to-run rail flips on
the coupled landscape). Final retune **converged** (Clarity/Light fixed points in one pass,
Broadcast two): Clarity 0.56 @ −16.1 LUFS, Broadcast 0.60 @ −22.7 LUFS, Light 0.43 @ −33 LUFS;
full battery green.

Remaining rails, all stable and explainable (see final session report for the table):
- `limiter.ceiling = −1.0` everywhere — railed at the design target: the cap doing its job.
- `de_esser.threshold = −24/−24/−22` — most-active bound: the sibilance reward is monotone in
  prominence reduction (the mirror of the old pathology, mild); a tent around a target prominence
  would give it an interior optimum. Candidate refinement, not shipped.
- `compressor.makeup = 0` (Broadcast/Light, hence quiet LUFS) — converged choice: buying loudness
  through the limiter costs more in clarity/coloration than the loudness term earns; consistent
  with the D6 decision that loudness is the user's concern (Clarity reaches −16 with headroom).
- Tonal shelves per-order at bounds (Clarity: high shelf +5; Broadcast: low shelf +5, high shelf
  −2), `air = 20` low (don't add ess-band energy — coherent), `high_pass.cutoff = 140` in Clarity
  (weakly-determined dim).

## Follow-up candidates (not bugs)

- Re-run `make tune-defaults` at leisure: the tuned defaults were optimized against the old
  (gain-boosting) saturation stages; they still pass their gates, but a re-tune could reclaim the
  headroom the normalization freed up.
- Galad realtime resampling at the transport boundary would lift the same-rate device requirement
  B8 now enforces.
