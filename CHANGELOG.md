# Changelog

All notable user-visible changes to Lindelion are recorded here.

## Unreleased

### Lamath

- Implemented the Lamath v2 sidechain workflow: optional audio input bus, audio-created notes from sidechain onsets, continuous and note-latched live excitation modes, and per-patch audio/MIDI interaction policy.
- Added typed parameter bindings for audio input mode, audio expression mapping, note detection thresholds, live excitation mode, latch window, and latch fade.
- Added preallocated sidechain scratch, pre-roll, and per-voice latch buffers so the v2 audio path remains allocation-free on the audio thread.

### Glirdir

- Completed Glirdir's VST3 buildout: sing-to-MIDI scratchpad with capture-first analysis worker, editor surface, drag/export with fallback paths, sample-library scratchpad save, patch and DAW state persistence, and macOS bundle support.

### Shared infrastructure

- Extracted shared Glirdir/Lamath surfaces into reusable crates: `lindelion-capture`, `lindelion-audio-expression`, `lindelion-phrase-analysis`, and `lindelion-midi`.
- Added host-agnostic Criterion benchmarks for `lindelion-dsp-utils`, Lamath modal/waveguide/engine paths, and Glirdir's offline analysis job.
- Added `make bench` and `make bench-smoke` targets and per-crate perf records under `docs/perf/`.

### Code organization

- Split oversized Rust modules so every file fits the 600-line size cap enforced by `xtask lint-sizes`.

### Bug fixes

- Fixed the resonator parameter architecture so registry bindings drive both host automation and the editor surface.
- Fixed resonator architecture debt by aligning module boundaries with the workspace's shared-core principles.

### Documentation

- Reorganized plugin docs and backlog tracking under `docs/plugins/` and added per-crate perf records under `docs/perf/`. Added the repository documentation convention (`AGENTS.md`, ADRs, CHANGELOG, workspace backlog) following `../ahara/REPO-DOCS.md`.

## v0.3.0 - 2026-05-23

### Plugin shell

- Implemented the resonator VST3 plugin shell for Lamath, including processor, controller, factory, editor, state, and message adapters.
- Established the parameter registry as the single source of truth for parameter metadata, normalized/plain conversion, formatting, patch get/set, apply policy, runtime target, smoothing metadata, and editor binding.
