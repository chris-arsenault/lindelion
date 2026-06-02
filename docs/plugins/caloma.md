# Calóma - Current Implementation Spec

**Name:** Calóma
**Name etymology:** Quenya `cala` ("bright/clear") + `óma` ("voice") — "clear voice".
**Target:** **Windows-only** VST3 audio **effect** (mic → clarified speech), for the Galad host
([ADR-0022](../adr/0022-windows-vst3-host.md)) or a Windows DAW. Per [ADR-0023](../adr/0023-new-vsts-windows-only.md).
**Status:** Built through M0–M6. Self-contained single-component VST3 with a Vizia editor and
per-order committed defaults. Linux `make ci` and the heavy `make test-models` fidelity gates pass;
`make build-windows` cross-compiles and stages `Caloma.vst3` (including the editor) via cargo-xwin.
On-target Windows load-and-run is the one item not yet verified.

This document describes the behavior implemented in the workspace today. The packaging decision is
[ADR-0020](../adr/0020-caloma-speech-vst-packaging.md) and the Windows-only / Vizia-editor decision is
[ADR-0023](../adr/0023-new-vsts-windows-only.md); remaining work lives in
[caloma-backlog.md](caloma-backlog.md).

---

## 1. Concept and goals

Calóma packages the 20 ported speech effects (`speech/`) as **one** VST3 — an opinionated serial
clarity chain for spoken word (meetings, oration, narration), not a musical effect. The product goal
is intelligibility: clean up noise/room, then shape and control the voice, with a *correct* signal
order baked in rather than left to the host.

Design principles:

- **One opinionated chain, three orders.** A 3-valued **signal-order** parameter selects one of
  three curated topologies (the slot sequence + which slots are enabled). Orders are *starting
  points*: selecting one loads that order's committed default tuning ([ADR-0020](../adr/0020-caloma-speech-vst-packaging.md)).
- **Self-contained and host-agnostic.** Calóma surfaces **no host-automatable parameters** and uses
  no host bridges. Its **Vizia editor is the sole control surface**; settings persist in the plugin
  state. (See §4.)
- **Compute-once analysis.** One shared analysis worker derives the `SignalSnapshot` per block; the
  chain injects it into the slots that consume it — no per-effect workers, no cross-effect bus.
- **Bounded realtime path.** The audio thread does not allocate or block; order switching and live
  control changes are lock-free (ADR-0001).
- **Speech-only tuning.** Defaults, thresholds, and band centers are tuned for spoken-word clarity,
  kept out of the shared `crates/` foundations ([ADR-0012](../adr/0012-speech-effect-port-shared-workspace.md)).

ConvolutionReverb is **not** part of the product (it adds reverberation, against the clarity goal;
Dereverberation is the speech tool) — [ADR-0020](../adr/0020-caloma-speech-vst-packaging.md).

## 2. Signal path

```
stereo in → downmix to mono
          → [input level (unity by default)]
          → analysis tap once/block → SignalSnapshot  ─┐  (injected into the consuming slots)
          → ordered chain of slots for the selected order:  ▼
              effect → effect → … → output limiter
          → [output level (unity by default)]
          → write mono to both output channels
latency = Σ all slots' latency (fixed-max), reported to the host
```

- The chain runs on a **mono downmix**; the result is written to both output channels.
- Each slot is enabled per the patch and blended by its **intensity** (a latency-compensated
  dry/wet, 1.0 = fully wet); a disabled slot is replaced by a delay equal to its latency so the
  chain stays time-aligned regardless of bypass (fixed-max latency, D5).
- Gain staging (input/output level) is applied at the head/tail; both default to unity (see §5).

### The three signal orders

| Order | Intent | Shape |
| ---- | ---- | ---- |
| **Clarity** | Default, balanced cleanup + clarity | repair (HPF → denoise → dereverb → gate) → tonal (EQ, dynamic EQ) → dynamics (compressor) → enhance (vitalizer, bass, air, contrast, consonant, upward expander) → de-ess → limit |
| **Broadcast** | Enhancement before dynamics, so the compressor controls the enhanced signal | repair → tonal → enhance → de-ess → compressor → upward expander → limit |
| **Light** | Transparency-first, minimal | HPF → VAD gate → denoise → de-ess → EQ → compressor → limit; the enhancement slots are present but **disabled** by the default patch |

The topologies are defined in `plugins/caloma/src/topology.rs`; which slots a default patch enables
is in `plugins/caloma/src/patch.rs` (`default_off_slots`).

## 3. Compute-once shared analysis

One `AnalysisWorker` (wrapping `lindelion-speech-signals` `SignalAnalyzer`: SwiftF0 voicing,
spectral/onset flux, HNR) computes the `SignalSnapshot` once per block at the chain head
(post-preprocessing tap, D4). The four SwiftF0-consuming effects (bass-enhancer, consonant-transient,
dynamic-eq, upward-expander) take an **injected** `&SignalSnapshot` rather than each embedding a
worker — the M1 refactor that retired the per-effect workers and the test-only `sync-analysis`
feature ([ADR-0013](../adr/0013-host-agnostic-effect-core.md): the neutral effect trait stays
signals-free; injection is a speech-layer concern). The neural slots (DFN3 denoiser, Silero
voice-gate) run their own inline inference ([ADR-0018](../adr/0018-nn-inference-allocation.md)); they are
chain slots, not analysis consumers.

## 4. VST3 boundary and the Vizia control surface

