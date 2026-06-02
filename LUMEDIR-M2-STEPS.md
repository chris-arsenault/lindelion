# Lúmedir — M2 — Execution Steps

Step-level expansion of **M2** from [`LUMEDIR-VST-PLAN.md`](LUMEDIR-VST-PLAN.md): *Pitch dynamism +
pause structure.* Depends on **M0** (the `plugins/lumedir` crate; the worker feed). Independent of
M1 — these are two more plugin-local delivery estimators alongside the speaking-rate one.

> Windowed voiced-f0 → standard deviation in semitones (flat↔animated); pause structure from
> `voicing_state` silence runs (pause fraction + count/length).
> **Exit:** pitch-dynamism ranks flat (1.1) vs animated (7.4) strongly and lands near the
> `FIXTURES.md` `pstd` targets; pause metrics are correct on the pauses fixture.

Run these in order. Each step names its file(s), the reference behavior to re-derive from a
source-of-truth (not memory), the minimal change, and a red→green test.

**Where this lives (resolved at M0):** plugin-local components in `plugins/lumedir`, reusing the
pure analysis primitives. Like M1's `SpeakingRateEstimator`, each is a pure component validated
offline; wiring them into the live off-thread delivery snapshot streamed to the editor is **M3**.

**No `[DECISION]` in this phase.** M2 carries no plan-level decision. The internal tuning constants
(confidence/window for dynamism; frame size, silence threshold, min-pause for pauses; the fixture
tolerances) are mine to set — guidance values are given per step and tuned against the fixtures in
Steps 3–4. Treat the estimators as **product requirements** (AGENTS "required DSP algorithms"): if a
fixture step fails to rank/land, work the algorithm and re-tune the component's constants — do not
weaken the assertion.

## Context / reuse map (re-derived for M2)

- *Pitch dynamism — reference:* the spread of voiced pitch in **semitones**. A frame's pitch in
  semitones is `12·log2(f0_hz)` (any reference cancels in a standard deviation); dynamism = the std
  over the recent **voiced** frames. `FIXTURES.md` `pstd` = "pitch standard deviation in semitones
  (flat ↔ animated)": `speech_flat_48k` **1.1**, `speech_animated_48k` **7.4**.
- *Pause structure — reference:* `FIXTURES.md` `pause` = "fraction of **low-energy frames**":
  `speech_pauses_48k` **0.26** (vs continuous fixtures ≈ 0.00–0.14). The plan frames this as
  `voicing_state` silence runs; the *silence* component of `voicing_state` is exactly a per-frame
  low-energy test (`SignalAnalyzer` marks silence when frame RMS < a floor), so the estimator
  reproduces the **energy-frame** method directly from audio — no pitch/voicing model needed.
- *Reuse as-is:* `lindelion_pitch_detect::SwiftF0StreamingPitchTracker` (`new(sample_rate, PitchDetectionConfig)`,
  `next_block(&[f32]) -> Result<&[PitchFrame]>`; `PitchFrame { f0_hz: Option<f32>, voiced, confidence, … }`,
  with `f0_hz = Some` only for confident voiced frames) — the **f0 source** for the dynamism fixture
  test and (M3) the live path. The model is embedded (`include_bytes!`), so the tracker needs no
  external file, but it is **ONNX inference** → heavy → `make test-models`. `lindelion_dsp_utils`
  `EnvelopeFollower`/`analysis::rms` for the pause energy framing. `lindelion_sample_library::decode_wav_mono`
  (`wav-decoder`, already a dev-dep) for loading fixtures.
- *Source-of-truth ADRs:* [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) — both run off
  the audio thread (M3 wires them), so not required allocation-free here; use bounded ring/​counters.
  [Heavy-test rule (AGENTS):] NN model tests are `#[ignore]`d and run via `make test-models`;
  filesystem tests are gated behind `integration-tests` and run via `make test-integration`.

---

## 1. Build the `PitchDynamism` component (windowed voiced-f0 → semitone std)

