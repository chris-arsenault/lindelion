# Architecture

Lindelion is a Rust workspace for related audio instruments and shared plugin infrastructure. The workspace keeps reusable host, state, UI, and DSP layers in `crates/`, while product-specific code lives under `plugins/`.

## Workspace Packages

| Path | Role |
| ---- | ---- |
| `crates/lindelion-plugin-shell` | Shared plugin boundary: descriptors, parameter registry/codecs/apply dispatch, process context, MIDI/control events, state, typed VST3 messages, VST3 factory helpers, TOML patch I/O, and voice allocation. |
| `crates/lindelion-dsp-utils` | DSP support code: analysis helpers, delay/interpolation, envelopes, filters, math, smoothing, saturation, and parameter smoothing policies. |
| `crates/lindelion-test-allocator` | Shared counting allocator and no-allocation assertion helper for realtime-path tests. |
| `crates/lindelion-capture` | Host-synced audio capture state, scratchpad audio/metadata, capture settings, sync modes, and capture timing constants. |
| `crates/lindelion-sample-library` | Sample references, loaded-audio ownership, hashing, file-library ingest, preview generation, and moved-file recovery by content hash. |
| `crates/lindelion-audio-expression` | Audio-analysis-to-expression bridge that maps pitch, onset, loudness, and brightness into plugin-shell expression streams. |
| `crates/lindelion-onset-detect` | Batch and streaming onset detection interfaces, detector configuration, and pitch-aware onset input DTOs used by pitch-aware products. |
| `crates/lindelion-pitch-detect` | SwiftF0 ONNX pitch detection, streaming pitch tracking, confidence filtering, resampling, and shared pitch-contour DTOs. |
| `crates/lindelion-phrase-analysis` | Pitch/onset phrase orchestration, note segmentation, segmentation heuristics, and phrase-analysis results shared by captured-phrase workflows. |
| `crates/lindelion-midi` | Root/scale models, timing and pitch quantization, velocity mapping, MIDI clip DTOs, and Standard MIDI File emission. |
| `crates/lindelion-psola` | Pitch-analysis and PSOLA boundary types for future melodic sample manipulation. |
| `crates/lindelion-ui` | Shared UI command model, editor services, editor surface primitives, and product Vizia editor surfaces. |
| `plugins/lamath` | Breath-excited resonator VST3 instrument. |
| `plugins/linnod` | Melodic sample-slicer scaffold with descriptor, parameters, patch model, and silent plugin implementation. |
| `plugins/glirdir` | Sing-to-MIDI scratchpad plugin: shared capture composition, phrase analysis, quantized MIDI derivation, audition, VST3 adapter, editor, drag/export, sample-library save, and bundle metadata. |
| `xtask` | Repository automation for checks and macOS VST3 bundle construction. |

## Shared Runtime Boundaries

- `AudioPlugin` in `lindelion-plugin-shell` is the host-facing contract used by plugin crates.
- `ProcessContext` carries shared output buffers, optional audio input buffers, MIDI events, and host transport data so instruments and input-driven effects use the same shell boundary.
- Parameter metadata is registry-driven: stable IDs, normalized/plain conversion, formatting, editor grouping, and runtime smoothing are declared together.
- `TomlPatchFormat<T>` owns versioned TOML patch envelopes, typed decode errors, migrations, atomic file writes, and `PluginState` roundtrips.
- `MidiEventNormalizer` converts host MIDI into internal `MidiEvent` values with plugin-provided controller routes and pitch-bend range.
- `VoiceManager` owns allocation, stealing, retrigger reuse, active/released/idle transitions, and per-channel/per-note expression routing.
- Pitch, onset, phrase analysis, audio expression, capture, and MIDI derivation live in shared crates. Product plugins compose those crates and own product-specific policy, UI, message payloads, and host integration.
- Shared capture and scratchpad audio live in `lindelion-capture`; product plugins own parameter stepping, naming, MIDI context projection, and other product semantics layered on top.
- `lindelion-ui` owns reusable editor commands, editor services, and product editor surfaces while the workspace remains small.

## Durable Architecture Principles

