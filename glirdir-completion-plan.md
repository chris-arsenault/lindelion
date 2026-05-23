# Glirdir Completion Plan

This plan starts from the current repository state: Glirdir has a core Rust crate with capture state, pitch/onset analysis, MIDI derivation, patch state, and audition. It does not yet have a VST3 adapter, editor surface, drag-out implementation, worker scheduling, compressed scratchpad persistence, bundle automation, or DAW validation.

## Current Reuse Baseline

Glirdir must continue to reuse these existing surfaces rather than reimplementing them:

1. `lindelion-plugin-shell`
   - `AudioPlugin`, `ProcessContext`, `AudioInputBuffer`, `TransportContext`
   - `ParameterInfo`, `ParameterRange`, parameter normalization
   - `TomlPatchFormat`, `PluginState`
   - VST3 state stream helpers, event conversion, factory helpers, typed messages

2. `lindelion-dsp-utils`
   - math helpers such as MIDI/frequency conversion
   - analysis helpers such as RMS/finite checks
   - smoothing primitives for audition and any live audio parameters

3. `lindelion-pitch-detect`
   - SwiftF0 ONNX inference
   - pitch contour DTOs
   - confidence/range configuration

4. `lindelion-onset-detect`
   - SuperFlux detector
   - pitch-stability detector fed by supplied pitch tracks
   - hybrid onset marker dedupe/debounce

5. `lindelion-midi`
   - root/scale/snap/grid models
   - detected note to quantized note conversion
   - velocity mapping
   - SMF byte export

6. `lindelion-ui`
   - command bus and editor service patterns
   - reusable UI conventions from Lamath where applicable

## Non-Negotiable Architecture Rules

1. Audio-thread code must not allocate, block, perform file I/O, perform database I/O, run pitch inference, write MIDI files, or call UI/host services.

2. Capture in `AudioPlugin::process` may only copy bounded input into preallocated buffers, update simple state, clear/render output, and consume already-derived audition data.

3. Capture finalization, SwiftF0 inference, onset detection, segmentation, MIDI derivation, scratchpad encoding, temp-file writing, and sample-library ingest must run off the audio thread.

4. Host protocol mechanics belong in `lindelion-plugin-shell` when they are not product-specific.

5. Glirdir parameter IDs must be interpreted by one registry. Host metadata, patch mutation, apply policy, and UI binding must not drift into separate lists.

6. Pitch detection must remain in `lindelion-pitch-detect`; onset detection must not grow its own pitch inference path.

7. UI controls must be backed by parameter metadata or typed UI commands, not magic float command IDs scattered through the editor.

8. Build automation must be explicit about whether it builds one selected plugin or all bundleable plugins.

## Completion Sequence

### 1. Lock The Core Contracts

Goal: make the current non-UI Glirdir core a reliable substrate before adding host/UI complexity.

Work:
1. Review `plugins/glirdir/src/lib.rs` exports and reduce them to intentional public API.
2. Keep module boundaries stable:
   - `patch.rs`: serializable state schema only.
   - `parameters.rs`: host parameter registry and patch mutation only.
   - `capture.rs`: realtime-safe capture state only.
   - `analysis.rs`: pitch/onset/segmentation/MIDI derivation only.
   - `audition.rs`: audio rendering from already-derived `MidiClip` only.
   - `patch_io.rs`: `TomlPatchFormat` adapter only.
   - `plugin.rs`: `AudioPlugin` composition only.
3. Add contract tests for public behavior that VST3/UI will depend on:
   - capture state transitions
   - capture finalization only off audio path
   - analysis settings flow into `PitchDetectionConfig`
   - quantize-only changes do not rerun pitch detection
   - patch state roundtrip with scratchpad

Acceptance:
1. `cargo test -p glirdir` passes.
2. `cargo check --workspace` passes.
3. The only allocation in capture completion is behind an explicit non-audio-thread call.

### 2. Add A Realtime-Safe Analysis Job Boundary

Goal: introduce the worker-facing contract before VST3 and UI depend on it.

Work:
1. Add a Glirdir analysis job type that owns:
   - scratchpad audio snapshot
   - sample rate
   - analysis settings
   - quantize settings
   - job sequence/version
2. Add an analysis result cache type that owns:
   - pitch contour
   - onset markers
   - detected notes
   - quantized MIDI clip
   - status: idle/capturing/analyzing/ready/error
   - sequence/version
3. Ensure the audio thread can publish "capture completed" without running analysis.
4. Ensure stale worker results cannot overwrite newer captures/settings.
5. Add a test pitch detector implementation so analysis scheduling tests do not invoke SwiftF0.