- **File(s):** `plugins/lumedir/src/pitch_dynamism.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `pub mod pitch_dynamism;`).
- **Reference behavior:** the semitone-std method above. A pure windowed-statistics component: it
  consumes per-frame voiced f0 (the f0 source is SwiftF0, supplied by the caller — Step 3's test and
  M3's live path), converts each voiced, pitched frame to `12·log2(f0_hz)`, keeps the last
  `window_voiced_frames` of them in a bounded ring, and reports their standard deviation. Unvoiced /
  unpitched frames (`f0_hz == None`) contribute nothing.
- **Change:** add `PitchDynamismConfig { window_voiced_frames: usize }` (Default ~2000 — covers the
  ~5 s fixtures whole, bounds the live window) and `PitchDynamism` with `new(config)`, `reset`,
  `push_frame(&mut self, f0_hz: Option<f32>)`, and `semitone_std(&self) -> f32` (0.0 with < 2
  values). Register the module in `lib.rs`. No audio framing, no plugin/worker wiring (M3).
- **Verify:** a fast, in-memory **synthetic** unit test (in `make ci`): a constant-f0 frame
  sequence reads `semitone_std ≈ 0`; a sequence alternating between two pitches a known interval
  apart reads the expected std (e.g. 200 Hz / 220 Hz alternating → std ≈ `12·log2(1.1)/2 ≈ 0.83`
  st); and a small-jitter "flat" sequence reads **strictly below** a large-swing "animated"
  sequence. **Red:** `PitchDynamism` doesn't exist → won't compile. **Green:** `cargo test -p lumedir`.

---

## 2. Build the `PauseStructure` component (energy-frame silence runs)

- **File(s):** `plugins/lumedir/src/pause_structure.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `pub mod pause_structure;`).
- **Reference behavior:** the low-energy-frame method above. The component consumes **audio**, frames
  it at `frame_s`, computes each frame's RMS in dB, and marks the frame **silent** when it falls
  below the silence threshold (relative to the running intensity maximum, so it is level-robust);
  it tracks the total/silent frame counts and the runs of consecutive silent frames. `pause_fraction`
  = silent ÷ total frames; a silence run counts as a **pause** only if it lasts ≥ `min_pause_s`
  (so brief inter-word gaps are not pauses); `pause_count`/`mean_pause_s`/`total_pause_s` summarize
  the runs.
- **Change:** add `PauseStructureConfig { frame_s, silence_threshold_db, min_pause_s }` (Default
  guidance: `frame_s ≈ 0.02`, `silence_threshold_db ≈ 35`, `min_pause_s ≈ 0.2`) and `PauseStructure`
  with `new(sample_rate, config)`, `reset`, `push(&mut self, block: &[f32])`, `pause_fraction`,
  `pause_count`, `mean_pause_s`, `total_pause_s`. Cumulative over the fed audio (a session/clip),
  reset per session. Register the module in `lib.rs`.
- **Verify:** a fast, in-memory **synthetic** unit test (in `make ci`): feed a tone–silence–tone
  signal with a known silent fraction (e.g. 2 s tone, 1 s silence, 2 s tone → `pause_fraction ≈ 0.2`,
  `pause_count == 1`, `mean_pause_s ≈ 1.0`); and a continuous tone reads `pause_fraction ≈ 0` with
  `pause_count == 0`. **Red:** `PauseStructure` doesn't exist → won't compile. **Green:**
  `cargo test -p lumedir`.

---

## 3. Validate pitch dynamism against the flat/animated fixtures (SwiftF0, model test)  [depends on #1]

- **File(s):** `plugins/lumedir/Cargo.toml` (dev-dep `lindelion-pitch-detect`),
  `plugins/lumedir/tests/dynamism_fixtures.rs` (new), `Makefile` (`test-models` target).
- **Reference behavior:** the dynamism half of the **phase exit**. Run `SwiftF0StreamingPitchTracker`
  over each fixture (feeding `next_block` in chunks, collecting every `PitchFrame`), push each
  frame's `f0_hz` into a `PitchDynamism`, and read `semitone_std`. The firm requirement is **strong
  ranking** — `speech_flat_48k` (pstd 1.1) reads **far below** `speech_animated_48k` (pstd 7.4); the
  softer one is **proximity** to those targets (absolute scale may differ from the fixture-table
  tracker, so use a generous tolerance). This is ONNX inference → heavy → `#[ignore]`d, run via
  `make test-models` (per the NN-test rule), **not** `make ci`/`make test-integration`.
