# Linnod Implementation Plan

This plan reconciles the Linnod current spec and backlog with the repository as it exists now. When the older spec conflicts with current repo architecture, the repo is the source of truth.

## User Decisions To Preserve

1. Library and patch storage should follow the current repo layout used by Lamath and Glirdir. Do not create a separate `Patches/Linnod/` layout just because the old spec mentions it.
2. The old Linnod documentation intended PSOLA, but that is not the implementation direction. Linnod must use the existing repo pitch detection path, especially `lindelion-pitch-detect` / SwiftF0, for pitch analysis and tuning decisions, and it must implement a real formant-preserving pitch-shift engine rather than falling back to normal sample-rate transposition.
3. Per-slice settings do not need to be host-automatable, but they must be preserved across plugin state reload and patch reload.
4. Do not invent a parallel persistence surface. Use the existing patch/state/parameter architecture correctly: `LinnodPatch` is the persistent source of truth, and the typed parameter registry should cover only the fields that are truly host parameters.

## Repo-Truth Corrections

1. `plugins/linnod` is currently a silent monolithic scaffold in `src/lib.rs`.
2. The current Linnod patch structs are not yet serde-backed and do not roundtrip through `TomlPatchFormat`.
3. `lindelion-psola` is currently placeholder-level and should not be the pitch-shift engine for Linnod. A non-PSOLA implementation should live behind a correctly named shared surface, not inside an algorithm-specific crate name that now describes the wrong design.
4. `lindelion-pitch-detect`, `lindelion-onset-detect`, `lindelion-phrase-analysis`, `lindelion-midi`, `lindelion-sample-library`, `lindelion-plugin-shell`, and `lindelion-ui` already provide shared surfaces the old spec treated as future work.
5. `ComplexFlux` and `SpectralSparsity` currently fall through to SuperFlux behavior in `ConfiguredOnsetDetector`; making them distinct algorithms is required Linnod implementation work and should extend `lindelion-onset-detect` rather than live locally in the plugin.
6. The sample library currently decodes mono WAV data and preserves source sample rate. Linnod should be sample-rate-aware rather than assuming every loaded source has been converted to 48 kHz unless a shared resampling utility is deliberately added.

## Pitch-Shift Architecture Decision

The best architecture for Linnod's core requirement is a shared source-filter / harmonic-plus-noise pitch-shift engine driven by SwiftF0 analysis. The offline analysis stage should estimate a trusted F0 contour with `lindelion-pitch-detect`, derive a pitch-adaptive spectral envelope, derive voiced/unvoiced plus aperiodic or residual energy descriptors, and cache those analysis products from the source sample and markers. The realtime playback stage should synthesize shifted harmonic excitation through the original spectral envelope so that pitch changes while formant placement is preserved.

This is not PSOLA. PSOLA was the pre-development intent in the old docs, and it can preserve spectral envelope for moderate monophonic speech shifts, but it couples the engine to epoch marking and waveform overlap-add artifacts. It is also not normal rate transposition, because rate transposition moves the formants with the harmonics and violates the product requirement.

ML alternatives should be treated as research or optional future backends, not the first implementation path. CLPCNet-style neural source-filter vocoders and source-filter HiFi-GAN variants are the relevant ML family, but they bring model training, model licensing, runtime integration, sample-rate, and domain-generalization risk. The shared engine should still be designed with backend traits so a licensed commercial engine or a trained neural backend can be evaluated later without changing Linnod's patch/runtime contract.

## Numbered Implementation Plan

1. Establish the Linnod foundation.
   Split the scaffold into repo-standard modules, then define the serde-backed `LinnodPatch`, `TomlPatchFormat` roundtrip, typed parameter registry, and patch-native slice editing model. Reuse `lindelion-midi` musical types instead of Linnod-local duplicates. Per-slice controls should persist through patch/plugin state but should not become host automation parameters unless explicitly chosen.

2. Extract shared support before adding a third copy.
   Move stable patch/library utilities and VST3 controller plumbing into shared crates where Lamath and Glirdir already duplicate behavior. This includes patch filename sanitization, default library-root handling, library patch save/load shape, normalized parameter arrays, patch mirrors, parameter info formatting, handler restart, and patch-to-processor message flow.