Acceptance:
1. Capture completion creates a job snapshot off the audio thread.
2. Re-quantization from cached detected notes is possible without recomputing pitch/onset.
3. Stale job result test passes.
4. No audio-thread allocation tests regress.

### 3. Finish Shared VST3 Shell Helpers Needed By Glirdir

Goal: avoid one-off VST3 plumbing in Glirdir.

Work:
1. Confirm `audio_input_buffer_from_vst_process_data` supports mono and stereo input buses.
2. Confirm `transport_context_from_vst_process_context` exposes:
   - playing
   - recording
   - sample position
   - project quarter note
   - bar position
   - cycle range
   - tempo
   - time signature
3. Add any missing shared helper for stereo output buffer projection if Lamath's local helper is reusable.
4. Add tests for invalid/null VST3 input buffers and invalid transport contexts.
5. Keep product-specific CIDs, class names, and parameter sets in plugin crates.

Acceptance:
1. `cargo test -p lindelion-plugin-shell` covers input and transport projection.
2. Glirdir VST3 code does not duplicate generic VST3 input/transport parsing.

### 4. Implement Glirdir VST3 Processor

Goal: make Glirdir loadable as an audio-effect VST3 with audio input and stereo output.

Work:
1. Add `plugins/glirdir/src/vst3_entry/processor.rs`.
2. Processor responsibilities:
   - expose one audio input bus
   - expose one stereo audio output bus
   - expose event input only if needed later; v1 can omit MIDI input
   - apply parameter changes to `Glirdir`
   - project VST3 audio input into `AudioInputBuffer`
   - project VST3 transport into `TransportContext`
   - call `Glirdir::process`
   - never run analysis or file I/O in `process`
3. Implement host process context requirements for tempo/time signature/bar position.
4. Add processor tests for:
   - bus count/info
   - sample size handling
   - process clears or renders finite output
   - input buffer reaches capture path
   - parameter changes mutate patch

Acceptance:
1. `cargo test -p glirdir vst3_entry` passes.
2. Processor has no direct SwiftF0, filesystem, or sample-library calls.

### 5. Implement Glirdir VST3 Controller And Messages

Goal: let UI/controller own off-audio-thread actions and communicate safely with the processor.

Work:
1. Add `controller.rs`, `messages.rs`, and tests under `plugins/glirdir/src/vst3_entry/`.
2. Define typed messages using `lindelion-plugin-shell::vst3::TypedPluginMessage`.
3. Message types should cover:
   - arm capture
   - clear scratchpad
   - finalize completed capture / request analysis
   - analysis status response
   - patch update
   - MIDI export request/response
   - telemetry/status request/response
4. Controller should mirror patch state like Lamath does, but avoid copying Lamath-specific sample-slot behavior.
5. Unknown and malformed messages must fail safely.

Acceptance:
1. Typed message payloads roundtrip.
2. Unknown messages are ignored.
3. Malformed payloads do not panic.
4. Controller can request capture finalization without touching audio thread state unsafely.

### 6. Implement Worker Scheduling

Goal: run capture finalization and analysis off the audio thread with deterministic result ownership.

Work:
1. Choose the worker mechanism:
   - simple controller-owned worker thread for v1, or
   - shared shell worker utility if Lamath/Linnod will also need it.
2. Worker jobs:
   - finalize completed capture into `ScratchpadAudio`
   - run `GlirdirAnalyzer`
   - rederive MIDI on quantize-only settings changes
   - encode scratchpad for persistence when needed
   - write temp MIDI files for drag-out
3. Add cancellation/version checks:
   - clear scratchpad invalidates current jobs
   - new capture invalidates older analysis
   - analysis setting changes invalidate pitch/onset jobs
   - quantize setting changes invalidate only MIDI derivation
4. Publish status to UI:
   - Idle
   - Armed
   - CountIn
   - Capturing
   - CapturedPendingAnalysis
   - Analyzing
   - Ready
   - Error

Acceptance:
1. Capturing a buffer schedules one analysis job.
2. Changing quantize strength does not schedule SwiftF0.
3. Clearing during analysis prevents stale result publication.
4. Worker tests use fake analyzer where possible and one SwiftF0 integration test where valuable.

### 7. Complete Scratchpad Persistence

Goal: make DAW/project reload restore the scratchpad without unbounded TOML or project bloat.

Work:
1. Decide storage format for VST3 state:
   - FLAC is the design target, but choose a Rust encoder crate deliberately.
   - If FLAC encoder quality/API is poor, use a simpler bounded binary float encoding temporarily and document the tradeoff.
