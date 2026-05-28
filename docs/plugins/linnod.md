# Linnod - Current Implementation Spec

**Name:** Linnod
**Name etymology:** Sindarin, a measured verse unit: a half-line of 4+3 syllables forming a distinct portion of a larger song.
**Target:** macOS VST3 instrument, Apple Silicon primary.
**Status:** VST3 melodic sample-slicer instrument with patch/state persistence, source loading and analysis, shared marker/slice domain logic, SwiftF0-backed tuning, setup-time Resample Stretch slice rendering, allocation-free slice playback, typed editor/VST3 messaging, and macOS bundle automation.

This document describes the behavior implemented in the workspace today. Deferred product extensions and external host-validation follow-up live in [linnod-backlog.md](linnod-backlog.md).

---

## 1. Product Intent

Linnod is Lindelion's melodic sample slicer. It targets monophonic melodic source material such as wind, voice, and bowed strings, detects or accepts slice markers, and turns those slices into MIDI-triggered rhythmic chops.

The core design keeps product policy local to `plugins/linnod` while shared Lindelion crates carry reusable plugin-shell, sample-library, onset, pitch-detect, pitch-shift, MIDI, UI, DSP, and state machinery.

---

## 2. Runtime Boundary

Current crate: `plugins/linnod`.

Implemented modules:

- `patch.rs`: serde-backed `LinnodPatch`, 16-slice layout, pad map, pad choke groups, trigger mode, tuning config, and per-slice persisted controls.
- `parameters.rs`: three host parameters: master gain, detection sensitivity, and tuning reference. Per-slice controls persist in the patch and are intentionally not host automation parameters.
- `analysis.rs` and `analysis_job.rs`: off-thread source load/ingest, sample-library recovery, waveform metadata, SwiftF0 pitch contour, onset detection, marker reconciliation, slice pitch summaries, and shared pitch-shift cache construction.
- `runtime.rs`: 16-voice slice playback with pad/chromatic triggering, cross-pad choke groups, forward/reverse cursors, one-shot/gated/looped playback, ADSR, gain, pan, low-pass filtering, master gain, MIDI expression, and bounded output.
- `tuning.rs`: tune selected slice, tune all slices, and scale snap using the shared pitch-shift analysis cache.
- `vst3_entry/`: VST3 processor, controller, factory, typed messages, editor bridge, state streams, MIDI normalization, and worker result routing.
- `lindelion-ui::linnod_vizia`: editor host contract for parameters, waveform markers, detection/tuning controls, pad grid, selected-slice editing, slice list, status, and telemetry.

Audio-thread behavior:

- Source loading, decode, hashing, pitch detection, onset detection, marker reconciliation, and pitch-shift cache construction run off the audio thread.
- The realtime path renders from already-published source analysis and preallocated voice state.
- No-allocation tests cover note triggering, note release, pad choke/retrigger, and block rendering.

---

## 3. Patch And State

`LinnodPatch` is the persistent source of truth. It contains:

- patch name;
- output master gain;
- optional shared-library source sample reference;
- onset detection config;
- marker list with auto/user marker kind;
- 16 default `SliceParams` entries;
- tuning reference, scale, and root;
- pad/chromatic trigger mode;
- active chromatic pad;
- 16 default pad assignments mapped to MIDI notes 36-51, each with an optional persisted choke group.

Each `SliceParams` entry contains:

- name;
- start and end offsets;
- pitch shift in semitones and cents;
- gain and pan;
- reverse flag;
- playback mode: one-shot, gated, or looped;
- ADSR values;
- one-pole low-pass cutoff.

Patches roundtrip through the shared `TomlPatchFormat`. VST3 DAW state stores the same patch payload through the shared plugin-state stream path. Cached pitch analysis is deterministic from source audio, markers, and pitch contour; it is rebuilt by the worker instead of being serialized into the patch.

Linnod uses the workspace's shared patch and sample-library conventions rather than a separate product-specific storage layout. Plugin-specific behavior stays in the Linnod patch schema, editor messages, and runtime policy.

---

## 4. Source Analysis And Pitch Shifting

Linnod uses the shared sample library for source ingest and moved-file recovery. Sources are decoded to mono while preserving source sample rate.

The source-analysis worker builds:

- `SampleMetadata` and waveform preview;
- SwiftF0 pitch contour from `lindelion-pitch-detect`;
- auto markers from `lindelion-onset-detect`;
- reconciled markers using shared auto/user marker policy;
- per-slice pitch summaries;
- a deterministic `lindelion-pitch-shift::PitchShiftSourceCache`.

Pitch shifting is a required Linnod capability for pad and chromatic playback, not an optional enhancement. Small shifts such as 1 cent should remain nearly identical to the source except for the intended pitch change, and semitone shifts should avoid added high-frequency noise, saturation-like coloration, or loop-boundary discontinuities. Fixes must preserve Linnod's fixed-duration pitch-shift semantics unless the user explicitly approves a product behavior change.

Linnod uses the shared pitch-shift crate. `ResampleStretch` is implemented as a phase-aware Resample Pro path: source-level STFT analysis, time scaling at the requested pitch ratio, and bandlimited windowed-sinc resampling back to fixed slice duration. The durable DSP reference is [Resample Pro pitch shifting](../dsp/resample-pro.md).

`ResampleStretch` is not PSOLA and is not plain sample-rate transposition. PSOLA-like behavior remains limited to the older pitch-synchronous source-filter mode, while varispeed remains the explicit mode that changes playback increment and moves formants with the sample.

Linnod keeps Resample Pro rendering outside MIDI note playback. Source and patch preparation resolve the non-identity Resample Stretch variants required by the current patch, render complete slice buffers with guard context, and cache them by source cache key, slice range, pitch ratio, formant ratio, playback direction, and render-config version. Voices read those prepared buffers and apply only playback cursor, declick, envelope, gain, pan, filter, and output processing.

Pad mode prepares the exact slice/pitch variants implied by the pad map. Chromatic mode prepares the selected root-note variant for the active chromatic pad. A chromatic note requiring a shifted variant that is not prepared is silent rather than falling back to direct unshifted playback or another pitch algorithm.

The older source-filter modes remain available for their existing semantics. When formants track pitch, voiced playback can use a pitch-synchronous overlap-add style path; when formants are preserved independently, source-filter synthesis uses pitch-adaptive spectral envelopes, voicing segments, and residual descriptors.

---

## 5. VST3, Editor, And Bundle Metadata

Linnod is registered as a VST3 instrument:

| Field | Value |
| ---- | ---- |
| Bundle name | `Linnod` |
| Executable | `Linnod` |
| Bundle identifier | `com.ahara.linnod` |
| Subcategories | `Instrument`, `Sampler` |

Stable CIDs live in the shared `VST3_BUNDLE_METADATA` constant and are consumed by the plugin factory and `xtask` bundle automation.

The VST3 adapter exposes:

- stereo audio output;
- MIDI event input;
- shared state stream read/write;
- typed controller/processor messages for patch updates, sample load/ingest, analysis responses, marker edits, pad edits, slice edits, status, and telemetry;
- a fixed-size editor view backed by `linnod_vizia`.

Build and validation commands:

```bash
make build PLUGIN=linnod
make inspect-vst3 PLUGIN=linnod
make validate-vst3 PLUGIN=linnod
```

The validator target requires Steinberg's `validator` executable on macOS. Set `VST3_VALIDATOR=/path/to/validator` when needed.

---

## 6. Performance Coverage

Current coverage:

- allocation tests for note-on, release, pad choke/retrigger, cross-pad choke groups, and block render;
- prepared Resample Stretch tests proving note playback reads cached buffers and does not invoke the Resample Pro renderer;
- finite-output runtime tests for source-backed rendering;
- Criterion runtime benches under `plugins/linnod/benches/runtime.rs`;
- `make ci` compiles all benches through `cargo bench --workspace --no-run`.

The benchmark scope and release-measurement procedure are documented in [../perf/linnod.md](../perf/linnod.md).

---

## Appendix A - Glossary

- **Slice:** playback region derived from a source sample marker.
- **Marker:** a start point in a source sample, tagged as auto or user.
- **Pitch-shift cache:** deterministic source-derived analysis from audio, markers, and SwiftF0 pitch contours used by formant-preserving synthesis.
- **Pad mode:** pads trigger assigned slices at original pitch.
- **Chromatic mode:** one selected slice plays across the keyboard.
- **Choke group:** optional pad assignment group where a new pad trigger stops other active voices in the same group on the same MIDI channel.
