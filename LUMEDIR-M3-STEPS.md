# Lúmedir — M3 — Execution Steps

Step-level expansion of **M3** from [`LUMEDIR-VST-PLAN.md`](LUMEDIR-VST-PLAN.md): *Clarity + the
delivery snapshot.* Depends on **M1** (`SpeakingRateEstimator`) and **M2** (`PitchDynamism`,
`PauseStructure`). This is the **integration milestone**: it adds the last estimator (clarity) and
wires all four into a single delivery snapshot **produced and consumed off the audio thread**.

> Clarity score from voicing ratio + onset sharpness (`onset_flux_high`); assemble the complete
> delivery snapshot (rate/WPM, dynamism, pauses, clarity) streamed to the editor.
> **Exit:** the clarity metric behaves sensibly across fixtures; a complete delivery snapshot is
> produced and consumed off the audio thread.

Run these in order. Each step names its file(s), the reference behavior to re-derive from a
source-of-truth (not memory), the minimal change, and a red→green test.

## Decided architecture (internal — not a user `[DECISION]`)

M3's only open question is *how* the four estimators run off the audio thread and reach the editor.
The estimators have different inputs: **rate** and **pauses** consume **audio** (they compute their
own envelope/energy); **dynamism** consumes voiced **f0**; **clarity** consumes **voicing_state +
onset_flux_high**. f0/voicing/onset come from `SignalAnalyzer` (`SignalSnapshot`). Because rate and
pauses need audio *off the audio thread*, and the editor (UI thread) has no audio, the aggregation
**must run on a worker thread that owns the audio**.

**Decision (least-future-mistake, boundary-respecting):** a **plugin-local `DeliveryWorker`** that
owns the off-thread transport (a lock-free SPSC audio ring + a background thread + an atomic-bits
publish — *modeled on the proven [`AnalysisWorker`](../speech/signals/src/worker.rs)*), holds a
**`SignalAnalyzer`** (reused) and a **`DeliveryAggregator`**, and publishes a **`DeliverySnapshot`**.
On its thread it drains audio in **small chunks** (≈ one analysis frame each, so the per-chunk
`SignalSnapshot` approximates the per-frame stream the M1/M2 estimators were validated on), runs the
analyzer, feeds the chunk audio to rate/pauses and the snapshot signals to dynamism/clarity,
assembles the snapshot, and publishes it. This keeps **all delivery code plugin-local** (M0
decision, ADR-0012) and reuses the analysis brain (`SignalAnalyzer`); the only thing reimplemented is
the thin transport, because the published payload is richer than `AnalysisWorker`'s `SignalSnapshot`.
It **replaces** the M0 `AnalysisWorker` in the plugin (Step 4) — a plan-specified evolution, not a
new parallel worker.

**No user `[DECISION]` in this phase.** The clarity blend weights / normalization and the worker's
chunk size are internal tuning, set per step and validated against fixtures in Step 5 (clarity is a
**product requirement** per AGENTS — tune the algorithm, do not weaken the assertion).

## Context / reuse map (re-derived for M3)

- *Clarity — reference:* "voicing ratio + onset sharpness." **Voicing ratio** = voiced ÷ (voiced +
  unvoiced) frames from `voicing_state` (0 silence / 1 unvoiced / 2 voiced) — silence excluded so
  pauses don't penalize clarity. **Onset sharpness** = a normalized aggregate of `onset_flux_high`
  over non-silent frames (crisper articulation reads sharper). Clarity = a bounded [0,1] blend.
- *Reuse as-is:* `SignalAnalyzer` (`speech/signals/src/analyzer.rs`) → `SignalSnapshot`
  (`pitch_hz`, `voicing_state`, `onset_flux_high`, …); the M1/M2 estimators (`SpeakingRateEstimator`,
  `PitchDynamism`, `PauseStructure`); `AnalysisWorker`'s lock-free SPSC ring + thread + atomic
  publish as the **model** for `DeliveryWorker` (and its `sync-analysis`/`test-sync-analysis`
  inline-test pattern). `lindelion_pitch_detect` / `lindelion_sample_library` (already dev-deps) for
  the fixture test.
- *Validation:* `FIXTURES.md` — `speech_clean_continuous_48k` (clean) vs `speech_noisy_48k`
  (matched pair at 10 dB SNR). Clean speech is clearer (higher voicing ratio; noise lowers voiced
  confidence), so clarity(clean) > clarity(noisy) is the sensible ranking.
- *Source-of-truth ADRs:* [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) — the audio
  thread only `push`es into the ring (allocation-free, non-blocking); all analysis/aggregation is
  off-thread; the editor reads the published snapshot, never the audio thread.
  [ADR-0012](docs/adr/0012-speech-effect-port-shared-workspace.md) — speech-specific delivery logic
  stays plugin-local, not in the shared crates.

---

## 1. Build the `Clarity` component (voicing ratio + onset sharpness)

