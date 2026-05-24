# Lamath v2 Implementation Plan

## Product Decisions

1. Sidechain audio creates notes. v2 is not only an expression follower for MIDI-held voices.
2. Live audio excitation supports both modes, configurable per patch:
   - continuous sidechain drive into active voices;
   - note-latched excitation snippets captured around audio-created or MIDI note starts.
3. Lamath remains the existing VST3 instrument and gains an optional audio input bus. Do not create a separate effect or sidechain-only plugin variant for v2.

## Numbered Plan

1. Update the Lamath spec before code changes.

   Rewrite the v2 sections of `docs/plugins/lamath.md` so they no longer describe audio analysis as an architectural seam only. The spec should state the chosen host contract, note-creation behavior, live-excitation modes, parameter surface, state format, and realtime constraints. Keep the durable shared-crate rules in `docs/architecture.md` as the implementation standard.

2. Add optional audio input bus support to the shared VST3 bus metadata.

   Extend `lindelion-plugin-shell::vst3_component::Vst3BusInfo` so a bus can declare bus type and default-active flags instead of always reporting `kMain` and `kDefaultActive`. Add tests for optional audio inputs and keep existing output/event behavior unchanged.

3. Add Lamath's optional sidechain input bus.

   Update `plugins/lamath/src/vst3_entry/processor.rs` so `RESONATOR_BUSES` exposes stereo output, optional mono or stereo audio input named `Sidechain Input`, and MIDI input. The processor should continue to work when the host supplies no input bus or an inactive/empty input bus.

4. Route sidechain audio through `ProcessContext`.

   Use the existing shared `ProcessContext::input` / `AudioInputBuffer` path instead of adding a Lamath-local host DTO. `ResonatorSynth::process` should project the optional input to a preallocated mono analysis/excitation scratch buffer, then pass that block into the runtime. Empty input must behave exactly like v1.

5. Add the v2 patch and parameter surface.

   Add registry-backed parameters and patch fields for:
   - audio input mode: off, audio creates notes, MIDI plus audio creates notes;
   - audio expression enable and mapping values already represented by `AudioExpressionMapping`;
   - note detection thresholds: onset sensitivity, note release floor, minimum note length, pitch confidence, velocity amount;
   - live excitation mode: off, continuous, note-latched, continuous plus note-latched;
   - live excitation gain, latch window length, latch pre-roll, and latch fade.

   Keep Lamath patch paths, apply policies, and runtime targets local. Use shared parameter codecs and registry projection.

6. Extend `lindelion-audio-expression` with a streaming audio note surface.

   Add shared host-neutral DTOs for audio-created note lifecycle events, for example `AudioNoteEvent { offset, note, velocity, pitch_hz, gate }`. Build them from existing streaming pitch, onset, and loudness trackers. The shared layer owns feature extraction and sanitization; Lamath owns how those events allocate voices, interact with MIDI, and route excitation.

7. Implement Lamath audio-note state.

   Add runtime state that tracks the currently active audio-created note, release hysteresis, last stable pitch, velocity from loudness, and note-off generation when the gate closes. On new sidechain onsets, allocate/retrigger a voice through the existing voice manager path rather than bypassing it. Generated audio notes should use the same `ExpressionSource` and `VoiceManager` contracts as MIDI notes.

8. Integrate audio expression with audio-created notes.

   Use `StreamingAudioAnalysisExpressionSource` for live pitch bend, pressure, and brightness. For audio-created voices, pitch bend is relative to the note chosen at onset; pitch drift after onset should become expression, not repeated note churn. Keep MIDI expression behavior unchanged for MIDI-created voices.

9. Define MIDI and audio interaction policy.

   Implement deterministic behavior when MIDI and audio input happen together:
   - MIDI-only mode preserves v1 behavior.
   - Audio-create mode ignores MIDI note allocation but may still accept transport and automation.
   - MIDI-plus-audio mode allows both sources, with separate ownership so audio note-offs cannot release MIDI voices and MIDI note-offs cannot release audio voices.

10. Add continuous live excitation.

   Add a runtime excitation source that can mix the current sidechain block into active voices without allocation. Continuous mode should be level-controlled, sanitized, bounded, and optional per patch. It should be available for MIDI-created and audio-created voices according to the patch policy.

11. Add note-latched live excitation.

   Allocate fixed-size per-voice latch buffers at setup based on the configured maximum latch window. Maintain a preallocated sidechain pre-roll ring buffer. On audio onset or MIDI note-on, copy the configured pre-roll plus post-onset window into the voice latch buffer, apply short fades, and feed that buffer through the existing excitation playback path. Changing latch length should use an explicit structural apply policy.

12. Keep the audio thread allocation-free.

   All sidechain scratch buffers, detector state, active-note state, pre-roll ring buffers, and per-voice latch buffers must be allocated during setup or structural patch application. Add `lindelion-test-allocator` coverage for process blocks with audio input, audio note creation, continuous excitation, note-latched excitation, and mixed MIDI/audio mode.

13. Update state and patch I/O.

   Lamath has no deployed pre-v2 compatibility surface, so do not add a migration layer for v2. The current patch and DAW state payloads should roundtrip the v2 audio input, audio expression, note detection, and live excitation fields directly. Add TOML and `PluginState` roundtrip tests.

14. Update the editor and controller messaging.

   Add UI controls derived from the parameter registry for audio input mode, note detection, expression mapping, and live excitation mode. Add visible status for input detected, note detected, pitch confidence, and missing/inactive sidechain input. Keep product-specific status payloads in Lamath VST3 message types.

15. Add focused tests before host validation.

   Required tests:
   - VST3 bus info exposes the optional sidechain bus.
   - Empty sidechain input preserves v1 output.
   - Sidechain onset creates a note and later releases it.
   - Pitch drift after onset becomes pitch bend for the audio-created voice.
   - MIDI and audio voices do not steal each other's note-off ownership.
   - Continuous excitation changes rendered audio and remains finite.
   - Note-latched excitation captures the expected window and remains finite.
   - No audio-thread allocations occur in all enabled v2 modes.

16. Measure latency and CPU.

   Record the effective note-on latency from sidechain onset to audible voice, including detector windowing. Document default detector/latch settings that keep the instrument playable on Apple Silicon. If SwiftF0 block latency is too high for note creation, keep the shared streaming trait and swap only the pitch tracker implementation.

17. Run full validation.

   Run `cargo fmt --all -- --check`, `cargo test --workspace`, and targeted Lamath realtime/no-allocation tests. On macOS, build the VST3 bundle, run Steinberg validator, and scan/load in Ableton with the optional sidechain routed and unrouted.

18. Finish by updating docs.

   Update `docs/plugins/lamath.md`, `docs/architecture.md` if a new durable rule emerges, and `plugins/lamath/README.md` with the v2 behavior. The docs should make clear that shared audio analysis lives in `lindelion-audio-expression`, while Lamath owns voice allocation, sidechain bus policy, and excitation routing.

## Completion Criteria

1. MIDI-only behavior remains compatible with the default/current patch.
2. The existing Lamath VST3 instrument exposes an optional sidechain input bus.
3. Sidechain audio can create and release notes without MIDI note input.
4. Continuous and note-latched live excitation both work and are patch-configurable.
5. Audio expression, note detection, and excitation processing use shared detector/expression primitives where they are host-neutral.
6. Product-specific behavior remains local to Lamath path, policy, message, runtime-target, and UI-slot types.
7. Realtime tests prove the enabled v2 audio path does not allocate.
