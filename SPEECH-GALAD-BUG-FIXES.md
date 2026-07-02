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

## Follow-up candidates (not bugs)

- Re-run `make tune-defaults` at leisure: the tuned defaults were optimized against the old
  (gain-boosting) saturation stages; they still pass their gates, but a re-tune could reclaim the
  headroom the normalization freed up.
- Galad realtime resampling at the transport boundary would lift the same-rate device requirement
  B8 now enforces.