- **Single-component VST3.** One COM object implements `IComponent + IAudioProcessor +
  IEditController + IProcessContextRequirements`; `getControllerClassId` returns its **own** CID and
  the factory registers exactly one class, so the host `queryInterface`s the controller on the same
  object. The editor and the DSP therefore share one object directly — no host-relayed parameter
  channel. (`plugins/caloma/src/vst3_entry/`.)
- **No host parameters.** `getParameterCount() == 0` — Calóma exposes nothing to host automation.
  This is deliberate (ADR-0023's self-contained design): the editor is the control surface, not the
  host.
- **State** is the full `CalomaPatch` — the order value plus, per slot, its enable flag, its dry/wet
  intensity, and its typed parameters, plus the input/output levels — round-tripped through
  `lindelion-plugin-shell`'s `TomlPatchFormat` ↔ `PluginState`. Loading a patch sets the order and
  the tuning together; the order lives *inside* the patch (ADR-0020).
- **Lock-free shared controls.** A `SharedControls` holds the live settings as atomics (order index,
  per-slot enable + intensity, input/output level). The Vizia editor (UI thread) writes; `process`
  (audio thread) reads each block — no locks (ADR-0001).
- **Realtime-safe order switching.** All three orders' chains are pre-built in `reset` (the heavy
  one-time NN model load is off the audio thread); an order change is an **atomic index flip** and
  `process` selects the pre-built chain — never an audio-thread rebuild.
- **Editor** (`crates/lindelion-ui/src/caloma_vizia`): a Vizia view — order selector, input/output
  level controls, and per-effect enable + intensity — that talks to the plugin through a
  `CalomaControlSurface` trait (defined in `lindelion-ui`, implemented on `SharedControls` in
  `caloma`, so there is no `lindelion-ui → caloma` cycle). The Windows `IPlugView`→`HWND` baseview
  attach is the one Windows-specific editor piece (ADR-0023); the view code is target-gated and is
  exercised by `make build-windows`, not Linux `make ci`.

## 5. Default tunings and the offline tuning harness

Each order ships a **committed default patch** (`plugins/caloma/src/defaults/{clarity,broadcast,light}.toml`,
loaded by `default_patch_for`). A default = the order's enabled slots at **tonal/dynamics parameters
chosen by an offline tuning harness**, committed at **unity gain staging**.

- **What is tuned:** the few perceptually-relevant tonal/dynamics params per enabled effect
  (compressor threshold/makeup, de-esser threshold, EQ shelves, enhancer amounts, limiter ceiling,
  …). **Gain staging is *not* tuned** — input/output level ship at unity and the limiter provides
  peak safety; loudness is the user's to set in the editor.
- **Why loudness is not a baked target:** chasing a fixed integrated-loudness target (e.g. −16 LUFS)
  on raw, pause-y speech proved brittle — only the most-processed order reached it, and forcing the
  others there required extreme limiting that *degraded* noise reduction and dereverb. A default is a
  robust starting point, not a master; the limiter caps peaks and the user dials level.
- **The harness** (`plugins/caloma/src/tuning/`, run via `make tune-defaults`, release-built):
  - Runs the **real** per-order chain on a small spoken-word battery — clean speech, the matched
    noisy↔clean pair, and a synthesized-reverb variant — driving a **synchronous** `SignalAnalyzer`
    per block so results are deterministic (not the realtime off-thread worker).
  - Measures, per candidate: BS.1770 K-weighted integrated loudness, matched-pair best-lag SNR
    (noisy and clean both run through the same chain, so the chain's intended processing cancels and
    only residual noise remains), late-tail energy (dereverb), HF presence (clarity), and core-band
    coloration.
  - Scores a weighted sum of those terms gated by **hard constraints** (finite, non-clipping at
    ≤ 0.95 ≈ −0.45 dBFS), and optimizes the tonal/dynamics params with **seeded coordinate descent**.
  - Writes the winning patch per order; deterministic (fixed seeds), so the committed
    `defaults/*.toml` are reproducible and diff-reviewable.

## 6. Gates (how the build is verified)

| Command | What it covers for Calóma |
| ---- | ---- |
| `make ci` | The fast, pure, no-NN suite: metric primitives, scoring, search, the `SharedControls` atomics, patch/state round-trips, the single-component COM scaffold + one-class factory + `IPlugView` view creation, the latency accessors, and the non-NN chain mechanics. Debug, milliseconds, allocation-free where it matters (ADR-0001). |
| `make test-models` | Heavy NN end-to-end (`#[ignore]`d): the chain smoke (`tests/chain_e2e.rs`) and the **per-order full-chain fidelity gate** (`tests/full_chain_fidelity.rs`) — at each order's committed default: finite, non-clipping, **noise not worsened** (matched pair), **dereverb reduces the late tail**, and **reported latency matches the measured group delay**. Deliberately robust thresholds; **no** loudness gate (user-controlled) and **no** clarity gate (HF presence is order-dependent — the minimal Light order legitimately reduces it). |
| `make tune-defaults` | Re-derives the committed per-order default patches from the tonal search (release; deterministic). |
| `make build-windows` | Cross-compiles the Windows `Caloma.vst3` (DLL + `moduleinfo.json`, including the Vizia editor) via cargo-xwin. |

## 7. Known limitations

- **Light reduces HF** on clean speech (its denoiser/de-esser), more than a strict
  "transparency-first" reading implies. It is a starting point the user adjusts; flagged for a
  possible future re-tune.
- **On-target Windows** load-and-run (the editor rendering in a real host / the Galad host) is not
  yet verified — the bundle cross-compiles and stages cleanly, but behavior on Windows is untested.
- Defaults are tuned on a small spoken-word battery; they are sensible starting points, not a
  per-material master.
