# Cenedril M2 — Step Plan

Expansion of **M2** from [`CENEDRIL-VST-PLAN.md`](CENEDRIL-VST-PLAN.md): the **realtime,
allocation-free audio-thread analysis core** (STFT magnitude frames + peak/RMS/crest + LUFS) → a
**lock-free SPSC ring of frames + a meter snapshot**, plus the **off-thread worker** running the
allocating `SignalAnalyzer` → `SignalSnapshot`s. [depends on M0]

This is a **cross-platform DSP milestone** — no Windows/Vizia/Galad dependency. The audio-thread
core and the ring/meter are fully `make ci`-testable on Linux; only the worker's *threaded* end-to-
end test is `integration-tests`-gated (`make test-integration`), per the AGENTS rule that `make ci`
unit tests must not spawn threads.

## Reference / reuse map (from `CENEDRIL-VST-PLAN.md` "Context / reuse map" + the cited ADRs)

*Reuse as-is:*
- **`StftProcessor`** (`crates/lindelion-dsp-utils/src/stft.rs`): `new(frame_size)` (power-of-two,
  fixed 75% overlap, sqrt-Hann, realfft), `process(&mut [f32], frame_fn: impl FnMut(&mut [Complex32]))`
  (calls `frame_fn` once per hop with the spectrum, length `frame_size/2 + 1`), `reset()`,
  `latency_samples()`. **Allocation-free** (proven by its own test). Magnitude = `Complex32::norm()`.
- **Levels** (`crates/lindelion-dsp-utils/src/analysis.rs`): `peak_abs(&[f32]) -> f32`,
  `rms(&[f32]) -> f32` — allocation-free. (Crest = peak/rms, computed here.)
- **`AnalysisWorker`** (`lindelion-speech-signals`, `speech/signals/src/worker.rs`): the **off-thread
  worker is already built** — `new(sample_rate)` (spawns the worker thread + a lock-free
  `SampleRing` + atomic snapshot), `push(&[f32])` (audio thread, allocation-free), `latest() ->
  SignalSnapshot` (audio thread, allocation-free). Runs `SignalAnalyzer` (which **allocates**, hence
  off-thread). `SignalSnapshot` = pitch_hz, pitch_confidence, voicing_score, voicing_state,
  onset_flux_high, spectral_flux, hnr_db.
- **Inline speech-presence** (`lindelion-speech-signals::inline::SpeechPresence`): the cheap,
  **audio-thread, allocation-free** speech-presence signal (the panel's "speech presence" without
  the worker).

*Build new (the M2 deltas):*
- **LUFS** (ITU-R BS.1770-4 K-weighting + integration) — absent today.
- **A lock-free SPSC ring of STFT magnitude frames + a meter snapshot** — the existing
  `SampleRing` is per-*sample* and private to `speech/signals`; mirror its lock-free discipline at
  *frame* granularity in Cenedril.

