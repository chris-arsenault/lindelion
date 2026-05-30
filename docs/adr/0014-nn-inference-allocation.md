# 0014 — Neural-network inference runs inline, not on an audio-through worker

- Status: Accepted (refines [ADR-0001](0001-allocation-free-audio-thread.md))
- Date: 2026-05-30

## Context

The Tier-3 speech effects (DeepFilterNet 3 denoiser, Silero VAD voice gate) run neural-network
inference (the denoiser on the native ONNX Runtime per [ADR-0015](0015-denoiser-native-onnx-runtime.md),
the others on `tract`). NN inference allocates, which appears to conflict with ADR-0001's
allocation-free audio-thread rule. The first M5 plan resolved this with an off-thread
audio-through worker (audio rings to a worker, cleaned audio rings back).

That design was wrong. ADR-0001's no-allocation rule is a **proxy** for "bounded, contention-free,
deadline-safe callback" — heap allocation is banned because `malloc`/`free` can take a global
lock, hit the OS, or cause priority inversion, any of which can blow the callback deadline and
produce a dropout. An audio-through worker does not remove the allocation; it moves it to another
thread and puts the cleaned audio behind a cross-thread handoff that is still in the hot path,
adding latency, scheduling jitter, and a dropout failure mode — the exact outcome ADR-0001 exists
to prevent. It satisfies the letter of the rule while making the real outcome worse.

## Decision

Ordinary DSP keeps the literal allocation-free bar (every non-NN effect in the workspace holds
it). **Neural-network inference effects run inline on the audio thread.** Their bar is ADR-0001's
*intent* — bounded, contention-free, deadline-safe — met by building the runnable model at
`prepare` and keeping per-inference allocation bounded, so steady-state inference does not hit the
system allocator beyond a bounded residual. If measurement shows that residual matters, the fix is
a preallocated arena allocator inline, not a worker.

An off-thread worker is justified only by **throughput** — a model that cannot finish within the
callback deadline. DFN3 (low-latency streaming, RTF < 1) and Silero VAD (tiny) meet the deadline,
so neither uses one.

## Alternatives considered

- **Audio-through worker.** Moves allocation off-thread but keeps the cleaned audio in the hot
  path behind a jittery handoff with a dropout fallback, while still allocating. Worse than
  bounded inline work on every axis that ADR-0001 cares about. Rejected.
- **Bespoke zero-allocation inference.** Reimplement the model forward pass with fully
  preallocated buffers for literal zero-alloc inline. Large, model-specific, fragile on model
  updates. Deferred; revisit only if measured residual allocation proves problematic.
- **Relax ADR-0001 globally.** Unnecessary and harmful — ordinary DSP can and does hit literal
  zero-alloc. The carve-out is scoped to NN inference only.

## Consequences

- DFN3 and Silero run inline with preallocated tract state; latency is the model's algorithmic
  frame latency, with no thread, no scheduling jitter, and no dropout fallback path.
- Inline inference is synchronous and deterministic, so NN effect tests (SNR improvement, VAD
  gate accuracy) are deterministic rather than worker-timing-dependent.
- The shared fidelity battery applies its strict `assert_allocation_free` only to non-NN effects;
  NN effects instead assert bounded steady-state allocation (`assert_bounded_allocation`).
- The battery's impulse-based latency check is likewise scoped out for NN effects
  (`BatteryOptions { check_latency: false }`): a model's warm-up transient produces output before
  its declared latency, which the linear-delay heuristic cannot model. NN-effect latency is
  validated instead by best-lag signal alignment in the effect's own tests.
- A per-inference compute bench is recorded; if a model's per-hop burst cannot fit realistic
  callbacks at small buffer sizes, burst-smoothing is reconsidered then as a throughput decision,
  with data.