2. Keep `TomlPatchFormat` for patch settings.
3. Store large audio payload separately inside plugin state envelope if needed.
4. Add versioning so future compressed-state migrations are possible.
5. Add maximum state size guards.
6. Add tests:
   - empty scratchpad state roundtrip
   - non-empty scratchpad state roundtrip
   - corrupted payload fails cleanly
   - forward version fails cleanly
   - large payload bounded

Acceptance:
1. DAW state can restore patch settings and scratchpad audio.
2. Patch files remain human-readable for settings.
3. Large scratchpads are not serialized as huge TOML arrays in production VST3 state.

### 8. Build The Glirdir Editor Surface

Goal: provide the usable capture/analyze/audition/drag workflow.

Work:
1. Decide whether to keep product-specific Vizia surfaces in `lindelion-ui` for now or split common/product surfaces.
2. Extract or add reusable UI components:
   - waveform display DTO and renderer
   - piano-roll preview DTO and renderer
   - segmented controls for bars/sync/count-in/snap/grid
   - root/scale selectors
   - sliders for confidence/onset/min note/strength/velocity/audition volume
   - capture state indicator
   - analysis status indicator
3. Product editor layout:
   - top patch/status bar
   - transport/capture controls
   - waveform plus pitch confidence overlay
   - MIDI piano roll as drag source
   - quantize panel
   - detection panel
   - audition panel
4. UI commands:
   - arm
   - clear
   - play audition
   - stop audition
   - toggle loop
   - save scratchpad to library
   - drag/export MIDI
5. Ensure visible controls resolve to parameter bindings or typed commands.

Acceptance:
1. Every visible parameter control resolves to one `ParameterBinding`.
2. Capture/analysis status updates without UI polling hacks.
3. Text and controls fit at target plugin size.
4. No product UI reimplements shell message parsing.

### 9. Validate Drag-Out Before Polishing The Editor

Goal: retire the highest-risk workflow assumption before investing in polish.

Work:
1. Add a minimal drag source in the Glirdir editor.
2. On drag start, ask worker/controller for a temp MIDI file from current `MidiClip`.
3. Use `objc2`/Cocoa interop to start `NSDraggingSession` from the plugin view.
4. Validate drop into Ableton MIDI track.
5. If NSDragging from the plugin view fails, implement and test fallback:
   - copy MIDI file to clipboard, or
   - export-to-file button, or
   - auxiliary overlay view/window only if necessary.
6. Keep temp file cleanup bounded and safe.

Acceptance:
1. Ableton accepts dragged MIDI and creates a clip.
2. Empty clip drag behaves predictably.
3. Repeated drags do not leak unbounded temp files.
4. Fallback path is documented if drag is unreliable.

### 10. Finish MIDI Export Details

Goal: make the exported MIDI musically useful and predictable.

Work:
1. Capture host BPM and time signature at capture time.
2. Store tempo/time signature in analysis or scratchpad metadata.
3. Confirm `MidiClip` writes:
   - PPQ 960
   - tempo meta event
   - time signature meta event
   - monophonic note on/off pairs
   - minimum note duration
4. Implement clip naming:
   - default target: `glirdir-Cmin-4bar-120bpm.mid`
   - sanitize filename
5. Add tests for:
   - clip tempo
   - time signature
   - empty capture
   - short note extension
   - no overlapping notes

Acceptance:
1. Dragged clips line up to host tempo/grid.
2. Clip names are deterministic and filesystem-safe.

### 11. Harden Detection Quality

Goal: make pitch/onset/note segmentation trustworthy across real sung material.

Work:
1. Add fixture categories:
   - silence
   - breath/noise
   - clipped input
   - soft vowel onset
   - hard consonant onset
   - vibrato
   - scoop into pitch
   - legato pitch jump
   - repeated same-pitch articulation
   - low vocal register
   - high vocal/flute boundary near SwiftF0 cap
2. Add synthetic tests where exact expected results are useful.
3. Add recorded fixtures where musical realism matters.
4. Metrics to assert:
   - no NaN/Inf
   - expected note count range
   - median pitch within tolerance
   - onset within tolerance
   - no phantom notes on low-confidence gaps
   - runtime below target for 4/8/16 bars
5. Separate fast unit tests from slower fixture/performance tests if needed.

Acceptance:
1. Detection tests cover the failure modes that would cause post-drag editing.
2. SwiftF0 integration test remains real, not just model-bytes existence.
3. Performance numbers are recorded for Apple Silicon.

### 12. Complete Audition Behavior

Goal: make audition useful without pretending to be the user's final synth.

Work:
1. Keep sine synth simple unless user chooses a palette.
2. Implement controls:
   - play
   - stop
   - loop
   - volume
   - live edit
3. Host behavior:
   - if host transport is playing, optionally sync audition position to host/capture start
   - if host is stopped, use internal clock
