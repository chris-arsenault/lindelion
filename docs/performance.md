# Audio Performance Contract

This workspace treats the real-time audio path as allocation-free.

In Rust, that rule is meaningful: ordinary stack values, slices, fixed-size arrays, enum matches, and iterator adapters do not inherently allocate, but `Vec` growth, `String`, `Box`, `Arc`, `format!`, most logging, dynamic sample loading, and many convenience conversions do. Safe Rust prevents memory unsafety; it does not automatically make code real-time safe.

## Real-Time Path Rules

- No heap allocation on `note_on`, voice stealing, per-sample processing, or block rendering.
- No file I/O, database I/O, sample decoding, hashing, or patch serialization on the audio thread.
- No locks, channels that can block, sleeping, logging, or host/UI calls from the audio thread.
- No unbounded loops based on user data in per-sample code. Bound work with hard caps such as the 256-mode modal limit.
- No panics in audio-path code. Validate and clamp parameters before or at the boundary.
- Audio output must remain finite: no NaN or infinity.
- DSP tails must be bounded under parameter sweeps.
- Render behavior should be deterministic for the same patch, sample, events, and sample rate.

## Current Violation Tests

- `resonator_synth::dsp::engine::tests::note_on_and_render_do_not_allocate`
  counts allocations during `note_on`, block render, and voice stealing.
- DSP tests in `ahara-dsp-utils` and `resonator-synth` assert finite output, bounded peaks, frequency behavior, filter behavior, and sweep stability.
- `cargo run -p xtask -- check` runs rustfmt, workspace/all-targets clippy with `-D warnings -W clippy::cognitive_complexity`, and workspace tests.

## Allowed Allocation Zones

- Plugin construction and process setup.
- Patch load/save.
- Sample ingest/decode/analysis.
- UI operations.
- Offline preparation before a patch becomes active.