- **Change:** add the `lindelion-pitch-detect` dev-dep (it is already a transitive dep via
  `lindelion-speech-signals`, so no new build cost — the dev-dep just makes it nameable). Write
  `tests/dynamism_fixtures.rs` with a fixture→frames helper and a test annotated
  `#[ignore = "runs SwiftF0 ONNX over fixtures; run via make test-models"]` asserting `flat`
  dynamism ≪ `animated` (a strong gap, e.g. `animated > flat + 3.0` and `animated > 2·flat`) and
  that each lands within tolerance of its target (start generous, tighten to the measured values
  while keeping the strong ranking). Add a `make test-models` line:
  `cargo test -p lumedir --test dynamism_fixtures -- --include-ignored`.
- **Verify:** **Red:** the test file / dev-dep don't exist (and the test is `#[ignore]`d out of
  `make ci`). **Green:** `cargo test -p lumedir --test dynamism_fixtures -- --include-ignored`
  (i.e. `make test-models`) ranks flat ≪ animated and lands each near its `pstd` target; `make ci`
  skips it. If ranking/proximity is off, **return to Step 1** and tune (confidence/window) — do not
  relax the assertion.

---

## 4. Validate pause structure against the pauses fixture  [depends on #2]

- **File(s):** `plugins/lumedir/tests/pause_fixtures.rs` (new). *(No Cargo/Makefile change: the M1
  `cargo test -p lumedir --features integration-tests` line in `test-integration` already runs every
  `integration-tests`-gated lumedir test; `lindelion-sample-library` is already a dev-dep.)*
- **Reference behavior:** the pause half of the **phase exit**. Feed each fixture through a
  `PauseStructure` and compare to `FIXTURES.md`: `speech_pauses_48k` `pause` **0.26**, and it must
  read **clearly above** a continuous fixture (`speech_clean_continuous_48k` ≈ 0.12,
  `speech_noisy_48k` ≈ 0.00). Heavy (file I/O) → gated behind `integration-tests`, run via
  `make test-integration`, **not** `make ci`.
- **Change:** write `tests/pause_fixtures.rs` (the M1 `fixture()` loader pattern) with a test
  annotated `#[cfg_attr(not(feature = "integration-tests"), ignore = "loads wav fixtures; run via make test-integration")]`
  asserting `speech_pauses_48k` pause_fraction is within tolerance of 0.26 (start ±0.1; tighten to
  the measured value) and reads **strictly above** `speech_clean_continuous_48k`, plus that the
  pauses fixture yields ≥ 1 pause run. If the fraction is off, **return to Step 2** and tune
  (`silence_threshold_db`, `frame_s`) so the pauses fixture lands at ~0.26 while continuous fixtures
  stay low — do not relax the assertion.
- **Verify:** **Red:** the test file doesn't exist (and `integration-tests` leaves it ignored).
  **Green:** `cargo test -p lumedir --features integration-tests` (i.e. `make test-integration`)
  lands the pauses fixture near 0.26 and above the continuous fixture; `make ci` skips it.

---

## Phase exit checklist

- [ ] `make ci` green (the synthetic dynamism + pause unit tests run; both fixture tests are ignored).
- [ ] `make test-models` (the lumedir line): pitch dynamism ranks `speech_flat` ≪ `speech_animated`
      and lands near the `pstd` targets (1.1 / 7.4).
- [ ] `make test-integration` (the lumedir line): `speech_pauses` pause_fraction ≈ 0.26 and reads
      above `speech_clean_continuous`; ≥ 1 pause run.

Then expand **M3** (clarity + assembling the complete delivery snapshot, streamed to the editor)
with `plan-phase`. The estimators stay pure components here; M3 wires rate/WPM (M1), dynamism +
pauses (M2), and clarity into the off-thread snapshot the editor reads.
