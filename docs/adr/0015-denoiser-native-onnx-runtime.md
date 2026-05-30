# 0015 — Speech NN effects (denoiser, voice gate) run on the native ONNX Runtime (`ort`)

- Status: Accepted
- Date: 2026-05-30

## Context

SwiftF0 pitch detection runs on `tract`, a pure-Rust ONNX runtime, which keeps that crate
dependency-light and cross-compilable. The two Tier-3 speech effects could not follow that path —
`tract` cannot run their models:

- **DFN3 denoiser.** The realtime-safe streaming graph — the `hot-mic` export, a single
  self-contained graph with the recurrent state exposed as a tensor — was exported through
  Microsoft ONNX Runtime with `SplitToSequence` and ORT-fused ops. `tract` rejects it at optimize
  time (`Unimplemented(SplitToSequence)`).
- **Silero VAD voice gate.** `tract` fails to optimize the v5 model (`Failed analyse for node
  "If_0" … "/decoder/Squeeze"`): an `If`/`Squeeze` in the decoder is not analyzable, independent of
  the `sr` input. `model_for_path` parses it, but `into_optimized()` fails, so it cannot run.

Two more paths were ruled out for DFN3 specifically:

- The upstream `deep_filter` crate (libDF) pins `tract ~0.21`, does not build against the
  workspace's `tract 0.23`, and cannot be used as a dependency.
- Porting libDF's `tract.rs` + `transforms.rs` to `tract 0.23` is a large translation against an
  unstable dev API: it relies on `tract-pulse` `PulsedModel` to carry the GRU recurrent state
  across hops, and the surrounding API (`shapefactoid!`, `with_output_names`, the pulse builder)
  has drifted substantially from libDF's 0.21. It is a multi-file port with real numerical-fidelity
  risk.

Both effects are required product behavior, not optional, so dropping them was not acceptable.

## Decision

Run both the DFN3 denoiser and the Silero voice gate on the **native ONNX Runtime via the `ort`
crate**. These are the only crates in the workspace with a native (non-pure-Rust) dependency.
SwiftF0 stays on `tract` (it optimizes and runs there).

The `ort` `download-binaries` feature fetches a prebuilt ONNX Runtime for the target at build time.
Each model's `Session` is built once at `prepare`; the denoiser runs once per 10 ms hop, the voice
gate once per 16 kHz VAD chunk, both inline on the audio thread per
[ADR-0014](0014-nn-inference-allocation.md).

## Alternatives considered

- **Pure-Rust `tract` for DFN3.** The realtime-safe streaming export will not load. Rejected.
- **The `deep_filter` (libDF) crate.** Pins `tract 0.21`; does not build; conflicts with the
  workspace `tract 0.23`. Rejected.
- **Port libDF to `tract 0.23`.** Feasible but large and fragile — `tract-pulse` plus heavy API
  drift across ~2000 lines, against a pre-release API, with numerical-fidelity risk. Deferred; the
  native runtime delivers the proven model immediately.
- **Drop DFN3.** It is the strongest denoiser available and a product requirement. Rejected.

## Consequences

- The `speech-denoiser` and `voice-gate` crates are not pure-Rust and not fully offline-buildable:
  a clean build needs network for `ort` to download the ONNX Runtime binary (or a system runtime
  via `load-dynamic`). This is isolated to those two crates; the rest of the workspace stays
  pure-Rust.
- The bundled DFN3 model is the proven streaming `hot-mic` export, so DFN3 is realtime-safe without
  a bespoke pipeline port. The voice gate runs Silero v5 at 16 kHz (the host's 48 kHz is decimated
  3:1) with zero reported latency.
- End-to-end latency is 1920 samples (four 480-sample hops: one host-side hop buffer plus three
  hops of model-internal latency), validated by best-lag alignment of clean speech.
- If the workspace later needs a fully pure-Rust, offline build, the libDF-to-`tract` port in the
  alternatives is the fallback to revisit.