These principles came out of the Glirdir/Lamath reuse remediation work and apply to new plugin work unless a later design explicitly replaces them.

### Shared Core Boundaries

- When two real consumers need the same behavior, extract the stable core immediately unless there is a concrete technical blocker. Do not defer the extraction only because a later plan item names the same area.
- Shared crates should contain host-neutral behavior, typed DTOs, validation, reusable state machines, and reusable dispatch/projection logic.
- Product crates should keep product policy local: patch paths, parameter stepping, runtime targets, apply-policy enums, message payloads, UI slots, file naming, MIDI context projection, and product DSP behavior.
- Single-consumer code that only looks generic may stay local. Document it as an optional extraction and move it only when a second consumer makes the boundary concrete.
- Removing duplication means deleting or rewiring the old local pattern, not only wrapping it. If a local variant remains, it needs a product-specific reason.

### Single Source Of Truth

- Parameter IDs must be interpreted by one typed registry. The registry owns host metadata, normalized/plain conversion, formatting, patch get/set, apply policy, runtime target, smoothing metadata, and editor-surface metadata.
- Enum-valued parameters must use typed codecs instead of paired free functions for each direction.
- Editor controls must be derived from parameter metadata. A visible control should not maintain a separate hand-written parameter ID list that can drift from host automation.
- DSP constants and formulas with semantic meaning must live behind named constants or small parameter structs. Tuning a behavior should have one definition site and tests that pin intentional numerical behavior.
- Detection heuristics must live in shared configuration/profile structs when the detector or segmenter is shared. Hardcoded musical thresholds are acceptable only when they are truly product policy.

### Host Boundaries

- Product crates own sound and product behavior. Host protocol mechanics belong in shared shell crates whenever they are not product-specific.
- VST3 factory registration, FFI string copying, entrypoint exports, fixed-size `IPlugView` base behavior, typed `IMessage` wrappers, and malformed-message handling belong in `lindelion-plugin-shell::vst3`.
- Plugin crates declare CIDs, class names, parameter sets, processor/controller construction, and product-specific message payloads on top of the shared VST3 layer.
- Host MIDI must be normalized through `MidiEventNormalizer`. VST3/AU/CLAP adapters should translate host fields into host-neutral events, then delegate CC routing, pitch-bend range, and expression mapping to shared code.
- Audio input buffers and transport state belong in `ProcessContext`; product processors should not invent parallel host-context DTOs.

### Editor Boundary

- UI commands are typed `UiCommand` values. Primitive encodings such as float command codes are allowed only behind one adapter layer required by the UI/host bridge.
- Patch save/load/export, sample ingest, sample-slot assignment, slot clearing, and telemetry requests flow through reusable editor services. File-dialog selection may remain host/UI-specific, but action handling should be shared.
- Product VST3 editors should be thin host adapters: attach/detach lifecycle, controller callback projection, and DTO conversion. Vizia application code belongs in `lindelion-ui` or a future UI crate.
- `lindelion-ui` may contain product-specific surfaces while there are few products. After Linnod or Glirdir ships, revisit whether common widgets/services and product compositions should split into separate crates.

### Module Boundaries

- Crate roots should wire modules, descriptors, re-exports, and test hooks. Patch schema, parameters, runtime, plugin trait implementation, VST3 adapters, and tests belong in focused files.
- VST3 adapters should be organized by role: processor, controller, factory, messages, MIDI mapping, state, editor, and tests.
- Large DSP structs should be decomposed by responsibility. Lamath voices coordinate `ResonatorStack`, `ModulationState`, and `OutputStage`; future voices should follow the same pattern.
- Shared voice allocation and stealing policy belongs in `VoiceManager`. Product voices implement `VoiceLike` and own product-specific trigger/render behavior.
- Product DSP internals should not move into shared crates just because another product exists. Extract architecture, ownership, and routing primitives first; leave sound-generation behavior local until a second product needs the same algorithm.

### State And Realtime Behavior

