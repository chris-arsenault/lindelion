# Hot-Mic Effect Port — Implementation Plan

Temporary working plan (repo-root per AGENTS.md). Goal: re-implement the *intent* of
hot-mic's channel-strip effects as idiomatic, allocation-free Rust effect crates, proven by
objective audio fidelity tests. No VST integration yet; hot-mic's routing/shell/UI/WASAPI are
out of scope.

Source repo: `~/repos/hot-mic` (C# / .NET 10). Effects live in
`src/HotMic.Core/Plugins/BuiltIn/`; per-effect specs in `hot-mic/docs/technical/`.

## Confirmed decisions

- **Packaging:** shared `lindelion-effect` trait crate (mirrors hot-mic `IPlugin`:
  `process(&mut [f32])`, indexed float params, get/set state, latency, allocation-free per
  ADR-0001) + **one crate per effect**.
- **Scope:** all three tiers, including the ONNX/ML effects.
- **Tests:** a shared **fidelity-harness crate** (general-signal battery) + per-effect-class
  objective tests. Existing pitch-shift battery stays put.
- **Analysis signals:** **no compute-once shared context up front.** Each effect is
  self-contained and derives the signals it needs on demand via shared crates. A shared
  analysis pipeline is a **benchmark-gated contingency**, not a goal.
- **Pitch:** use **SwiftF0** (`lindelion-pitch-detect`, tract-onnx, model bundled) instead of
  hot-mic's YIN/pYIN/SWIPE/cepstral/autocorrelation. The bet: SwiftF0 is cheap enough that
  per-plugin self-derivation never needs a shared bus.
- **Signal fidelity:** match *intent* and reuse `lindelion-pitch-detect` /
  `lindelion-onset-detect` / `lindelion-dsp-utils`; do not port `AnalysisSignalProcessor`'s
  exact math. Objective test per signal.

## Repository layout (decided)

Same workspace, layered so the product boundary is physical and a future split stays cheap
([ADR-0012](docs/adr/0012-speech-effect-port-shared-workspace.md)).

| Path | Role |
| ---- | ---- |
| `crates/lindelion-effect` | Host-agnostic effect trait + neutral param/state/latency primitives (shared, use-case-neutral). |
| `crates/lindelion-fidelity` | Shared general-signal fidelity harness (shared, use-case-neutral). |
| `speech/<effect>` | One crate per ported effect, package `lindelion-speech-<effect>` (speech-tuned). |
| `speech/signals` | Speech analysis-signal derivation library (speech-tuned). |

Shared foundations also reused from `crates/`: `lindelion-dsp-utils`, `lindelion-pitch-detect`,
`lindelion-onset-detect`. Crate directories are registered in the workspace `Cargo.toml` as
each is implemented (M0+), so `make ci` stays green; the READMEs reserve the homes now.

## Packaging neutrality (cross-cutting constraint)
See [ADR-0013](docs/adr/0013-host-agnostic-effect-core.md).


The eventual packaging is **undecided** and must stay that way. The implementation must support
**all three** future targets without rework:

1. a standalone application (à la hot-mic),
2. a single VST with a prebaked signal flow,
3. one VST per effect.

Design rules that keep all three open:

- **Host-agnostic core.** `lindelion-effect` and every effect crate depend **only** on the
  pure-DSP crates (`lindelion-dsp-utils`, `lindelion-pitch-detect`, `lindelion-onset-detect`).
  They must **not** depend on `lindelion-plugin-shell` (it pulls in `vst3`), `vst3`,
  `lindelion-ui`, or any app/host code. Verified: the pure-DSP crates carry no host deps.
- **Adapters depend on the core, never the reverse.** VST3 wrappers, a standalone app, and any
  chain/graph host are **separate future layers** built on top. None exist in this phase.
- **No signal-flow lock-in.** An effect knows nothing about its neighbors, chain position, or
  host routing. A chain/graph is an optional composition layer added later — not assumed by
  effects. (This is also why we rejected hot-mic's position-dependent analysis routing.)
- **No I/O assumptions.** `process` operates on plain sample blocks; no two-mic / stereo-routing
  / fixed-sample-rate assumptions baked in (sample rate arrives via init). The ML 48 kHz
  constraint is handled *inside* those effects, not imposed on the core.
- **Neutral host primitives only.** The trait exposes indexed float params, opaque byte-blob
  state, and latency-in-samples — enough for automation, persistence, and PDC in any of the
  three packagings, without choosing one.
- **Shared analysis stays optional.** If the M4 shared-signal context is ever built, it is an
  optional composition layer usable by all three packagings — never a host-specific feature.

Non-goal for this phase: any VST3 entry point, bundle/factory metadata, editor, or app shell.

## Use-case divergence: musical → speech (cross-cutting principle)

Lindelion's existing DSP is tuned for **wind-like instruments in a musical context**
(sustained pitched notes, timbre/musicality, note/phrase segmentation). This port targets
**spoken word in a meeting / oration context**. Every algorithm, default, threshold, and test
here is tuned for **speech clarity and intelligibility — not musicality.** Concretely:

- **Reuse primitives, re-tune parameters.** A Biquad is a Biquad; its cutoffs/Q/defaults are
  not. Reused building blocks get speech-context defaults, not the instrument defaults.
- **Pitch:** target speech f0 (~80–300 Hz; male ~85–180, female ~165–255) and *dynamic*
  voiced/unvoiced switching, not sustained-note tracking. SwiftF0's musical confidence/voicing
  gating likely needs speech tuning.
- **"Onset" means consonant transients** (plosives/fricatives), not pitched note onsets.
  onset-detect's flux engine is reusable; its pitch-stability/note-segmentation layer is the
  wrong model and is not used.
- **Effect goals shift:** de-esser = sibilance control without dulling consonant
  intelligibility; saturation = vocal presence/warmth, not harmonic richness; EQ bands =
  speech formant/presence regions; compressor = dialogue leveling. Success metrics are
  intelligibility/clarity, not musical fidelity.
- **Fixtures weight toward speech.** `vocal_spoken.wav` + sourced spoken-word (M6) are the
  primary battery; instrument/ensemble fixtures become robustness/"don't explode on
  non-speech" cases, not tuning targets.

## Context / reuse map

The source-of-truth an executor re-derives reference behavior from: this reuse inventory, the
`## Repository layout (decided)` section above, the cross-cutting constraint sections, and the
ADRs they cite (0012, 0013) plus the workspace ADRs (0001 allocation-free, 0007 macOS build).

Milestones **must reuse existing lindelion DSP where it overlaps**, re-tuned for speech.

**Reuse as-is (speech-tuned params):**
- `dsp-utils::filters` — `Biquad` (LP/HP/BP), `Svf`, `OnePoleLowpass` → HPF, 5-Band EQ,
  de-esser/fricative/sibilance bands, dynamic-EQ bands.
- `dsp-utils::smoothing` — `LinearSmoother`, `SmoothedParam` → gain/param ramps.
- `dsp-utils::delay` — `DelayLine`, `FirstOrderAllpass` → limiter lookahead, dereverb,
  convolution-reverb taps.
- `dsp-utils::resampling` — `WindowedSincResampler` → 48 kHz ⇄ SwiftF0 16 kHz.
- `dsp-utils::{window, ola}` — Hann + OLA window-sum → STFT framing for Tier-2.
- `dsp-utils::{params, math, analysis}` — effect param model, helpers, fidelity metrics.
- `lindelion-pitch-detect` SwiftF0 streaming tracker → PitchHz/PitchConfidence (re-tuned).
- `lindelion-onset-detect` flux engines (SuperFlux/ComplexFlux) → high-band flux **only**;
  bypass the note-segmentation/pitch-stability layer.

**Gaps — lindelion's primitive is musical or absent; build new, speech-tuned:**
- **Envelope follower.** `dsp-utils::envelope` is an **ADSR for synth voices**, not a peak/RMS
  attack-release follower. Every dynamics effect needs one → build a shared speech-tuned
  envelope follower in dsp-utils, in **M1** (first consumers: the dynamics effects).
- **Saturation.** No saturation module exists → build in dsp-utils, in **M1** (voice-presence
  shaping, not musical harmonics).
- **Shelf / peaking biquad coefficients.** `dsp-utils::filters` has only LP/HP/BP. The 5-Band
  EQ (and later Dynamic EQ) need low/high shelf + peaking → extend `BiquadCoefficients` in
  **M1** (RBJ cookbook).
- **Realtime FFT.** No standalone allocation-free STFT exists — `pitch-shift/spectral.rs` and
  `onset-detect/spectral_flux.rs` both create a `RealFftPlanner` per call. Build a new
  preallocated-scratch realfft STFT in dsp-utils, in **M3** (first consumer: the Tier-2 spectral
  effects that run FFTs inline on the audio thread). The M2 FFT *signals* (OnsetFlux /
  SpectralFlux / HnrDb) instead use plain `realfft` inside the off-thread analysis worker, where
  allocation is allowed, so they do not need this STFT.
- **Note/phrase segmentation** (`phrase-analysis`, `NoteSegmenter`) — musical; **not used.**

## The analysis-signal story (the part that was "a mess")

Hot-mic's analysis-tap was a **compute-once / read-many bus** for expensive shared
measurements. Ten normalized signals were computed once (mostly in `AnalysisSignalProcessor`,
1264 LOC) and published to per-producer sample-time ring buffers; downstream effects read
aligned samples instead of each re-running STFT + pitch tracking.

**Real producer → consumer fan-out (why the perf win was real):**

| Signal | Produced by | Consumed by |
| --- | --- | --- |
| SpeechPresence | tap, VoiceGate | RoomTone, SpectralContrast, UpwardExpander, AirExciter |
| VoicingScore | tap | BassEnhancer, DynamicEq, AirExciter |
| VoicingState | tap | UpwardExpander |
| FricativeActivity | tap | DynamicEq |
| SibilanceEnergy | DeEsser (side-effect producer) | AirExciter |
| OnsetFluxHigh | tap | ConsonantTransient |
| PitchHz / PitchConfidence / SpectralFlux / HnrDb | tap | (analysis/visualizers) |

What rotted: the **routing machinery**, not the idea — per-slot producer maps recomputed on
every chain edit, three overlapping interfaces (Producer/Consumer/Blocker), a
Generate/UseExisting/Disabled tri-mode tap, and effects that double as producers (DeEsser
emits SibilanceEnergy). Position-in-chain changed which signals existed. We keep the *idea*
(typed named signals), drop the routing.

### Our approach: self-contained, measure before sharing

Each effect derives its required signals internally from shared crates. Signal → crate map
for idiomatic re-derivation:

| Signal | Derive from |
| --- | --- |
| PitchHz, PitchConfidence | `lindelion-pitch-detect` SwiftF0 streaming tracker |
| VoicingScore, VoicingState | SwiftF0 confidence + periodicity gate (idiomatic) |
| OnsetFluxHigh, SpectralFlux | `lindelion-onset-detect` flux **engine only** (no note segmentation), speech HF band |
| SpeechPresence | dsp-utils envelope follower on preprocessed signal |
| FricativeActivity | dsp-utils HPF ~2.5 kHz + envelope, normalized |
| SibilanceEnergy | dsp-utils BPF ~6.5 kHz (Q≈1.2) + envelope, normalized |
| HnrDb | spectral flatness via dsp-utils window/analysis |

**Two realtime risks this raises (the actual subject of the perf testing):**

1. **Redundant CPU** — if 3 effects each self-derive VoicingScore, that's 3× SwiftF0 +
   voicing per block. The bet is SwiftF0 makes this affordable. Benchmarks decide.
2. **Allocation / realtime-safety** — tract-onnx inference **allocates**, and FFT-based
   signal derivation must use preallocated scratch to satisfy ADR-0001. Hot-mic ran all this
   inline on the audio thread; we cannot, allocation-free, without care. Per signal we must
   determine: allocation-free inline path possible (preallocated FFT scratch), or does it need
   a deferred worker (as Linnod already does for analysis)?

The benchmark gate is therefore **two-axis**: CPU cost of N× derivation *and*
allocation/realtime safety of each derivation path.

## Milestones

Seven milestones, **M0–M6**. Points that need your input are tagged inline with
**`[DECISION]`** and collected here:

| Where | Decision you own |
| --- | --- |
| M2 | Approve the per-signal gate verdict (`self-derive-ok` / `needs-sharing` / `needs-worker`) — this promotes work into shared infra. |
| M4 | Whether to merge per-effect analysis workers into one shared worker — deferred until real end-state chain perf exists (M2 gate recorded). |
| M5 | Confirm scoping ADR-0001 for NN inference (bounded inline alloc, no audio worker) + accept DFN3 (Apache/MIT) and Silero (MIT) licenses. RNNoise dropped. |
| M6 | Whether you supply your own spoken-word recordings or I source public-domain / CC0 clips. |
| Post-port | Packaging target: standalone app, single VST with prebaked flow, or one VST per effect (deferred by design — [ADR-0013](docs/adr/0013-host-agnostic-effect-core.md)). |

### M0 — Foundation
The host-agnostic effect trait, the shared fidelity harness, and one trivial effect (Gain) end
to end. Gap-fillers (envelope follower, saturation, STFT) are deferred to their first consumers
(M1, M3) — nothing in M0 exercises them. Expanded steps (run in order, red→green per step):

1. **Create the `lindelion-effect` trait crate.**
   - File(s): `crates/lindelion-effect/Cargo.toml`, `crates/lindelion-effect/src/lib.rs`,
     `Cargo.toml` (add workspace member + `workspace.dependencies` path entry).
   - Reference: ADR-0013 (host-agnostic — pure-DSP deps only; no `plugin-shell`/`vst3`/`ui`) +
     ADR-0001 (allocation-free `process`). Neutral primitives: `process(&mut [f32])`, indexed
     float params, `Vec<u8>` state, `latency_samples`, `reset`, bypass.
   - Change: define `trait Effect` + param/state types. No effect implementations.
   - Verify: `cargo test -p lindelion-effect` compiles; a `Box<dyn Effect>` smoke test runs.
     Red = crate/trait do not resolve yet (greenfield).

2. **Add the allocation-free `process` contract test.**  [depends on #1]
   - File(s): `crates/lindelion-effect/src/lib.rs` (tests), `crates/lindelion-effect/Cargo.toml`
     (dev-dep `lindelion-test-allocator`).
   - Reference: ADR-0001. `lindelion-test-allocator` exposes the `install_test_allocator!`
     macro and the **function** `assert_no_allocations("label", || …)` (not a macro).
   - Change: a no-op test `Effect`; wrap its `process` in `assert_no_allocations`.
   - Verify: passes for the no-op effect. Red = test/allocator wiring does not exist yet
     (greenfield).

3. **Create `lindelion-fidelity` with the general-signal battery.**  [depends on #1]
   - File(s): `crates/lindelion-fidelity/Cargo.toml`, `crates/lindelion-fidelity/src/lib.rs`,
     `Cargo.toml` (member + dep).
   - Reference: Context / reuse map — reuse `dsp-utils::analysis` (`assert_all_finite`,
     `max_adjacent_delta`, `peak_abs`/`rms`, `windowed_dft_magnitude_at`). Checks: finite/no-NaN,
     no-clicks, denormal flush, bypass==identity, latency-report accuracy (impulse → first
     non-zero at `latency_samples`), allocation-free, frequency-response sanity.
   - Change: `run_general_battery(&mut dyn Effect, …)`.
   - Verify: passes on a passthrough fixture **and** each check has a deliberately-broken
     fixture that fails it (proves the battery is not vacuous). Red = harness symbols do not
     exist yet (greenfield).

4. **Implement the Gain effect.**  [depends on #1]
   - File(s): `speech/gain/Cargo.toml`, `speech/gain/src/lib.rs`, `Cargo.toml` (member + dep).
   - Reference: hot-mic `GainPlugin` intent (dB gain + phase invert), reuse
     `dsp-utils::smoothing` (`LinearSmoother`/`SmoothedParam`) + `db_to_gain`. Speech-tuned
     defaults are M1's concern; here only correct gain.
   - Change: `impl Effect for Gain`.
   - Verify: +6 dB ≈ ×2 after smoothing settles; bypass == identity. Red = Gain does not exist
     yet (greenfield).

5. **Run Gain through the battery; phase exit.**  [depends on #3, #4]
   - File(s): `speech/gain/tests/fidelity.rs`.
   - Reference: M0 exit.
   - Verify: `run_general_battery` passes for Gain.

- Exit: `make ci` green; Gain passes the general battery.

### M1 — Tier 1 classic time-domain effects
Gain shipped in M0. M1 builds three shared dsp-utils primitives plus the **seven self-contained**
Tier-1 effects (Noise Gate, Compressor, Limiter, De-Esser, High-Pass Filter, 5-Band EQ,
Saturation). Defaults come from hot-mic's own (already speech-context) values; the **objective
class-specific tests are the arbiter** — there is no by-ear sign-off. Each effect's verification
is the general battery + allocation-free + a class-specific objective test. Expanded steps (run
in order, red→green per step):

**Dynamic EQ and Upward Expander are deferred to post-M2.** Both require a voicing signal;
SwiftF0 allocates (`pitch-detect/swiftf0.rs:365` tract `.run`, plus its `Vec` scratch) and so
cannot run in a realtime `process()` under ADR-0001. The high-quality path is SwiftF0 at
control-rate in a worker with a lock-free handoff — the M2 deliverable — so these two effects
follow M2.

1. **Peak/RMS envelope follower.** File(s): `crates/lindelion-dsp-utils/src/envelope_follower.rs`,
   `crates/lindelion-dsp-utils/src/lib.rs` (`pub mod`). Reference: attack/release follower,
   coeff `exp(-1/(time_s * sr))`; distinct from the ADSR in `envelope.rs`. Verify: step input
   reaches ~63% of target in one attack time-constant and decays over release; allocation-free.
   Red: symbol does not exist.
2. **Saturation shaper.** File(s): `crates/lindelion-dsp-utils/src/saturation.rs`, `lib.rs`
   (`pub mod`). Reference: hot-mic `SaturationPlugin` intent — soft-clip waveshaper (e.g.
   `tanh`-based) with drive. Verify: monotonic, output bounded; a pure sine gains harmonics
   (THD rises); allocation-free. Red: greenfield.
3. **Shelf + peaking biquad coefficients.** File(s): `crates/lindelion-dsp-utils/src/filters.rs`.
   Reference: RBJ cookbook low-shelf / high-shelf / peaking, extending `BiquadCoefficients`
   (which today has only LP/HP/BP). Verify: low-shelf magnitude at LF ≈ set gain, peaking
   magnitude at center ≈ set gain. Red: methods do not exist.
4. **High-Pass Filter** → `speech/high-pass`. Reference: hot-mic `HighPassFilterPlugin`
   (Cutoff, Slope), reuse `Biquad::highpass`, Slope as a biquad cascade. Verify: −3 dB at
   cutoff, attenuation deepens below; general battery; allocation-free. Red: greenfield.
5. **Noise Gate** → `speech/noise-gate`.  [depends on #1] Reference: `NoiseGatePlugin`
   (Threshold −40, Hysteresis 4, Attack 1 ms, Hold 50 ms, Release 100 ms). Reuse envelope
   follower. Verify: gain ≈ 0 below threshold, ≈ 1 above, hysteresis holds; general battery.
6. **Compressor** → `speech/compressor`.  [depends on #1] Reference: `CompressorPlugin`
   (Threshold −20, Ratio 4, Attack 10 ms, Release 100 ms, Knee 6, Makeup, internal detector
   HPF). Reuse envelope follower + `Biquad`. Verify: gain-reduction curve (above threshold
   output rises at 1/ratio) + attack/release timing; general battery.
7. **Limiter** → `speech/limiter`.  [depends on #1] Reference: `LimiterPlugin` (Ceiling −1,
   Release 50 ms), reuse envelope follower + `delay` lookahead. Verify: output peak never
   exceeds ceiling; general battery (latency = lookahead, reported accurately).
8. **De-Esser** → `speech/de-esser`.  [depends on #1] Reference: `DeEsserPlugin` (Center 6000,
   Bandwidth 2000, Threshold −30, Reduction 6, Max Range 10); self-derives SibilanceEnergy via
   `Biquad::bandpass` + envelope follower. Verify: sibilance-band energy reduced over threshold,
   low band untouched; general battery.
9. **5-Band EQ** → `speech/five-band-eq`.  [depends on #3] Reference: `FiveBandEqPlugin`
   (HPF 80 + low shelf +3 @120 + low-mid −3 @300 Q1 + high-mid +3 @3000 Q1 + high shelf +2).
   Reuse shelf/peaking coefficients. Verify: magnitude response at each band center ≈ set gain;
   general battery.
10. **Saturation** → `speech/saturation`.  [depends on #2] Reference: `SaturationPlugin`
    (Warmth 50, Blend 100), reuse saturation shaper. Verify: THD rises vs input sine; Blend 0 ==
    identity; general battery.

- Exit: the three primitives plus all seven effects pass the general battery + their
  class-specific tests; `make ci` green.

### M2 — Analysis-signal derivation library + benchmarks (the gate)
Build all ten analysis signals as a reusable `speech/signals` library, then benchmark and record
the realtime-strategy verdict. **Cheap signals** (envelope/filter — SpeechPresence,
FricativeActivity, SibilanceEnergy) run inline on the audio thread, allocation-free. **Heavy
signals** (SwiftF0 voicing; FFT-based OnsetFluxHigh / SpectralFlux / HnrDb) run in an off-thread
analysis worker that publishes latest values via a lock-free handoff — allocation in the worker
is fine (not the audio thread). The worker uses `realfft` directly off-thread, so M3's
allocation-free STFT is **not** needed here. Each effect self-contains its own worker; the bench
gate measures the N× redundancy that informs M4. Speech-weighted tests throughout. Expanded
steps (run in order, red→green per step):

1. **`speech/signals` crate + inline signals.**
   - File(s): `speech/signals/{Cargo.toml, src/lib.rs, src/inline.rs}`, `Cargo.toml` (member +
     `workspace.dependencies` path).
   - Reference: hot-mic `docs/technical/Analysis-Signals.md` — SpeechPresence = envelope
     follower on the preprocessed signal, `clamp((envDb − (−50)) / 30)`; FricativeActivity = HPF
     ~2.5 kHz + envelope, normalized by the full-band envelope; SibilanceEnergy = BPF ~6.5 kHz
     (Q≈1.2) + envelope, normalized. Reuse the M1 envelope follower + dsp-utils filters.
   - Change: inline signal structs, allocation-free `process`.
   - Verify: on `vocal_spoken.wav`, voiced/loud frames give high SpeechPresence and silence ~0;
     fricative frames give higher FricativeActivity than vowels; each allocation-free. Red:
     greenfield.
2. **Voicing + FFT signal analyzer (synchronous, off-thread).**  [depends on #1]
   - File(s): `speech/signals/src/analyzer.rs`, `lib.rs`.
   - Reference: `SwiftF0StreamingPitchTracker::next_block` → `PitchFrame{f0_hz, confidence,
     voiced, rms}` yields PitchHz / PitchConfidence / VoicingScore(=confidence) /
     VoicingState(voiced+rms→silence/unvoiced/voiced); onset-detect flux engine
     (`StreamingSpectralFlux` / SuperFlux) → OnsetFluxHigh (HF band) + SpectralFlux; HnrDb =
     `−10·log10(spectral flatness)` via `realfft` magnitude. May allocate (off-thread only).
   - Change: `SignalAnalyzer` consuming a block → `SignalSnapshot { pitch_hz, pitch_confidence,
     voicing_score, voicing_state, onset_flux_high, spectral_flux, hnr_db }`.
   - Verify: deterministic on fixtures — voiced vowel → voiced + plausible f0 + high confidence;
     silence → unvoiced; plosive/onset fixture → OnsetFlux spike; tonal vs noise → HNR high vs
     low. Red: greenfield.
3. **Analysis worker + lock-free handoff.**  [depends on #2]
   - File(s): `speech/signals/src/worker.rs`, `lib.rs`.
   - Reference: ADR-0001 (audio thread allocation-free). SPSC ring (preallocated Vec + atomic
     indices) for audio→worker; per-signal `AtomicU32` (f32 bits) for the worker→audio latest
     snapshot; worker thread runs `SignalAnalyzer` per hop; stop flag + `join` on drop.
   - Change: `AnalysisWorker` with allocation-free `push(&self, &[f32])` and `latest(&self) ->
     SignalSnapshot`; thread spawned at construction.
   - Verify: `push` + `latest` are allocation-free (`assert_no_allocations`); a handoff smoke
     test pushes voiced audio and polls `latest()` against a **generous, scheduling-tolerant
     deadline** (per the flaky-test lesson). Red: greenfield.
4. **Dynamic EQ effect.**  [depends on #1, #3]
   - File(s): `speech/dynamic-eq/{Cargo.toml, src/lib.rs, tests/fidelity.rs}`, `Cargo.toml`
     (member).
   - Reference: hot-mic `DynamicEqPlugin` (Low Boost +2, High Boost +2, Scale, Smoothing 80 ms);
     consumes VoicingScore (worker) + FricativeActivity (inline) to shape low/high bands
     dynamically. Reuse dsp-utils shelves + the signals library.
   - Change: `impl Effect`; embeds an `AnalysisWorker` + inline FricativeActivity.
   - Verify: general battery + allocation-free; class test — voiced vs unvoiced/fricative input
     yields measurably different low/high band gain. Red: greenfield.
5. **Upward Expander effect.**  [depends on #1, #3]
   - File(s): `speech/upward-expander/{Cargo.toml, src/lib.rs, tests/fidelity.rs}`, `Cargo.toml`
     (member).
   - Reference: hot-mic `UpwardExpanderPlugin` (Amount 20, Scale, Threshold −35, Low Split 200,
     High Split 3500, Attack 8, Release 120, Gate Strength 0.8); multiband upward expansion
     gated by SpeechPresence (inline) + VoicingState (worker). Reuse dsp-utils crossover filters.
   - Change: `impl Effect`; embeds worker + inline SpeechPresence.
   - Verify: general battery + allocation-free; class test — quiet-but-present speech below
     threshold is boosted (upward ratio > 1) while silence is not. Red: greenfield.
6. **Two-axis Criterion benchmarks.**  [depends on #1, #2, #3]
   - File(s): `speech/signals/benches/signals.rs`, `speech/signals/Cargo.toml` (`[[bench]]`).
   - Reference: the two-axis gate — per-signal cost in isolation; redundant chains (3×
     VoicingScore, 4× SpeechPresence); allocation audit of each path.
   - Change: Criterion benches for each signal + the redundant-chain scenarios.
   - Verify: `make bench-smoke` compiles the benches; they run and emit timings. Red: bench
     target doesn't exist.
7. **Gate verdict table.**  [depends on #6]  **`[DECISION]`**
   - File(s): `docs/perf/speech-signal-gate.md` + a pointer from this plan.
   - Reference: the bench results from #6.
   - Change: record each signal's verdict `self-derive-ok` / `needs-sharing` / `needs-worker`.
   - Verify: **[DECISION]** — you approve the verdict before it gates M4 (whether to build the
     shared compute-once context). The artifact is the committed table + your sign-off; no code
     test.

- Exit: signal library + worker tested; Dynamic EQ + Upward Expander pass both batteries; benches
  compile and run; gate table committed and approved; `make ci` green.

### M3 — Tier 2 spectral / FFT effects  [depends on M2]
Build the allocation-free STFT first, then the eight Tier-2 effects. Of the eight, three are
genuinely spectral (FFT Noise Removal, Dereverberation, Spectral Contrast — `[dep #1]`); the
rest are time-domain filter/waveshaper/keying effects grouped here. Effects that consume worker
signals (voicing, onset-flux) each embed their own `AnalysisWorker` per the M2 gate deferral.
Every effect: general battery + allocation-free + a class-specific objective test. Reference
specs are hot-mic `docs/technical/Enhance-*.md`, `Vitalizer-Mk2T.md`, `Cleanup.md`. Expanded
steps (run in order, red→green per step):

1. **Allocation-free STFT processor.**
   - File(s): `crates/lindelion-dsp-utils/src/stft.rs`, `crates/lindelion-dsp-utils/src/lib.rs`
     (`pub mod`).
   - Reference: a new build (not a lift) — `pitch-shift/spectral.rs` and
     `onset-detect/spectral_flux.rs` plan-allocate a `RealFftPlanner` per call. Use realfft 3.5
     `make_scratch_vec` + `process_with_scratch` (no per-call allocation) with preallocated
     analysis/synthesis/scratch buffers; Hann analysis+synthesis windows with COLA overlap-add
     (reuse dsp-utils `window`/`ola`). Latency = frame_size.
   - Change: `StftProcessor` with `process(&mut [f32], frame_fn: impl FnMut(&mut [Complex32]))`
     (or analyze/modify/synthesize), allocation-free after `prepare`.
   - Verify: a passthrough frame-fn round-trips to the input delayed by `latency_samples`
     (COLA reconstruction within tolerance); allocation-free `process`. Red: symbol doesn't
     exist (greenfield).
2. **FFT Noise Removal** → `speech/fft-noise-removal`.  [depends on #1]
   - Reference: hot-mic `FFTNoiseRemovalPlugin` + `Cleanup.md` — spectral subtraction against a
     learned/estimated noise profile; reuse the STFT.
   - Change: `impl Effect`; magnitude spectral subtraction in the STFT frame-fn.
   - Verify: noise floor reduced while a tone is preserved (DFT magnitude at the tone ≈ input,
     broadband noise energy down); general battery + alloc-free. Red: greenfield.
3. **Dereverberation** → `speech/dereverberation`.  [depends on #1]
   - Reference: `Enhance-Dereverberation.md` — spectral suppression of late-reverb energy.
   - Change: `impl Effect`; per-bin reverb-decay suppression in the STFT frame-fn.
   - Verify: a synthetic reverberant tail is reduced relative to the direct sound; general
     battery + alloc-free. Red: greenfield.
4. **Spectral Contrast** → `speech/spectral-contrast`.  [depends on #1]
   - Reference: `Enhance-Spectral-Contrast.md` — raise peak-to-valley spectral contrast;
     consumes SpeechPresence (inline).
   - Change: `impl Effect`; per-bin contrast shaping in the STFT frame-fn, gated by presence.
   - Verify: spectral peak-to-valley ratio increases vs input on a formant-like fixture;
     general battery + alloc-free. Red: greenfield.
5. **Air Exciter** → `speech/air-exciter`.
   - Reference: `Enhance-Air-Exciter.md` — keyed high-frequency exciter (voiced-aware,
     de-ess-aware); HPF + waveshaper generating HF harmonics, keyed by SibilanceEnergy (inline)
     so it backs off on sibilants. Reuse dsp-utils filters + saturation + signals.
   - Change: `impl Effect`; embeds an `AnalysisWorker` for voicing if needed (or inline only).
   - Verify: HF harmonic content added above the band; attenuated when sibilance is high
     (pure-mapping key test); general battery + alloc-free. Red: greenfield.
6. **Bass Enhancer** → `speech/bass-enhancer`.
   - Reference: `Enhance-Bass-Enhancer.md` — psychoacoustic bass (harmonics of the low band so
     bass is perceived without low-end power); consumes VoicingScore (worker). Reuse filters +
     saturation + signals.
   - Change: `impl Effect`; LPF → harmonic generator → blend, voicing-keyed.
   - Verify: harmonic content added above the bass fundamental; general battery + alloc-free.
     Red: greenfield.
7. **Consonant Transient** → `speech/consonant-transient`.
   - Reference: `Enhance-Consonant-Transient.md` — emphasize consonant transients, keyed by
     OnsetFluxHigh (worker). Reuse a transient shaper + the worker.
   - Change: `impl Effect`; transient gain driven by onset flux.
   - Verify: transient attack emphasized when onset flux is high vs steady state (pure-mapping
     key test); general battery + alloc-free. Red: greenfield.
8. **Room Tone** → `speech/room-tone`.
   - Reference: `Enhance-Room-Tone.md` — synthetic room-tone bed with speech ducking; consumes
     SpeechPresence (inline). Reuse a noise generator + the presence signal.
   - Change: `impl Effect`; low-level tone bed ducked by presence.
   - Verify: tone bed present in silence, ducked under speech (pure-mapping duck test); general
     battery + alloc-free. Red: greenfield.
9. **Vitalizer Mk2-T** → `speech/vitalizer`.
   - Reference: `Vitalizer-Mk2T.md` — psychoacoustic bass/treble shaping + tube saturation
     (mono, no stereo expander). Reuse dsp-utils shelves + saturation.
   - Change: `impl Effect`; bass/treble shelf network + tube saturation core.
   - Verify: bass + treble shaping present and harmonics added (THD up); general battery +
     alloc-free. Red: greenfield.

- Exit: the STFT and all eight effects pass the general battery + their class-specific tests;
  `make ci` green.

### M4 — Shared analysis worker (CONTINGENT on real end-state perf)  [depends on M2]
**`[DECISION]`** The M2 gate ([docs/perf/speech-signal-gate.md](docs/perf/speech-signal-gate.md))
classified the cheap signals `self-derive-ok` and the SwiftF0+FFT analyzer `needs-worker`. The
**sharing** decision is deliberately deferred: every effect keeps its **own** analysis worker
for now, so the whole chain is built as individual self-contained signals. The micro-benchmarks
are isolated/synthetic, not the real steady-state load of the assembled chain.
This milestone — merging the redundant per-effect workers into **one** shared analysis worker
that all voicing/flux/HNR consumers read from — is gated on **real, steady, end-state
performance statistics** for the full chain, not the M2 micro-benches. If built, it is the
**clean** form (one worker, read-only snapshots to effects) — explicitly *without* hot-mic's
producer-map / ring-buffer / blocker / tri-mode routing. Inline signals stay self-derived.
- Exit: either the shared worker is built (gated on end-state perf data) and every dependent
  effect passes its battery with `make ci` green, **or** the deferral stands and per-effect
  workers remain. Both are valid exits.

### M5 — Tier 3 ML effects (DFN3 + Silero), run inline  [depends on M2]
RNNoise dropped (DeepFilterNet 3 supersedes it). Two effects: **Speech Denoiser (DFN3)** and
**Voice Gate (Silero VAD)**.

**Realtime principle (first-principles, corrects the earlier worker plan).** ADR-0001's no-alloc
rule is a proxy for "bounded, contention-free, deadline-safe callback" — not an end in itself.
An off-thread audio-through worker does not remove the allocation; it moves it to another thread
and puts the cleaned audio behind a cross-thread handoff still in the hot path, adding latency,
scheduling jitter, and a dropout failure mode — categorically worse than bounded inline work. A
worker is justified by **throughput** (the model can't finish within the callback deadline), not
by allocation. DFN3 (low-latency streaming, RTF < 1) and Silero VAD (tiny) fit the callback, so
both run **inline**: build the tract runnable + streaming state once at `prepare`, reuse
preallocated I/O/scratch, latency = the model's algorithmic frame latency. The only off-thread
argument is burst-smoothing at tiny buffer sizes — a measurable throughput question, not an
allocation one, and not pre-built here.

Expanded steps:

1. **Scope ADR-0001 for NN inference + source the models.**  **`[DECISION]`**
   - File(s): `docs/adr/00NN-nn-inference-allocation.md` (new ADR refining ADR-0001),
     `docs/adr/README.md`, `crates/lindelion-fidelity/src/lib.rs` (scope the allocation-free
     assertion for NN effects), model provenance doc + bundled `.onnx` assets.
   - Reference: ADR-0001 intent (bounded/deadline-safe). Decision to confirm: literal zero-alloc
     stays the bar for ordinary DSP (all 18 effects hold it); NN inference may allocate inline
     **bounded and contention-free** (preallocation first; arena allocator if measurement shows
     it is needed) — never a worker for the audio path. Source + commit DFN3 (DeepFilterNet 3,
     Apache/MIT) and Silero VAD (MIT) ONNX models; record source + license.
   - Change: write the ADR; add an NN-scoped path to the fidelity battery (the strict
     `assert_allocation_free` does not apply to NN-inference effects); fetch + bundle models.
   - Verify: ADR indexed; each model loads into a tract runnable (a load test); `make ci` green.
     **[DECISION]** — stop for the user to confirm the ADR-0001 scoping before integration.
   - Note: obtaining the binaries may need network; if unavailable the user drops the vetted
     `.onnx` files in place.
2. **DFN3 Speech Denoiser (inline, native ONNX Runtime).**  [depends on #1] — **DONE**
   - Approach (decided C): the pure-Rust paths were all infeasible — `tract` can't load the
     streaming export, the `deep_filter` crate is broken on `tract 0.23`, and a libDF→`tract 0.23`
     port is a large, fragile translation against a dev API (de-risked and confirmed: heavy
     `tract-pulse`/API drift). So the denoiser runs hot-mic's proven self-contained streaming graph
     on the **native ONNX Runtime via `ort`** ([ADR-0015](docs/adr/0015-denoiser-native-onnx-runtime.md)).
   - Files: `speech/speech-denoiser/{Cargo.toml, src/lib.rs, tests/fidelity.rs}`,
     `crates/lindelion-fidelity/src/lib.rs` (added `BatteryOptions`/`run_general_battery_with` to
     scope out the latency heuristic for NN warm-up).
   - Implementation: `impl Effect`; `Session` built once at `prepare`; one run per 480-sample hop
     threading the 45304-wide recurrent state; host-side hop-buffering makes any block size work;
     48 kHz auto-bypass; latency reported as 1920 samples (four hops); inline per ADR-0014.
   - Verified — split into two suites so the heavy inference tests stay out of `make ci`:
     - `tests/contract.rs` (model-free, runs in `make ci`): non-48k passthrough, parameter
       clamping, state round-trip, parameter surface.
     - `tests/integration.rs` (`#[ignore]`d, run via `make test-models`): block-size invariance
       (480/512/137 identical), no boundary artifacts, noise suppression (>40% on pure noise),
       **SNR +2.8 dB on noisy speech with best-lag alignment confirming the 1920-sample latency**,
       bounded inline allocation, general battery (latency-scoped), latency report, bypass identity.
3. **Silero Voice Gate (inline, native ONNX Runtime).**  [depends on #1] — **DONE**
   - Runtime (decided): `tract` cannot optimize Silero v5 (an `If`/`Squeeze` in the decoder fails
     analysis), so — like the denoiser — it runs on `ort`
     ([ADR-0015](docs/adr/0015-denoiser-native-onnx-runtime.md)), correcting that ADR's earlier
     "Silero on tract" assumption.
   - Files: `speech/voice-gate/{Cargo.toml, src/lib.rs, tests/contract.rs, tests/integration.rs}`,
     `Cargo.toml` (member).
   - Implementation: `impl Effect`; `Session` built once at `prepare`; host 48 kHz decimated 3:1 to
     16 kHz; each 512-sample window is prepended with 64 context samples (the previous window's
     tail — **required** by Silero v5, else clean speech scores as non-speech) to a 576-sample
     input, run with threaded state → speech probability drives a hysteretic,
     hold-and-release-smoothed gain applied in place; latency 0; 48 kHz auto-bypass; inline per
     ADR-0014.
   - Verified:
     - Unit tests (`make ci`): pure gate-mapping — opens above threshold, holds, then closes to the
       floor (deterministic, model-free).
     - `tests/contract.rs` (`make ci`): non-48k passthrough, parameter clamping, state round-trip,
       zero latency, parameter surface.
     - `tests/integration.rs` (`#[ignore]`, `make test-models`): **opens on the real target-mic
       spoken-word fixture (gain 1.00) and closes on broadband noise (gain 0.09)** through the real
       model, general battery (latency-scoped), bounded inline allocation.

- Exit: both effects pass the general battery + class-specific tests; alloc/compute bench
  recorded; `make ci` green.

### M6 — Fixtures
**`[DECISION]`** Whether you supply your own spoken-word recordings (as with `vocal_spoken.wav`)
or I source public-domain / CC0 clips.
Source additional **public-domain / CC0** spoken-word WAVs (LibriVox, US-gov) for the
speech-centric effects; record provenance in `testdata/audio/FIXTURES.md` before commit.
Can land incrementally alongside M1/M3/M5 as each effect needs material.
- Exit: each new fixture committed with provenance + license in `FIXTURES.md`; `make ci` green.

## Open risks
- SwiftF0 allocation behavior under per-plugin streaming use (M2 resolves).
- SwiftF0 fmax 2094 Hz: fine for voice f0, but anything needing higher-frequency pitch tracking
  would need a fallback — none of the in-scope effects appear to.
- Realtime-safe FFT for Tier-2/HNR: no allocation-free STFT exists to reuse — `pitch-shift` and
  `onset-detect` both plan-allocate a `RealFftPlanner` per call. A new preallocated-scratch
  realfft STFT is built in M3 as its first consumer.
- **Musical→speech retuning is itself a risk:** SwiftF0's voicing/confidence behavior and the
  onset flux engine were validated on sustained pitched instruments. Their speech behavior
  (short voiced bursts, unvoiced consonants, plosive transients) must be validated on
  spoken-word fixtures (M2), and may need parameter work beyond defaults.