*ADRs:* [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (audio thread allocation-free —
the binding constraint for Steps 1–3); the analysis is a **parallel tap** that never alters or
delays the audio (the M0 bit-exact 0-latency passthrough is preserved).

*Tools:* `lindelion-test-allocator` (`install_test_allocator!` + `assert_no_allocations`) for the
allocation-free proofs; the per-crate `integration-tests` feature +
`#[cfg_attr(not(feature = "integration-tests"), ignore = "…")]` for the one threaded test.

---

## Step 1 — LUFS meter (ITU-R BS.1770-4) in `lindelion-dsp-utils`  [DECISION]
- **File(s):** `crates/lindelion-dsp-utils/src/lufs.rs` (new) + `crates/lindelion-dsp-utils/src/lib.rs`
  (register `pub mod lufs;` + re-export). Reuse the crate's existing `Biquad` for the K-weighting
  stages (K-weighting is *fixed coloration* → RBJ-style biquads are the right tool, per the filter
  strategy).
- **Reference behavior (re-derive from ITU-R BS.1770-4, do not guess constants):**
  - **K-weighting** = two cascaded biquads: stage 1 a high-shelf "pre-filter" (~+4 dB shelf near
    1681 Hz) and stage 2 an "RLB" high-pass (~38 Hz). Use the BS.1770-4 reference coefficients at
    48 kHz, and re-derive coefficients for an arbitrary `sample_rate` (bilinear transform of the
    analog prototypes) so the meter is correct off 48 kHz.
  - **Loudness** `L = -0.691 + 10·log10( Σ_ch G_ch · z_ch )` LKFS, where `z_ch` is the mean square
    of the K-weighted channel and the channel gains `G = 1.0` for L/R (no surround). Cenedril is
    stereo → sum both channels' K-weighted mean-square.
  - **Momentary** = mean-square over a sliding **400 ms** window; **short-term** = sliding **3 s**;
    **integrated** = gated mean over 400 ms blocks at 75% overlap, with the absolute gate
    **−70 LKFS** and the relative gate **−10 LU** below the ungated gated-mean (two-pass gating per
    BS.1770-4).
  - **Allocation-free:** preallocate the sliding-window ring buffers + gating-block accumulators in
    `reset(sample_rate)` (sized for the rate); `push` must not allocate.
- **[DECISION] (the LUFS set):** which of **momentary / short-term / integrated** to implement.
  *Recommendation — all three* (the complete BS.1770-4 meter a visualizer should show; the
  user-wide planning default is complete features, not MVP cuts). Momentary-only would be an
  interim slice; confirm the full set or name a narrower one. (True-peak/oversampled-peak is **out**
  of M2 scope — sample-peak only — unless you ask for it.)
- **Change:** add `LufsMeter { … }` with `reset(sample_rate)`, `push(&[f32], &[f32])` (L/R blocks)
  updating the K-weighted mean-square state, and `momentary()/short_term()/integrated() -> f32`
  (LKFS), plus a silence floor (`f32::NEG_INFINITY` or −70.0).
- **Verify (`make ci`, allocation-free):** unit tests with re-derived expected values (not magic
  numbers): a 997 Hz sine of amplitude `A` reads momentary `≈ 10·log10(A²/2) − 0.691 + K_gain(997Hz)`
  (K_gain ≈ 0 dB near 1 kHz) within ±0.5 LU once the 400 ms window fills; short-term and integrated
  converge to the same steady value for a stationary sine; **silence floors** (integrated ≤ −70).
  Wrap `push` in `assert_no_allocations!`. **Red:** `LufsMeter` doesn't exist (greenfield). **Green:**
  values match within tolerance and `push` allocates nothing.

## Step 2 — Lock-free STFT magnitude-frame ring + meter snapshot (Cenedril)  [depends on #1]
- **File(s):** `plugins/cenedril/src/analysis/ring.rs` (new) + `plugins/cenedril/src/analysis/mod.rs`
  (new, `mod ring;`) + `plugins/cenedril/src/lib.rs` (`mod analysis;`).
- **Reference behavior:** mirror the lock-free SPSC discipline of `speech/signals`'s `SampleRing`
  (`AtomicUsize` `write`/`read` with `Release`/`Acquire`, `f32`-as-`AtomicU32` storage, power-of-two
  mask, **lossy on overflow** — drop oldest when the consumer lags), but at **frame granularity**:
  a `FrameRing` of `FRAME_SLOTS` preallocated slots, each slot a fixed `BIN_COUNT = frame_size/2 + 1`
  block of magnitudes plus a monotonically increasing frame index. Producer `push_frame(&[f32])`
  (audio thread) copies the magnitudes into the next slot and bumps `write` with `Release`, never
  allocating; consumer `drain_frames(&mut impl FnMut(frame_index, &[f32]))` (editor thread) reads
  slots written since its last read. Plus a `MeterSnapshot { peak, rms, crest, lufs_momentary,
  lufs_short, lufs_integrated, speech_presence }` published via per-field `AtomicU32` (`f32::to_bits`)
  — the same publish/read shape as the worker's snapshot — allocation-free on both sides. `frame_size`
  / `FRAME_SLOTS` are construction constants (a spectrogram frame size, e.g. **2048**, and enough
  slots for the scroll history, e.g. **512**), sized once at construction (off the audio thread).
- **Change:** add `ring.rs` (`FrameRing` + `MeterSnapshot` + an atomic snapshot cell); the slot
  storage is a single `Box<[AtomicU32]>` of `FRAME_SLOTS * BIN_COUNT`, preallocated in the
  constructor.
- **Verify (`make ci`, single-threaded, deterministic):** push a known sequence of frames; `drain`
  yields them **in order** with correct magnitudes; overflow drops the oldest (documented lossy
  behavior) and the consumer still gets a contiguous recent run; a `MeterSnapshot` round-trips a
  known struct through publish→read. `push_frame` and meter `publish` proven allocation-free. **Red:**
  `FrameRing`/`MeterSnapshot` don't exist (greenfield). **Green:** ordered drain + round-trip pass,
  no allocations.

## Step 3 — Audio-thread analysis tap wired into `Cenedril` (StFT + levels + LUFS → ring/meter)  [depends on #1, #2]
- **File(s):** `plugins/cenedril/src/analysis/mod.rs` (the `CenedrilAnalysis` core) +
  `plugins/cenedril/src/plugin.rs` (wire the tap into `process`/`reset`) +
  `plugins/cenedril/Cargo.toml` (add `lindelion-speech-signals.workspace = true`).
- **Reference behavior:** Per [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) the tap is
  allocation-free and **parallel** — it reads the input only and never touches the output, so the M0
  passthrough stays bit-exact at 0 latency. `CenedrilAnalysis::reset(sample_rate, max_block)`
  (off the audio thread) builds: `StftProcessor::new(FRAME_SIZE)`, `LufsMeter::reset(sample_rate)`,
  the `FrameRing`, the inline `SpeechPresence`, and a **preallocated mono scratch buffer** (so the
  STFT tap never allocates and never reuses the output buffer). `CenedrilAnalysis::process(input:
  AudioInputBuffer)` (allocation-free): downmix L/R → mono scratch; run `StftProcessor::process` on
  the scratch, in `frame_fn` writing `bin.norm()` magnitudes into the `FrameRing`; compute
  `peak_abs`/`rms`/crest over the block; update + read `LufsMeter` (push L/R); update
  `SpeechPresence`; **publish** the `MeterSnapshot`. **The worker is owned here behind an `Option`**
  (see Step 4) so this core is constructible **without spawning a thread** for `make ci`: provide
  `CenedrilAnalysis::without_worker(...)` for unit tests and the worker-bearing constructor for the
  plugin. Wire `Cenedril::reset` → `analysis.reset(...)`; `Cenedril::process` → after the bit-exact
  passthrough copy, call `analysis.process(input)`.
- **Change:** add `CenedrilAnalysis`; thread it into `Cenedril` (a field, reset + process). Keep the
  passthrough copy exactly as M0.
- **Verify (`make ci`, thread-free via `without_worker`):** (a) the M0 bit-exact passthrough test
  **still passes** (the tap leaves the output untouched); (b) `assert_no_allocations!` around
  `Cenedril::process` after `reset` (the tap path allocates nothing); (c) after feeding ≥ `FRAME_SIZE`
  samples of a known sine, `drain_frames` yields ≥1 frame whose peak magnitude bin matches the sine's
  frequency, and the `MeterSnapshot` peak ≈ the input peak with LUFS in the Step-1 expected range.
  **Red:** `CenedrilAnalysis` doesn't exist / `process` doesn't tap (greenfield + the passthrough-only
  M0 `process` produces no frames). **Green:** frames + meters appear, output unchanged, no allocations.

## Step 4 — Off-thread `SignalAnalyzer` worker integration (reuse `AnalysisWorker`)  [depends on #3]
- **File(s):** `plugins/cenedril/src/analysis/mod.rs` (wire the `Some(AnalysisWorker)` path +
  `latest_snapshot() -> SignalSnapshot` for the editor) + `plugins/cenedril/src/plugin.rs` (the
  plugin builds `CenedrilAnalysis` *with* the worker).
- **Reference behavior:** reuse `lindelion_speech_signals::AnalysisWorker` unchanged — `new(sr)`
  spawns the worker thread (in `reset`, off the audio thread); `process` calls `worker.push(scratch)`
  (allocation-free) each block; `latest_snapshot()` returns `worker.latest()` (allocation-free) for
  M5's panel. The worker runs `SignalAnalyzer` (allocating) off-thread and publishes `SignalSnapshot`s
  through its own lock-free handoff.
- **Change:** add the worker field + `latest_snapshot()`; the plugin's `reset` constructs the
  worker-bearing analysis.
- **Verify:** **Plan-exit correction (honest):** the plan's M2 exit says the worker is "tested in
  `make ci`", but constructing the worker **spawns a thread**, which AGENTS forbids in `make ci`
  units. So: a **`integration-tests`-gated** test (`#[cfg_attr(not(feature = "integration-tests"),
  ignore = "see make test-integration")]`, run via `make test-integration`) builds the worker-bearing
  `CenedrilAnalysis`, pushes a **voiced** fixture for enough blocks (brief drain wait), and asserts
  `latest_snapshot().voicing_state` reflects *voiced* while silence reflects *silence*. The cheap
  `make ci` coverage of the worker hand-off is the allocation-free `push`/`latest` guarantee already
  proven inside `speech/signals` (Step 3 keeps the audio-thread core thread-free). **Red:** the worker
  path / `latest_snapshot` doesn't exist (greenfield). **Green:** the gated test sees a voiced
  snapshot; `make ci` stays green.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| 1 | The **LUFS set** (BS.1770-4 momentary / short-term / integrated). | **Open** — recommend **all three** (complete meter; planning default avoids MVP cuts). True-peak excluded unless requested. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. The executor **stops at Step 1's
`[DECISION]`** (LUFS set). Steps 1–3 are fully `make ci`-verifiable on Linux (allocation-free proofs
included); Step 4's threaded worker test runs via `make test-integration`. The whole milestone is
cross-platform — no Windows/Vizia involvement; the editor *renders* these frames/meters/snapshots in
M3+/M5.
