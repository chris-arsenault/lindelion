# Calóma — Speech VST Implementation Plan

**Calóma** (Quenya `cala` "bright/clear" + `óma` "voice" — "clear voice") packages the ported
speech effects as a **single VST3**: a serial effect chain with a **3-valued signal-order
parameter** (each order being a curated chain topology with a tuned default configuration),
**normal VST patches** that capture the order plus the full parameter tuning, **compute-once**
shared analysis, and **gain staging** at order-defined points. The packaging decision is recorded
in [ADR-0020](docs/adr/0020-caloma-speech-vst-packaging.md); it refines
[ADR-0013](docs/adr/0013-host-agnostic-effect-core.md) (packaging now chosen) and resolves the
contingent shared-analysis milestone (M4 of the port plan).

The Windows host, Visualizer VST, and Speech Coach VST are separate future projects — see their
placeholder plans (`WINDOWS-HOST-PLAN.md`, `VISUALIZER-VST-PLAN.md`, `SPEECH-COACH-VST-PLAN.md`).

## Signal order and patches (how they relate)

- **Signal order** is a **multi-valued parameter** — one of **3** curated chain topologies
  (the *sequence* of effect slots + where gain-staging nodes sit). Each order has a **default
  parameter tuning** (produced by M5's end-to-end tuning).
- **Changing the signal-order parameter loads that order's default tuning** — i.e. selecting a new
  order resets the per-effect parameters to that order's tuned defaults. Orders are *starting
  points*.
- A **patch** is a full snapshot — the signal-order value **plus** the complete parameter tuning
  (every effect's params + per-slot enable/disable). **Loading a patch sets the order and the
  tuning together**; saving captures both. Patches are handled like normal VST patches via
  `lindelion-plugin-shell`'s parameter-state save/load.

So they are *not* orthogonal: the order lives inside the patch as a parameter, and it carries an
associated default tuning that a patch may have customized.

## Decisions

| # | Decision | Status |
| - | -------- | ------ |
| D1 | Product name | **RESOLVED: Calóma** (crate `plugins/caloma`) |
| D3 | ConvolutionReverb | **RESOLVED: dropped** from the product (ADR-0020) |
| D2 | The 3 preset signal orders (composition + gain-stage points) | Open — proposed in M3; confirm/adjust at M3 |
| D4 | Analysis tap point for the shared snapshot | Head, post-preprocessing (DC/HPF/pre-emphasis) |
| D5 | Latency on order/bypass change: fixed-max (always-compensated) vs dynamic | Fixed-max (host-friendly) |
| D6 | Default-tuning targets (integrated-loudness target; metric weights) | -16 LUFS; weights proposed in M5 |
| D7 | Editor scope: generic param list first vs custom Vizia editor | Generic first; Vizia editor as follow-on |

## Context / reuse map

- **DSP:** the 20 ported effect crates under `speech/` (`lindelion-speech-*`) are the chain modules,
  unchanged in behavior. (ConvolutionReverb is not ported; not in the chain — D3.)
- **Plugin boundary:** `lindelion-plugin-shell` — parameters, process context, control events,
  state, typed VST3 messages, **patch I/O** (the normal-VST-patch mechanism), voice allocation. The
  `vst3` crate provides the binding. This is the same path as `plugins/lamath|linnod|glirdir`
  ({patch model, runtime, VST3 adapter, tests}); **no new plugin framework** (ADR-0002 respected).
- **Analysis:** `lindelion-speech-signals` `SignalAnalyzer` (SwiftF0 voicing/onset/HNR) — run by a
  **single** `AnalysisWorker` owned by the chain (compute-once), not per effect. Inline signals
  (`SpeechPresence`/`SibilanceEnergy`/`FricativeActivity`) stay inline.
- **NN effects:** DFN3 denoiser + Silero voice-gate run their own inline inference (ADR-0018); they
  are chain slots, not analysis-signal consumers.
- **Editor:** `lindelion-ui` (Vizia editors) for the eventual custom editor.
- **Build:** `xtask` macOS VST3 bundle automation; `lindelion-plugin-metadata`. macOS-only VST3
  build today (ADR-0007); Windows is the separate host project.
- **Tests/fixtures:** `lindelion-fidelity` (general battery + the new FFT helpers); the M6
  spoken-word fixtures (clean / **noisy↔clean matched pair** / pauses / fast / slow / flat /
  animated); `make test-models` for heavy NN/chain tests.

## Architecture

```
input → [preprocess + ANALYSIS TAP once/block → SignalSnapshot] ─┐
                                                                 ▼ (snapshot injected)
   ordered chain of slots (per selected Signal Order):
     gain-stage → effect → effect → gain-stage → … → output limiter
   each slot: enabled? (patch) + params (patch); SwiftF0-consumers read the shared snapshot
output (latency = Σ active-slot latency, reported to host)
```

- One `AnalysisWorker` (shared) computes voicing/onset/HNR once; the chain injects the snapshot
  into the slots that need it. **No per-effect workers, no cross-effect bus.**
- Gain-staging nodes are part of each signal-order definition (topology-dependent).
- The runtime takes (order value + full parameter tuning) — both from the patch. Selecting an order
  loads its default tuning; the runtime then processes that order's topology with those params.

## Milestones

### M0 — crate scaffold + the patch model  [depends on: ADR-0020 ✓]
- ADR-0020 is written + indexed (decision resolved: Calóma; ConvolutionReverb dropped). No gate
  here — proceed.
- Register `plugins/caloma` as a workspace member (the reserved dir + README already exist) and
  build out: the **signal-order parameter** (3 enumerable topology specs, each with a slot for its
  default tuning) and the **patch model** = {order value + per-effect params + per-slot enable},
  serialized via plugin-shell patch I/O. Selecting an order loads that order's default tuning. No
  chain DSP yet.
- Verify: the 3 order specs enumerate; a patch round-trips (order + params) through plugin-shell
  patch I/O; changing the order parameter loads that order's defaults; greenfield red → green.
  `make ci` green (registering the member must keep the build green).

### M1 — Compute-once shared analysis (resolves port-plan M4)  [depends on M0]
- Refactor the SwiftF0-consuming effects (`bass-enhancer`, `consonant-transient`, `dynamic-eq`,
  `upward-expander`) to accept an injected `&SignalSnapshot` instead of embedding `AnalysisWorker`.
  The chain owns one shared `AnalysisWorker`; computes the snapshot once/block; injects it.
- Retire the per-effect workers and the `sync-analysis` test feature (tests now inject known
  snapshots → deterministic by construction).
- Verify: a chain of all four consumers instantiates exactly **one** analyzer (assert); each
  effect's output under an injected snapshot matches its prior self-derived output on a fixture
  (characterization test, within tolerance); `make ci` green; `make test-models` green.

### M2 — Chain runtime: ordered processing, per-slot bypass, gain staging, latency  [depends on M1]
- Process the selected order's slots in sequence; per-slot enable from the patch; gain-staging
  nodes at the order-defined points; aggregate + report latency (Σ active-slot latency; DFN3 1920 +
  STFT frames dominate). Latency strategy per D5.
- Verify: e2e chain on speech is finite + non-clipping; per-slot bypass equals identity for that
  slot; reported latency equals the active sum; selecting a different order re-routes correctly.

### M3 — The 3 preset signal orders + normal-VST patches  [depends on M2]  **`[DECISION]`** (D2)
- Define the 3 built-in order specs (gain-stage points included). **Proposed (confirm):**
  - **Order 1 — Clarity (default):** HPF → Noise Gate → DFN3 Denoiser → Dereverb → FFT Noise
    Removal → De-esser → 5-Band EQ → Dynamic EQ → Compressor → Upward Expander → Bass Enhancer →
    Air Exciter → Spectral Contrast → Consonant Transient → Vitalizer → Limiter. (repair → tonal →
    dynamics → enhance → limit)
  - **Order 2 — Broadcast:** repair (gate/denoise/dereverb/de-ess) → EQ → enhancement (bass/air/
    spectral/consonant/vitalizer) → Compressor → Upward Expander → Limiter. (enhance *before* the
    dynamics so the compressor controls the enhanced signal for consistent loudness)
  - **Order 3 — Light:** HPF → Voice Gate → DFN3 Denoiser → De-esser → gentle 5-Band EQ →
    Compressor → Limiter, with enhancement slots present but disabled by the default patch.
    (minimal, transparency-first)
- Each order ships with its M5-tuned default parameter set; selecting the order loads it. Wire
  patch save/load (normal VST behavior) — a patch stores the order value + the (possibly tweaked)
  tuning.
- Verify: each order builds a valid chain + processes; selecting an order loads its defaults; a
  saved-then-loaded patch restores the same order + tuning (round-trip).

### M4 — VST3 adapter + editor  [depends on M2]  **`[DECISION]`** (D7)
- VST3 adapter via `lindelion-plugin-shell` (params, state = order + patch, typed messages);
  editor — generic param list first, Vizia editor (`lindelion-ui`) as a follow-on.
- Verify: Steinberg validator passes (macOS, `make validate-vst3`); loads in a host; params
  automatable; order + patch persist across reload.

### M5 — Default parameter selection via full-chain e2e tuning  [depends on M2, M3]  **`[DECISION]`** (D6)
- The novel piece: pick each order's default patch by **optimizing measured full-chain output**,
  not by hand. Detailed in the next section.
- Verify: the tuning harness runs e2e on the battery + emits a ranked, seeded result; the selected
  defaults pass the quality gates; a committed-defaults regression test guards future degradation.

### M6 — Full-chain fidelity + integration tests  [depends on M3, M4, M5]
- E2e on speech, per order at its default patch: clarity up; noise down (SNR vs the matched
  clean↔noisy pair); dereverb on a synthetic-reverb variant; no artifacts; target loudness;
  correct latency. Heavy (NN) → `make test-models`.
- Exit: e2e gates green for all 3 orders; `make ci` green; VST3 validates on macOS.

## M5 in detail — default parameters from end-to-end chain audio tests

**Goal.** For each signal order, choose its **default parameter tuning** — the params loaded when
that order is selected — by running the **whole chain end-to-end** on the spoken-word fixture
battery and selecting the parameters that maximize measured output quality, subject to hard
no-artifact constraints. Re-runnable; the result is committed as that order's built-in default
tuning (and is therefore the starting point any user patch customizes).

**Fixtures (already sourced, M6 of the port).** clean, **noisy + its matched clean reference**,
pauses, fast, slow, flat, animated — plus a synthetic-reverb variant of a clean clip (feedback-comb,
as in the dereverb test).

**Per-candidate evaluation** (a candidate = one full parameter patch for one order):
1. Run the full chain e2e on every fixture (inline NN; off-thread analysis worker; deterministic).
2. **Hard constraints** — reject the candidate if any fixture violates: non-finite, clipping
   (peak > ~0.99), or pumping (the output gain envelope's variance exceeds a bound).
3. **Scored targets** (aggregate across fixtures; minimize deviation / maximize gain):
   - **Loudness:** integrated loudness near the target (D6, e.g. -16 LUFS) — penalize the deviation.
   - **Noise reduction:** on `speech_noisy`, SNR of the output vs the matched `speech_clean`
     reference (the fixtures were sourced as a matched pair for exactly this).
   - **Dereverb:** on the synthetic-reverb variant, late-tail energy reduction.
   - **Clarity:** consonant/HF presence preserved-or-raised on clean speech.
   - **Low coloration:** the speech core band (~300–3000 Hz) of clean speech stays near the dry
     shape — penalize muddying (keeps the chain "transparent" where it should be).
4. **Score** = weighted sum of the target terms, gated by the hard constraints (weights = D6).

**Search.** Per order, over a bounded space of the few perceptually-relevant params per enabled
effect: seeded **coordinate descent** (cheap, deterministic, reproducible), optionally seeded random
restarts. Keep the argmax patch.

**Integration.**
- A heavy offline task `make tune-defaults` (NN; excluded from `make ci`) runs the search per order
  and writes the winning patches into the built-in defaults. Deterministic (fixed seeds) so results
  are reproducible and reviewable in diffs.
- A **committed-defaults regression test** (in `make test-models`) asserts each built-in default
  patch still scores above a floor on the battery — so a future effect change that degrades a
  default trips CI-models and prompts a re-tune, rather than silently shipping worse defaults.

## Open risks
- **Signal-injection refactor (M1):** touches 4 effect crates + their tests and removes the
  `sync-analysis` feature. Bounded, but it changes their public shape (signals injected, not
  self-derived).
- **Dynamic latency:** order/bypass changes alter total latency; some hosts handle dynamic latency
  poorly → D5 leans fixed-max (always run latency-inducing slots delay-compensated).
- **Tuning metrics are proxies:** loudness + matched-pair SNR + coloration are solid; a true
  intelligibility metric (STOI-like) is a stretch goal, not a gate.
- **Order-as-parameter state:** the order value lives in the patch and reloads the per-effect
  defaults when changed — program-change-like behavior to map onto VST3 parameter/state semantics.
- **Platform:** VST3 builds are macOS-only (ADR-0007); the Windows host is a separate project.

## Decision register & handoff

- **Resolved:** D1 name = **Calóma** (`plugins/caloma`); D3 = **ConvolutionReverb dropped**;
  packaging shape recorded in [ADR-0020](docs/adr/0020-caloma-speech-vst-packaging.md).
- **Open `[DECISION]` gates** (confirmed at their milestone): D2 the 3 signal orders (M3); D4
  analysis-tap point; D5 latency strategy; D6 tuning targets/weights (M5); D7 editor scope (M4).
- **Durable docs created:** ADR-0020 (+ index), `plugins/caloma/README.md` (reserved, not yet a
  member — build untouched), AGENTS product-names row, backlog entries (Calóma + host/visualizer/
  coach), this plan.
- **Handoff:** this plan is the single source of truth. To execute, run `plan-phase` on **M0** to
  expand it into red→green steps, then the companion execution prompt. Expand one milestone at a
  time, just before running it.