- **File(s):** `plugins/lumedir/src/clarity.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `pub mod clarity;`).
- **Reference behavior:** the clarity method above. A pure per-frame accumulator: `update(voicing_state,
  onset_flux_high)` tallies voiced / unvoiced / silence frames and accumulates `onset_flux_high`
  over non-silent frames; `clarity()` returns a bounded [0,1] blend of the **voicing ratio** (voiced
  ÷ (voiced + unvoiced)) and a **normalized onset sharpness** (mean onset flux mapped to [0,1] via a
  soft saturation `x/(x+k)` with a reference `k`). Weights and `k` are configurable.
- **Change:** add `ClarityConfig { voicing_weight, onset_weight, onset_reference }` (Default
  guidance: 0.6 / 0.4 / a flux reference tuned in Step 5) and `Clarity` with `new(config)`, `reset`,
  `update(&mut self, voicing_state: f32, onset_flux_high: f32)`, `clarity(&self) -> f32` (0.0 with
  no frames; always within [0,1]). Register the module in `lib.rs`.
- **Verify:** a fast, in-memory **synthetic** unit test (in `make ci`): all-voiced (`state == 2`)
  frames with strong onset flux read a **high** clarity; mostly-unvoiced (`state == 1`) frames with
  weak flux read a **low** clarity; the clear case reads **strictly above** the unclear case; and
  the score is always within [0,1]. **Red:** `Clarity` doesn't exist → won't compile. **Green:**
  `cargo test -p lumedir`.

---

## 2. Build `DeliverySnapshot` + `DeliveryAggregator` (pure assembly)  [depends on #1]

- **File(s):** `plugins/lumedir/src/delivery.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `pub mod delivery;`).
- **Reference behavior:** the complete delivery snapshot — rate, WPM, dynamism, pauses, clarity — as
  a plain value, plus a pure aggregator that owns the four estimators and feeds each its input from
  one `(audio chunk, SignalSnapshot)` pair: rate/pauses get the audio; dynamism gets `Some(pitch_hz)`
  when `voicing_state == 2` else `None`; clarity gets `(voicing_state, onset_flux_high)`. No
  threading here — this is the pure aggregation, unit-testable without the worker.
- **Change:** add `#[derive(Clone, Copy, Default)] DeliverySnapshot { syllables_per_second,
  words_per_minute, pitch_dynamism_semitones, pause_fraction, pause_count, clarity }`; and
  `DeliveryAggregator` holding `SpeakingRateEstimator`/`PitchDynamism`/`PauseStructure`/`Clarity` + a
  `syllables_per_word` factor (default `DEFAULT_SYLLABLES_PER_WORD`), with `new(sample_rate, config)`,
  `reset`, `update(&mut self, audio: &[f32], signal: &SignalSnapshot)`, and `snapshot(&self) ->
  DeliverySnapshot`. Register the module.
- **Verify:** a fast, in-memory **synthetic** unit test (in `make ci`): drive the aggregator with a
  short synthetic audio block plus a constructed `SignalSnapshot` (voiced, with a pitch + onset
  flux) and assert the resulting `DeliverySnapshot` carries the expected fields — finite, in range,
  and consistent with the sub-estimators (e.g. WPM = syllables/min ÷ factor; clarity in [0,1]).
  **Red:** `DeliveryAggregator`/`DeliverySnapshot` don't exist → won't compile. **Green:**
  `cargo test -p lumedir`.

---

## 3. Build the off-thread `DeliveryWorker`  [depends on #2]