- Patch serialization uses `TomlPatchFormat<T>` for version envelopes, typed errors, migration hooks, atomic writes, and `PluginState` roundtrips.
- Plugin-specific patch I/O should be a thin adapter around the shared format, keeping only product-specific migrations and file naming.
- Live smoothed parameters should use `SmoothedAtomicParam` or equivalent shared smoothing bridges fed by the parameter registry.
- Structural changes must have explicit apply policies. If a change affects active audio, the policy must specify whether it is live, note-boundary, reset-state, live crossfade, or live mute-ramp behavior.
- Detection algorithms must be configured through shared pitch/onset/phrase DTOs. Pitch-aware products pass `PitchFrame` tracks into onset and segmentation code instead of making onset detectors own pitch inference.
- `PitchFrame` is the canonical pitch-analysis frame. Do not introduce adapter frame types for subsets of the same data unless a dependency boundary makes the adapter unavoidable.
- Onset detectors should express required context in input types. A detector must not return sentinel empty results only because the trait did not provide required pitch context.
- Streaming pitch, onset, and loudness analysis are first-class surfaces. Batch analysis may buffer or wrap streaming implementations, but it should not create a second semantic model for the same detector.
- Audio-expression mapping from pitch, onset, loudness, and brightness belongs in `lindelion-audio-expression` and should be configured through mapping parameters rather than product-specific branches.
- Capture completion and scratchpad finalization must keep allocation-heavy ownership changes off the realtime path; shared capture state should make that boundary explicit.
- Audio-thread code must not allocate, block, perform file or database I/O, log, call host/UI services, or loop without hard bounds. See [performance.md](performance.md).

### Architectural Tests

- Every exposed host parameter should have exactly one binding, and every binding should roundtrip patch get/set where applicable.
- Editor surfaces should prove that every visible control resolves to a binding and every required group has visible controls.
- Typed plugin messages should roundtrip payloads, ignore unknown message IDs, and reject malformed payloads without panics.
- MIDI normalization should use shared host-neutral fixtures for notes, CC routes, channel pressure, poly pressure, and pitch bend.
- Voice management tests should cover deterministic stealing, retrigger behavior, released/active/idle transitions, and per-channel/per-note expression routing.
- Patch I/O tests should cover valid roundtrips, malformed TOML, forward versions, migrations, and atomic writes.
- DSP constant tests should pin numerical behavior for non-obvious formulas such as boundary models and conditioning filters.
- Realtime shared helpers should use `lindelion-test-allocator` coverage in each test binary that exercises an audio-thread path.
- Analysis tests should cover both batch and streaming entry points where both exist, with shared fixtures for pitch frames, onset markers, and segmentation behavior.

## VST3 Product Boundaries

Lamath and Glirdir are the current bundleable VST3 products. Their plugin crates keep host ABI code under `plugins/*/src/vst3_entry/` and keep audio/runtime code outside that boundary:

| Module | Role |
| ---- | ---- |
| `plugin.rs` | Product `AudioPlugin` implementation and patch/state composition. |
| `patch.rs` / `patch_io.rs` | Serializable patch model, product-specific patch migrations, and shared TOML/state adapters. |
| `parameters.rs` | Parameter registry, patch binding, apply policy, formatting, and editor-surface metadata. |
| `vst3_entry/` | Processor/controller/factory/editor/state/message adapters for VST3 hosts. |
| Lamath `runtime.rs` / `dsp/` | Resonator runtime patch conversion, excitation playback, resonators, voice rendering, modulation state, and output stage. |
| Glirdir `analysis.rs` / `analysis_job.rs` / `worker.rs` | Product orchestration around shared phrase analysis, cached MIDI derivation, and off-audio-thread jobs. |
| Glirdir `patch.rs` | Product patch state plus Glirdir-specific scratchpad MIDI context layered on shared scratchpad audio. |
| Glirdir `audition.rs` | Local MIDI audition engine; optional shared extraction only when a second consumer exists. |
| Glirdir `midi_export.rs` / `sample_library.rs` | SMF drag/export payloads and shared sample-library scratchpad ingest. |

## Real-Time Rule

No heap allocation, file I/O, blocking synchronization, logging, host/UI calls, or unbounded user-data loops are allowed on the audio thread. See [performance.md](performance.md) for the enforceable contract and current coverage.
