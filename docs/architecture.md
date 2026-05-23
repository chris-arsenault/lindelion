# Architecture

Lindelion is a Rust workspace for related audio instruments and shared plugin infrastructure. The workspace keeps reusable host, state, UI, and DSP layers in `crates/`, while product-specific code lives under `plugins/`.

## Workspace Packages

| Path | Role |
| ---- | ---- |
| `crates/lindelion-plugin-shell` | Shared plugin boundary: descriptors, parameters, process context, MIDI/control events, state, typed VST3 messages, VST3 factory helpers, TOML patch I/O, and voice allocation. |
| `crates/lindelion-dsp-utils` | DSP support code: analysis, delay/interpolation, envelopes, filters, math, smoothing, saturation, and parameter smoothing policies. |
| `crates/lindelion-sample-library` | Sample references, hashing, file-library ingest, preview generation, and moved-file recovery by content hash. |
| `crates/lindelion-ui` | Shared UI command model, editor services, and product Vizia editor surfaces. |
| `crates/lindelion-onset-detect` | Onset detection interfaces and detectors used by Linnod and Glirdir. |
| `crates/lindelion-pitch-detect` | SwiftF0 ONNX pitch detection, confidence filtering, resampling, and pitch-contour DTOs shared by pitch-aware products. |
| `crates/lindelion-midi` | Root/scale models, timing and pitch quantization, velocity mapping, MIDI clip DTOs, and Standard MIDI File emission. |
| `crates/lindelion-psola` | Pitch-analysis and PSOLA boundary types for future melodic sample manipulation. |
| `plugins/lamath` | Implemented breath-excited resonator VST3 instrument. |
| `plugins/linnod` | Melodic sample-slicer scaffold with descriptor, parameters, patch model, and silent plugin implementation. |
| `plugins/glirdir` | Sing-to-MIDI scratchpad plugin: capture state, pitch/onset analysis, segmentation, quantized MIDI derivation, audition, VST3 adapter, editor, drag/export, sample-library save, and bundle metadata. |
| `xtask` | Repository automation for checks and macOS VST3 bundle construction. |

## Shared Runtime Boundaries

- `AudioPlugin` in `lindelion-plugin-shell` is the host-facing contract used by plugin crates.
- `ProcessContext` carries shared output buffers, optional audio input buffers, MIDI events, and host transport data so instruments and input-driven effects use the same shell boundary.
- Parameter metadata is registry-driven: stable IDs, normalized/plain conversion, formatting, editor grouping, and runtime smoothing are declared together.
- `TomlPatchFormat<T>` owns versioned TOML patch envelopes, typed decode errors, migrations, atomic file writes, and `PluginState` roundtrips.
- `MidiEventNormalizer` converts host MIDI into internal `MidiEvent` values with plugin-provided controller routes and pitch-bend range.
- `VoiceManager` owns allocation, stealing, retrigger reuse, active/released/idle transitions, and per-channel/per-note expression routing.
- Pitch, onset, and MIDI derivation live in shared crates. Product plugins compose those crates and own only product-specific capture, segmentation policy, UI, and host integration.
- `lindelion-ui` owns reusable editor commands, editor services, and product editor surfaces while the workspace remains small.

## Durable Architecture Principles

These principles came out of the Lamath architecture remediation work and apply to new plugin work unless a later design explicitly replaces them.

### Single Source Of Truth

- Parameter IDs must be interpreted by one typed registry. The registry owns host metadata, normalized/plain conversion, formatting, patch get/set, apply policy, runtime target, smoothing metadata, and editor-surface metadata.
- Enum-valued parameters must use typed codecs instead of paired free functions for each direction.
- Editor controls must be derived from parameter metadata. A visible control should not maintain a separate hand-written parameter ID list that can drift from host automation.
- DSP constants and formulas with semantic meaning must live behind named constants or small parameter structs. Tuning a behavior should have one definition site and tests that pin intentional numerical behavior.

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

### State And Realtime Behavior

- Patch serialization uses `TomlPatchFormat<T>` for version envelopes, typed errors, migration hooks, atomic writes, and `PluginState` roundtrips.
- Plugin-specific patch I/O should be a thin adapter around the shared format, keeping only product-specific migrations and file naming.
- Live smoothed parameters should use `SmoothedAtomicParam` or equivalent shared smoothing bridges fed by the parameter registry.
- Structural changes must have explicit apply policies. If a change affects active audio, the policy must specify whether it is live, note-boundary, reset-state, live crossfade, or live mute-ramp behavior.
- Detection algorithms must be configured through shared pitch/onset DTOs. Pitch-aware products should pass pitch tracks into onset/segmentation code rather than making onset detectors own pitch inference.
- Audio-thread code must not allocate, block, perform file or database I/O, log, call host/UI services, or loop without hard bounds. See [performance.md](performance.md).

### Architectural Tests

- Every exposed host parameter should have exactly one binding, and every binding should roundtrip patch get/set where applicable.
- Editor surfaces should prove that every visible control resolves to a binding and every required group has visible controls.
- Typed plugin messages should roundtrip payloads, ignore unknown message IDs, and reject malformed payloads without panics.
- MIDI normalization should use shared host-neutral fixtures for notes, CC routes, channel pressure, poly pressure, and pitch bend.
- Voice management tests should cover deterministic stealing, retrigger behavior, released/active/idle transitions, and per-channel/per-note expression routing.
- Patch I/O tests should cover valid roundtrips, malformed TOML, forward versions, migrations, and atomic writes.
- DSP constant tests should pin numerical behavior for non-obvious formulas such as boundary models and conditioning filters.

## VST3 Product Boundaries

Lamath and Glirdir are the current bundleable VST3 products. Their plugin crates keep host ABI code under `plugins/*/src/vst3_entry/` and keep audio/runtime code outside that boundary:

| Module | Role |
| ---- | ---- |
| `plugin.rs` | Product `AudioPlugin` implementation and patch/state composition. |
| `patch.rs` / `patch_io.rs` | Serializable patch model and shared TOML/state adapters. |
| `parameters.rs` | Parameter registry, patch binding, apply policy, formatting, and editor-surface metadata. |
| `vst3_entry/` | Processor/controller/factory/editor/state/message adapters for VST3 hosts. |
| Lamath `runtime.rs` / `dsp/` | Resonator runtime patch conversion, excitation playback, resonators, voice rendering, modulation state, and output stage. |
| Glirdir `capture.rs` / `analysis.rs` / `audition.rs` / `worker.rs` | Scratchpad capture, pitch/onset analysis, MIDI derivation, audition rendering, and off-audio-thread jobs. |
| Glirdir `midi_export.rs` / `sample_library.rs` | SMF drag/export payloads and shared sample-library scratchpad ingest. |

## Real-Time Rule

No heap allocation, file I/O, blocking synchronization, logging, host/UI calls, or unbounded user-data loops are allowed on the audio thread. See [performance.md](performance.md) for the enforceable contract and current coverage.