4. Re-derivation behavior:
   - live edit can restart from current playhead or continue with updated MIDI
   - choose behavior before implementation
5. Tests:
   - finite output
   - no allocation in render
   - loop wrap
   - stop clears playhead
   - volume smoothing

Acceptance:
1. User can hear current derived MIDI without dragging.
2. Audition render remains realtime safe.

### 13. Add Sample Library Integration

Goal: save useful scratchpad audio into the shared library without duplicating library logic.

Work:
1. Use `lindelion-sample-library` for paths, hashing, ingest, and moved-file recovery.
2. Add UI command: Save Scratchpad To Library.
3. Include source metadata if the sample-library schema supports it:
   - plugin: Glirdir
   - capture bars
   - tempo
   - root/scale at save time
4. Handle empty scratchpad and ingest errors visibly.

Acceptance:
1. Save-to-library uses shared APIs.
2. Empty scratchpad cannot create invalid samples.
3. Saved audio is available to Lamath/Linnod library workflows.

### 14. Add Glirdir Bundle Support

Goal: make Glirdir buildable/stageable as a VST3 once its VST3 entry exists.

Work:
1. Add Glirdir VST3 entry exports.
2. Add Glirdir `BundleSpec` to `xtask`.
3. Add stable CIDs:
   - processor CID
   - controller CID
4. Add bundle metadata:
   - bundle name: `Glirdir`
   - executable: `Glirdir`
   - identifier: product identifier chosen consistently with renamed Lindelion naming
   - category: audio effect or generator as validated by host behavior
5. Update `Makefile`:
   - keep `make build PLUGIN=lamath` and `make build PLUGIN=glirdir` working, or
   - add `make build-all` for all bundleable plugins.
6. Decide whether `make build` should mean default plugin only or all plugins.

Acceptance:
1. `cargo run -p xtask -- bundle glirdir --target aarch64-apple-darwin` builds a `.vst3`.
2. `make build PLUGIN=glirdir BUNDLE_NAME=Glirdir.vst3` stages and installs Glirdir.
3. `make build-all` exists if both-plugin staging is desired.
4. Existing Lamath bundle path still works.

### 15. DAW Validation

Goal: prove the plugin works in the target workflow, not only in unit tests.

Work:
1. Validate bundle inspection:
   - Info.plist
   - executable name
   - exported VST3 symbols
   - codesign
2. Validate host loading:
   - Ableton scans plugin
   - plugin appears in expected category
   - editor opens/closes repeatedly
3. Validate capture:
   - immediate mode
   - next downbeat
   - phrase boundary
   - count-in 0/1/2
   - transport stop/resume behavior
4. Validate analysis:
   - capture completes
   - status changes to analyzing
   - notes appear
   - parameter changes rederive
5. Validate audition:
   - play/stop/loop
   - live edit
6. Validate drag:
   - MIDI drops into Ableton
   - tempo/grid alignment
   - clip contents match preview
7. Validate project reload:
   - scratchpad restored
   - settings restored
   - derived MIDI ready or rederived

Acceptance:
1. End-to-end workflow works: sing, capture, derive, audition, adjust, drag to Ableton.
2. No crash on close/reopen/rescan.
3. No obvious audio-thread stalls during capture/audition.

## Design Input Needed Before Implementation

1. Build semantics: should `make build` build the default selected plugin, or should it build every bundleable plugin? Recommendation: keep `make build` selected/default and add `make build-all`.

2. Bar 1 sync semantics: should "phrase boundary" mean every N bars from project start, loop-region start, or actual bar 1 only? Recommendation: every N bars from the current bar-position origin.

3. Drag fallback: if NSDragging is unreliable inside the plugin view, is "Copy MIDI to clipboard" acceptable as v1 fallback?

4. Scratchpad persistence: is FLAC a hard requirement for v1 VST3 state, or can a bounded binary float encoding ship first if FLAC encoder integration is risky?

5. Audition palette: keep sine-only for v1, or add a tiny palette? Recommendation: sine-only until drag-out and detection quality are proven.

## Final Done Criteria

Glirdir is complete for v1 when all of the following are true:

1. `cargo test --workspace` passes.
2. `cargo check --workspace` passes.
3. Lamath regression tests pass.
4. Glirdir VST3 bundle builds and installs on macOS.
5. Ableton loads Glirdir.
6. User can capture sung audio into the plugin.
7. SwiftF0/onset/segmentation produces visible notes.
8. Quantize/key/scale changes update MIDI without recapturing.
9. Audition plays the derived MIDI.
10. Dragging exports a MIDI clip into Ableton.
11. DAW project reload restores scratchpad and settings.
12. Build automation can stage Lamath and Glirdir intentionally.