- **File(s):** `plugins/lumedir/src/delivery_worker.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `mod delivery_worker;` + re-export), `plugins/lumedir/Cargo.toml` (the `test-sync-analysis`
  feature already exists; reuse it to gate the inline path).
- **Reference behavior:** model the transport on `AnalysisWorker` (`speech/signals/src/worker.rs`):
  a lock-free SPSC `AtomicU32`-bits ring for audio (audio thread → worker, `push` allocation-free &
  non-blocking), a background thread draining in **small chunks**, and an atomic-bits publish of the
  output read by `latest_delivery()`. The worker holds a `SignalAnalyzer` and a `DeliveryAggregator`;
  per drained chunk it runs `analyzer.process(chunk)` → `SignalSnapshot`, `aggregator.update(chunk,
  &snapshot)`, then publishes `aggregator.snapshot()`. Mirror `AnalysisWorker`'s `sync-analysis`
  pattern: under `cfg(feature = "test-sync-analysis")`, `push` runs the analyze+aggregate+publish
  inline so a test reads a deterministic snapshot with no thread/sleep. Also expose the underlying
  `latest_snapshot() -> SignalSnapshot` for continuity with M0. `Drop` stops and joins the thread.
- **Change:** add `DeliveryWorker` with `new(sample_rate, config)`, `push(&self, block: &[f32])`,
  `latest_delivery(&self) -> DeliverySnapshot`, `latest_snapshot(&self) -> SignalSnapshot`, the
  `sync`/off-thread split, and `Drop`. Re-export it from `lib.rs`. No plugin changes yet (Step 4).
- **Verify:** **(a)** an allocation-free unit test (in `make ci`, default/off-thread build):
  `assert_no_allocations` around `push` + `latest_delivery` (mirroring the speech worker's
  `push_and_latest_are_allocation_free`). **(b)** a deterministic end-to-end test gated behind
  `test-sync-analysis` (inline; runs SwiftF0 ONNX inline, like the M0 worker test) — feed a voiced
  tone, assert `latest_delivery()` carries sane fields (e.g. clarity in [0,1], a non-zero rate or a
  voiced-consistent dynamism) — run via the existing `make test-integration` lumedir line
  (`--features test-sync-analysis`). **Red:** `DeliveryWorker` doesn't exist → won't compile.
  **Green:** `cargo test -p lumedir` (alloc test) and `cargo test -p lumedir --test … --features test-sync-analysis`.

---

## 4. Wire the plugin to the `DeliveryWorker` (replace the M0 `AnalysisWorker`)  [depends on #3]

- **File(s):** `plugins/lumedir/src/plugin.rs`, `plugins/lumedir/tests/integration.rs` (update the
  M0 worker test to the new worker).
- **Reference behavior:** the M0 plugin feeds `Option<AnalysisWorker>` and exposes `latest_snapshot`.
  M3 replaces that worker with the `DeliveryWorker`: `reset` builds it at the host sample rate; the
  audio thread still feeds it the mono mix via the pre-sized scratch (allocation-free, unchanged
  shape); the plugin exposes `latest_delivery() -> DeliverySnapshot` (the editor's read path, M4) and
  keeps `latest_snapshot()` delegating to the worker. The audio path stays a bit-exact, 0-latency
  passthrough.
- **Change:** swap the `worker: Option<AnalysisWorker>` field for `Option<DeliveryWorker>`; update
  `reset`/`process`/`latest_snapshot`; add `latest_delivery`. Update `tests/integration.rs` so the
  M0 worker-yields-a-snapshot test drives the `DeliveryWorker` (still under `test-sync-analysis`),
  asserting the voiced/silence `SignalSnapshot` as before.
- **Verify:** the M0 passthrough+feed unit test still passes (bit-exact stereo passthrough,
  allocation-free `process` with the new worker fed) in `make ci`; the updated M0 worker test passes
  under `make test-integration`. **Red:** before the swap, `latest_delivery`/`DeliveryWorker` aren't
  wired into the plugin (the new assertions don't compile). **Green:** `cargo test -p lumedir` and
  `cargo test -p lumedir --test integration --features test-sync-analysis -- --include-ignored`.

---

## 5. Validate clarity + the complete snapshot across fixtures  [depends on #1, #2]

- **File(s):** `plugins/lumedir/tests/delivery_fixtures.rs` (new), `Makefile` (`test-models` target).
- **Reference behavior:** the **phase exit**. Run `SignalAnalyzer` over the clean vs noisy matched
  pair (feeding it in small chunks, reading its `SignalSnapshot` per chunk, driving a
  `DeliveryAggregator` alongside the chunk audio), and assert (i) **clarity behaves sensibly** —
  `clarity(speech_clean_continuous) > clarity(speech_noisy)` (clean is clearer; noise lowers the
  voicing ratio) — and (ii) a **complete `DeliverySnapshot`** is produced with every field finite and
  in range (rate ≥ 0, dynamism ≥ 0, pause_fraction in [0,1], clarity in [0,1]). ONNX over whole
  fixtures → heavy → `#[ignore]`d, run via `make test-models`. Tune `ClarityConfig` here (weights /
  onset reference) so the ranking holds and the score uses its range — do not weaken the assertion.
- **Change:** write `tests/delivery_fixtures.rs` (the M2 fixture-loader pattern) with a test
  annotated `#[ignore = "runs SwiftF0 ONNX over fixtures; run via make test-models"]` asserting the
  clarity ranking and the complete-snapshot invariants. Add a `make test-models` line:
  `cargo test -p lumedir --test delivery_fixtures -- --include-ignored`.
- **Verify:** **Red:** the test file doesn't exist (and it's `#[ignore]`d out of `make ci`).
  **Green:** `cargo test -p lumedir --test delivery_fixtures -- --include-ignored` (i.e.
  `make test-models`) ranks clean > noisy and produces a complete, in-range snapshot; `make ci`
  skips it. If the ranking is off, **return to Step 1** and tune the clarity blend.

---

## Phase exit checklist

- [ ] `make ci` green (clarity + aggregator + alloc-free worker unit tests run; fixture/worker
      inline tests are ignored).
- [ ] `make test-integration` (the lumedir line): the `DeliveryWorker` produces a sane
      `DeliverySnapshot` from fed audio (inline), and the M0 worker test still passes.
- [ ] `make test-models` (the lumedir line): `clarity(clean) > clarity(noisy)` and a complete,
      in-range `DeliverySnapshot` is produced.
- [ ] The plugin exposes `latest_delivery()` — the delivery snapshot is produced on the worker
      thread and consumed off the audio thread (the editor view itself is M4).

Then expand **M4** (the Vizia live running-readout view, reading `latest_delivery()`) with
`plan-phase`.
