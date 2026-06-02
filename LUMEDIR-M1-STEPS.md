# Lúmedir — M1 — Execution Steps

Step-level expansion of **M1** from [`LUMEDIR-VST-PLAN.md`](LUMEDIR-VST-PLAN.md): *Speaking-rate
(syllable-nuclei) estimator — the largest greenfield piece.* Depends on **M0** (the `plugins/lumedir`
crate + the analysis-worker feed, both built).

> Envelope-peak / syllable-nuclei detector → syllables/min over a sliding window (off-thread);
> WPM = rate ÷ a configurable syllables-per-word factor.
> **Exit:** the estimator ranks the slow (2.8) vs fast (3.8) vs clean fixtures correctly and lands
> near their `FIXTURES.md` `syl/s` targets within tolerance; WPM derives from it.

Run these in order. Each step names its file(s), the reference behavior to re-derive from a
source-of-truth (not memory), the minimal change, and a red→green test.

**Where this lives (resolved at M0):** the delivery estimators are **plugin-local** (in
`plugins/lumedir`), not promoted into shared `lindelion-speech-signals`. So the estimator is a
`plugins/lumedir` module, reusing only the pure DSP primitives from `lindelion-dsp-utils`.

**No `[DECISION]` in this phase.** M1 carries no plan-level decision. The internal tuning constants
(silence/dip thresholds, smoothing/window lengths, the default syllables-per-word factor, the
fixture tolerance) are mine to set — guidance values are given per step and validated/tuned against
the fixtures in Step 3. Treat the syllable-nuclei detector as a **product requirement** (AGENTS
"required DSP algorithms"): if Step 3's ranking/proximity fails, work the algorithm and re-tune
Step 1's constants — do **not** weaken the assertions or swap in a simpler proxy.

## Context / reuse map (re-derived for M1)

