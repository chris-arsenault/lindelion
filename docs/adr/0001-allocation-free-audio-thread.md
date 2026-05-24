# 0001 — Allocation-free audio thread

- Status: Accepted
- Date: 2026-05-23

## Context

Realtime audio plugins produce samples within strict deadlines determined by the host's block size and sample rate. Heap allocation, locks, file I/O, logging, and unbounded loops on the audio thread cause glitches, dropouts, and missed deadlines. Safe Rust prevents memory unsafety; it does not automatically make code realtime-safe.

## Decision

All audio-thread paths in Lindelion plugins are allocation-free, lock-free, and bounded. The contract covers `note_on`, voice stealing, per-sample processing, block rendering, and any sidechain-mode path. Compliance is enforced by `lindelion-test-allocator::assert_no_allocations!` in tests that wrap every audio-thread entry point.

## Alternatives considered

- **Soft constraint with manual review.** Drift is inevitable once the workspace has more than one product. Rejected.
- **A real-time allocator like rtsan.** Adds an unstable runtime dependency and would still require test coverage. The counting allocator is simpler and integrates with `cargo test`. Rejected.
- **`#![no_alloc]` lint at compile time.** Too coarse — patch I/O and UI paths legitimately allocate. Rejected.

## Consequences

- Audio-thread buffers, voice state, latch buffers, pre-roll rings, and per-mode arrays are preallocated during `prepare()` or structural patch application.
- Hard caps on per-sample iteration are stated explicitly (e.g., 256-mode modal limit).
- Allowed allocation zones are plugin construction, patch load/save, sample ingest/decode/analysis, UI operations, and offline preparation. These are listed in `docs/performance.md`.
- Audio output is validated as finite by `lindelion-dsp-utils::analysis::assert_all_finite` in tests across parameter sweeps.
- Filter state is flushed against denormals using `lindelion-dsp-utils::math::snap_to_zero`.