3. Integrate source-sample loading and background analysis.
   Use `FileSampleLibrary`, `SampleReference`, `RuntimeMonoAudioBuffer`, waveform previews, moved-file recovery, and hash/path fallback. Add Linnod's sequence-checked worker jobs for source load/ingest, waveform preview, SwiftF0 pitch contour, onset detection, marker reconciliation, and slice pitch summaries. Loading, ingest, decode, and analysis stay off the audio thread.

4. Complete the shared onset and marker/slice domain layer.
   Extend `lindelion-onset-detect` with real ComplexFlux and spectral-sparsity implementations instead of local Linnod copies. Keep SuperFlux, pitch-stability, energy/transient, and manual grid reusable. Implement marker sorting, dedupe, auto-marker replacement, user-marker merge/replace/cancel policy, zero-crossing snap, slice duration calculation, pad assignment validation, selected-slice lookup, and source-bound clamping as tested pure functions.

5. Build the shared formant-preserving pitch-shift engine.
   Introduce `lindelion-pitch-shift` or an equivalent correctly named shared crate rather than expanding `lindelion-psola` with non-PSOLA behavior. Use SwiftF0 as the authoritative F0 contour input, derive pitch-adaptive spectral envelopes, voiced/unvoiced segmentation, aperiodic or residual-energy descriptors, per-slice pitch summaries, and deterministic source-derived caches.

6. Implement pitch-shift synthesis and tuning behavior.
   Render shifted harmonic excitation through the unshifted spectral envelope, preserve slice duration, mix unvoiced/residual content with documented policy, and expose pitch ratio plus optional formant ratio through the engine contract. SwiftF0-backed analysis also drives detected fundamental display, cents deviation, tune-one, tune-all, and scale snap. Add tests that verify pitch changes while spectral-envelope peaks stay near their original frequencies.

7. Implement the realtime Linnod runtime.
   Add a 16-voice allocation-free runtime with forward/reverse cursors, one-shot/gated/looped modes, formant-preserving pitch shift, ADSR, gain, pan, low-pass filter, master gain, bounded output mixing, and MIDI trigger handling. Reuse or extend `VoiceManager` for chromatic stealing and pad-mode choke/retrigger ownership.

8. Prove realtime safety.
   Allocate voices, engine state, and scratch buffers during setup, source load, or structural patch application. Add `lindelion-test-allocator` no-allocation tests for note triggering, release, pad choking, and block rendering, then use `make ci` as the canonical verification path before commit-ready handoff.

9. Add editor, message, and VST3 integration.
   Build `linnod_vizia` in `lindelion-ui` for waveform markers, detection controls, tuning controls, 4x4 pad grid, trigger modes, selected-slice editing, and slice list. Use typed VST3 messages for patch updates, sample load/ingest, analysis responses, marker edits, slice edits, status, and telemetry. Implement Linnod processor/controller/factory/messages/editor as an instrument with stable CIDs.

10. Finish bundle automation, docs, and validation.
    Extend `xtask`, `Makefile`, and macOS VST3 docs for `PLUGIN=linnod`. Move completed behavior from backlog into `docs/plugins/linnod.md`, keep only unfinished items in `docs/plugins/linnod-backlog.md`, and complete validator/Ableton/performance validation.

## Suggested Milestones

1. Patch/schema/parameter foundation.
2. Shared extraction for patch/library utilities and VST3 parameter plumbing.
3. Source sample loading, waveform preview, and patch/state roundtrip.
4. Full shared onset detector implementations and marker/slice domain logic.
5. Shared formant-preserving pitch-shift analysis and synthesis engine.
6. Basic sample-playback runtime and no-allocation tests.
7. SwiftF0-backed tuning and scale snap.
8. VST3 processor/controller/messages without full editor polish.
9. Vizia Linnod editor and sample/marker editing workflows.
10. Bundle automation, docs, validator/Ableton validation, and performance polish.