- *Reference method:* `testdata/audio/FIXTURES.md` defines `syl/s` as the **envelope-peak syllable
  rate** measured over the whole clip (nuclei ÷ clip seconds — e.g. the pauses fixture's 3.5 syl/s
  is over total time, pauses included). The standard envelope-peak nuclei method (De Jong & Wempe,
  *"Praat script to detect syllable nuclei"*): take the **intensity envelope**, keep **local maxima
  above a silence threshold** (relative to the signal's intensity maximum) that are **separated from
  the previous nucleus by an intensity dip** of at least a few dB. No committed generator exists —
  reproduce the *method*, validated by ranking + proximity, not a bit-exact number.
- *Reuse as-is:* `lindelion_dsp_utils::envelope_follower::{EnvelopeFollower, DetectorMode}` (RMS
  one-pole) for the intensity envelope; `lindelion_sample_library::decode_wav_mono` (behind
  `wav-decoder`) for loading fixtures in the integration test, mirroring the speech crates'
  `fixture()` helper (`env!("CARGO_MANIFEST_DIR")/../../testdata/audio`).
- *Validation targets (`FIXTURES.md`):* `speech_slow_48k` **2.8**, `speech_fast_48k` **3.8**,
  `speech_clean_continuous_48k` **3.7** syl/s (mono 48 kHz, ~5 s clips).
- *Source-of-truth ADRs:* [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) — the estimator
  runs **off the audio thread** (M3 wires it into the worker), so it need not be allocation-free
  here, but use a **bounded, preallocated** nucleus ring so the M3 live path stays clean.

---

## 1. Build the `SpeakingRateEstimator` core (envelope → online nuclei → sliding-window rate)

- **File(s):** `plugins/lumedir/src/speaking_rate.rs` (new), `plugins/lumedir/src/lib.rs` (add
  `pub mod speaking_rate;`).
- **Reference behavior:** the envelope-peak nuclei method above. Intensity envelope = a smoothed
  energy contour via `EnvelopeFollower::new(DetectorMode::Rms)` with `set_times` chosen to pass the
  syllabic band (~3–8 Hz) and smooth the pitch period (start ~10 ms attack / ~40 ms release),
  converted to dB. **Online** nucleus detection (no look-ahead over the whole file): track a running
  intensity **maximum that decays over the window** (the relative silence reference); confirm a
  nucleus when, after a local peak that is within `silence_threshold_db` of that maximum, the
  envelope **dips by ≥ `min_dip_db`** below the peak, provided ≥ `min_nucleus_interval_s` has passed
  since the last nucleus (a refractory bound on max rate). Record each nucleus's sample index in a
  **bounded ring** sized to the window. `syllables_per_second()` = nuclei within the last `window_s`
  ÷ `min(elapsed, window_s)` seconds; `syllables_per_minute()` = that × 60. Starting constants
  (De Jong-derived; tune in Step 3): `silence_threshold_db ≈ 25`, `min_dip_db ≈ 2`,
  `min_nucleus_interval_s ≈ 0.10`, `window_s ≈ 8.0`.
- **Change:** add `SpeakingRateConfig` (the constants above, with `Default`) and
  `SpeakingRateEstimator` with `new(sample_rate: f32, config: SpeakingRateConfig)`, `reset`,
  `push(&mut self, block: &[f32])`, `syllables_per_second`, `syllables_per_minute`. No WPM yet
  (Step 2); no plugin/worker wiring yet (M3). Register the module in `lib.rs`.
- **Verify:** a fast, in-memory **synthetic** unit test (in `make ci`): synthesize a voiced carrier
  (~160 Hz) amplitude-modulated by raised-cosine "syllable" bumps at a known rate over a few
  seconds — e.g. 4 bumps/s for 4 s — and assert `syllables_per_second()` is within ±0.5 of 4.0;
  then a 6 bumps/s signal reads a **strictly higher** rate (ranking). **Red:** `SpeakingRateEstimator`
  doesn't exist → won't compile. **Green:** `cargo test -p lumedir`.

---

## 2. Derive WPM from the rate  [depends on #1]

- **File(s):** `plugins/lumedir/src/speaking_rate.rs`.
- **Reference behavior:** the plan's `WPM = syllables/min ÷ a configurable syllables-per-word
  factor`. The factor is a per-plugin setting (persisted in M6); English averages ≈ 1.4–1.5
  syllables/word, so the default is **1.5** (my call — an internal tuning constant, not a user
  decision). WPM is a pure transform of Step 1's rate; no new signal processing.
- **Change:** add `pub const DEFAULT_SYLLABLES_PER_WORD: f32 = 1.5;` and
  `words_per_minute(&self, syllables_per_word: f32) -> f32` = `syllables_per_minute() /
  syllables_per_word.max(ε)` (guard divide-by-zero). (Keep the factor a call/config parameter, not
  hard-wired, so the editor/state layer can override it later.)
- **Verify:** a unit test (in `make ci`): on a known rate (reuse the Step 1 synthetic), assert
  `words_per_minute(1.5)` equals `syllables_per_minute() / 1.5` within float tolerance (e.g. a
  4 syl/s signal → 240 syl/min → 160 WPM), and that a smaller factor yields a larger WPM. **Red:**
  `words_per_minute`/`DEFAULT_SYLLABLES_PER_WORD` don't exist → won't compile. **Green:**
  `cargo test -p lumedir`.

---

## 3. Validate against the spoken-word fixtures (ranking + proximity)  [depends on #1]

- **File(s):** `plugins/lumedir/Cargo.toml` (dev-dep `lindelion-sample-library` with `wav-decoder`),
  `plugins/lumedir/tests/rate_fixtures.rs` (new), `Makefile` (`test-integration` target).
- **Reference behavior:** this is the **phase exit**. Feed each real fixture through the estimator
  and compare to its `FIXTURES.md` `syl/s` target. The firm requirement is **ranking** —
  `speech_slow_48k` (2.8) reads **strictly less** than `speech_fast_48k` (3.8) and than
  `speech_clean_continuous_48k` (3.7); the softer one is **proximity** — each measured rate within a
  stated tolerance of its target. Mirror the speech crates' `fixture()` loader
  (`decode_wav_mono` → `samples` + `sample_rate`); feed the whole clip (the window ≥ clip length, so
  `syllables_per_second()` = nuclei ÷ clip seconds). Heavy (file I/O), so gate behind the per-crate
  `integration-tests` feature per the AGENTS unit-purity rule — **not** in `make ci`.
- **Change:** add the `lindelion-sample-library` dev-dep (`features = ["wav-decoder"]`); write
  `tests/rate_fixtures.rs` with a `fixture()` helper and a test, annotated
  `#[cfg_attr(not(feature = "integration-tests"), ignore = "loads wav fixtures; run via make test-integration")]`,
  that asserts the slow<fast and slow<clean ordering and that each of slow/fast/clean lands within
  tolerance (start at **±1.0 syl/s**; tighten if the detector supports it). Add a `make
  test-integration` line: `cargo test -p lumedir --features integration-tests` (this also leaves the
  existing `test-sync-analysis` worker test untouched — different feature). If ranking or proximity
  fails, **return to Step 1 and tune** the detector constants (envelope times, `silence_threshold_db`,
  `min_dip_db`, refractory) — optionally gating nuclei on a voiced/low-band-energy proxy — until the
  fixtures rank and land; do not relax the assertion.
- **Verify:** **Red:** the test file / dev-dep don't exist (and `integration-tests` leaves it
  ignored). **Green:** `cargo test -p lumedir --features integration-tests` ranks slow<fast<≈clean
  and lands each within tolerance; `make ci` skips it (ignored). Confirm both.

---

## Phase exit checklist

- [ ] `make ci` green (the synthetic rate + WPM unit tests run; the fixture test is ignored).
- [ ] `cargo test -p lumedir --features integration-tests` (via `make test-integration`): the
      estimator ranks `speech_slow` < `speech_fast` and `speech_slow` < `speech_clean_continuous`,
      and each lands within tolerance of its `FIXTURES.md` `syl/s` target (2.8 / 3.8 / 3.7).
- [ ] `words_per_minute` derives from the rate via the configurable factor (default 1.5).

Then expand **M2** (pitch dynamism + pause structure) with `plan-phase`. The estimator stays a pure
component here; wiring rate/WPM into the off-thread delivery snapshot streamed to the editor is
**M3**.
