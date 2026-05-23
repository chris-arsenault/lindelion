# Resonator Reusable Architecture Debt

This tracks places where the current resonator implementation still relies on
surface-level branches, copied protocol mappings, or plugin-local glue instead
of reusable architecture. It is a working tracker, not durable documentation, so
it lives in the repo root.

The numbered items are stable references for implementation and discussion.

## Implementation Debt

1. **ARCH-001: Replace duplicated parameter ID switches with typed parameter bindings.**
   - Status: Fixed. Resonator parameters now flow through a typed
     `ParameterBinding` registry with shared patch get/set, apply policy,
     runtime target, VST3 formatting, editor signal metadata, and
     `ParameterCodec` enum conversions.
   - Problem: parameter IDs are interpreted independently in patch access,
     runtime live application, VST3 value formatting, and editor signal updates.
     This creates drift risk every time a parameter is added or renamed. Scope
     measured by review: ~540 contiguous LOC of parameter dispatch in
     `lib.rs:1164–1703` alone, plus a 28-branch editor match and a 30-branch
     formatter match, totalling 4 redundant interpretations of the same id space.
   - Current examples:
     `plugins/resonator-synth/src/lib.rs::patch_parameter_plain_value`,
     `plugins/resonator-synth/src/lib.rs::apply_parameter_plain`,
     `plugins/resonator-synth/src/lib.rs::resonator_parameter_plain_value`,
     `plugins/resonator-synth/src/lib.rs::apply_resonator_parameter`,
     `plugins/resonator-synth/src/lib.rs::apply_modal_parameter_if_selected`,
     `plugins/resonator-synth/src/lib.rs::apply_waveguide_parameter_if_selected`,
     `plugins/resonator-synth/src/runtime.rs::set_parameter_plain`,
     `plugins/resonator-synth/src/vst3_entry.rs::format_parameter_plain_value`,
     and `plugins/resonator-synth/src/vst3_entry/editor.rs::update_signal`.
   - Sibling duplication: six structurally identical enum codec pairs in
     `lib.rs` — `modal_preset_from_plain`/`modal_preset_plain`,
     `lfo_shape_from_plain`/`lfo_shape_plain`,
     `modulation_source_from_plain`/`modulation_source_plain`,
     `modulation_destination_from_plain`/`modulation_destination_plain`,
     `filter_mode_from_plain`/`filter_mode_plain`,
     `waveguide_style_from_plain`/`waveguide_style_plain` — plus the
     `parallel_mix_a`/`parallel_mix_b` and `routing_plain`/`routing_from_plain`
     variants of the same pattern.
   - Required fix: introduce a typed `ParameterBinding` registry that owns id,
     range, label formatting, patch getter/setter, apply policy, live engine
     target, and optional editor binding. Fold the enum codec pairs into a
     single `ParameterCodec` trait (or derive macro) so adding an enum-valued
     parameter is one impl, not two free functions.
   - First tests: assert every exposed `PARAMETERS` entry has exactly one
     binding, every binding round-trips patch get/set where applicable, and VST3
     formatting/editor projection consume the same binding metadata. Add a
     round-trip property test per enum codec.

2. **ARCH-002: Replace editor command float codes with a typed command bus.**
   - Problem: editor commands are encoded as `f32` magic numbers and decoded
     back to `UiCommand`, which is brittle and not reusable across editors or
     plugins.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry/editor.rs::command_code` and
     `plugins/resonator-synth/src/vst3_entry/editor.rs::command_from_code`.
   - Required fix: move command transport into a typed `EditorCommandBus` or
     equivalent in `ahara-ui`/`ahara-plugin-shell`, with primitive serialization
     hidden behind one adapter only if the host UI layer requires it.
   - First tests: command encode/decode round-trips every `UiCommand`, invalid
     payloads are ignored safely, and selected-slot commands preserve their slot
     identity.

3. **ARCH-003: Move editor action handling into reusable services.**
   - Problem: patch save/load/export, sample ingest, slot assignment, and
     telemetry requests are handled by a plugin-local command branch in the VST3
     editor.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry/editor.rs::handle_editor_command`
     and `plugins/resonator-synth/src/vst3_entry/editor.rs::assign_sample_to_slot`.
   - Required fix: introduce reusable services such as `PatchIoService`,
     `SampleSlotService`, and `EditorCommandHandler`, so future plugins share
     the same host-safe file-action and processor-transfer pattern.
   - First tests: command handler invokes the correct service method for patch
     save/load/export and sample slot commands, while file-dialog selection stays
     outside the audio path.

4. **ARCH-004: Move VST3 message strings and payload plumbing into a typed plugin-message layer.**
   - Problem: controller/processor messages use plugin-local string IDs and raw
     payload extraction, so patch updates and telemetry are encoded as local
     protocol glue. The supporting C-FFI string helpers and COM wrappers are
     plugin-agnostic but live alongside resonator code.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry.rs::MESSAGE_PATCH_UPDATE`,
     `MESSAGE_TELEMETRY_REQUEST`, `MESSAGE_TELEMETRY_RESPONSE`,
     `PluginMessage`, `PluginAttributes`, `message_id`, `message_payload`,
     `copy_cstring`, `copy_wstring`, and `len_wstring`.
   - Related: the controller’s `IConnectionPointTrait::notify` dispatch
     (`vst3_entry.rs:611–626`) matches on these string IDs inline, so a typed
     enum here also cleans up the receive path.
   - Required fix: add typed message definitions and payload encode/decode
     helpers to `ahara-plugin-shell`, with plugin-specific payloads layered on
     top. Move the FFI/string helpers and `PluginAttributes` wrapper there too.
   - First tests: patch-update and telemetry messages round-trip through the
     shared layer, unknown message IDs are ignored safely, and malformed payloads
     cannot panic.

5. **ARCH-005: Move host MIDI normalization out of the VST3 entrypoint.**
   - Problem: `MidiExpressionSource` now owns expression mapping, but low-level
     host MIDI normalization still lives in VST3-local branches. The CC dispatch
     specifically is a 37-branch match that will be re-written verbatim in the
     next plugin adapter unless lifted.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry.rs::vst_event_to_midi`
     (event-type match, `1505–1534`),
     `legacy_midi_cc_to_event` (CC dispatch, `1539–1575`),
     `midi7`, and `pitch_bend_semitones`.
   - Related shared-layer cleanup: `crates/ahara-plugin-shell/src/events.rs`
     currently hard-codes the recognized CC set to `CC#1` and `CC#74`
     (`events.rs:186–191`) and clamps pitch bend to ±96 semitones
     (`events.rs:355–359`); the normalizer should make this configurable per
     plugin so slicer is not forced into resonator’s two-wheel design.
   - Required fix: introduce a shared `MidiEventNormalizer` in
     `ahara-plugin-shell` so VST3/AU/CLAP adapters all map host MIDI into the
     same internal `MidiEvent` representation, with a per-plugin CC routing
     table and pitch-bend range supplied at construction.
   - First tests: CC1, CC74, channel pressure, poly pressure, note on/off, and
     pitch bend normalize identically from shared test fixtures. A second
     fixture with a different CC routing table and pitch-bend range exercises
     the configurable path.

6. **ARCH-006: Derive editor parameter surface from parameter metadata.**
   - Problem: visible editor parameter IDs and Vizia signals are manually
     curated, so the editor can silently drift from the plugin parameter model.
     The 28-branch id-to-signal match (`editor.rs:1501–1531`) and the static
     `VISIBLE_PARAMETER_IDS` array (`editor.rs:257–260`) are two independent
     hand-maintained projections of `PARAMETERS`.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry/editor.rs::VISIBLE_PARAMETER_IDS`,
     editor signal fields, and the parameter-id-specific update logic.
   - Required fix: extend parameter metadata with editor grouping/layout hints,
     then build editor controls and signal routing from the same parameter
     registry used by host automation. Replace the id-to-signal match with a
     map populated at editor init time from `PARAMETERS`.
   - First tests: every visible editor control maps to a real parameter binding,
     every required group has at least one visible parameter, and removed
     parameters cannot remain in the editor surface.

7. **ARCH-007: Split monolithic `lib.rs` and `vst3_entry.rs` into focused modules.**
   - Problem: `plugins/resonator-synth/src/lib.rs` is 3791 LOC mixing module
     wiring, plugin descriptor + `PARAMETERS` table, patch data structures,
     parameter dispatch trees, the `AudioPlugin` impl, telemetry/state
     serialization, and ~2000 LOC of tests. `plugins/resonator-synth/src/vst3_entry.rs`
     is 2108 LOC mixing processor impls, controller impls, data marshalling,
     factory boilerplate, MIDI conversion, and parameter formatting. File size
     this large makes diff review noisy and discourages safe surgical refactors
     (ARCH-001, ARCH-004, ARCH-005 all touch these files).
   - Current examples:
     `plugins/resonator-synth/src/lib.rs` (sections: descriptor 84–128, patch
     types 536–800, plugin state 842–1006, patch loading 1027–1155, parameter
     dispatch 1164–1703, plugin-trait impl 1704–1777, tests 1779–3791);
     `plugins/resonator-synth/src/vst3_entry.rs` (sections: processor 48–414,
     controller 416–627, data marshalling 629–856, factory 1261–1401, MIDI
     conversion 1505–1575, parameter formatting 1598–1681).
   - Also: `vst3_entry.rs` exists as a sibling file to the `vst3_entry/`
     directory; convert to `vst3_entry/mod.rs` for clarity.
   - Required fix: split `lib.rs` into `lib.rs` (re-exports + descriptor),
     `parameters.rs` (apply / plain conversions), `patch.rs` (data structures),
     `plugin.rs` (`AudioPlugin` impl + state ser); split `vst3_entry.rs` into
     `processor.rs`, `controller.rs`, `factory.rs`, `messages.rs`,
     `midi.rs`. Move tests under `tests/` or feature-scoped files.
   - First tests: existing test suite continues to pass after the split; no
     new public items are exposed at crate roots; `cargo check
     --target aarch64-apple-darwin` still succeeds.

8. **ARCH-008: Move the Vizia editor UI out of the VST3 plugin into `ahara-ui`.**
   - Problem: `plugins/resonator-synth/src/vst3_entry/editor.rs` is 2106 LOC, of
     which only ~180 are VST3 `IPlugView` plumbing; the remaining ~1900 LOC
     under `mod macos` is Vizia application code (panels, knob/slider
     builders, file dialogs, sample drawer, signal wiring). `crates/ahara-ui`
     is currently a 155-line stub even though it already supplies `PadId`,
     `UiCommand`, etc. that this editor consumes. The UI cannot be reused for
     slicer or for a future standalone host without lifting it out.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry/editor.rs::macos` (`231–2106`),
     `build_application` (`684`), `excitation_column`, `resonator_column`,
     `output_column`, sample-drawer builders, and the parameter-signal struct.
   - Required fix: move the `macos` Vizia module into
     `crates/ahara-ui/src/resonator_vizia.rs` (or a new `ahara-vizia-ui`
     crate if the dep should be optional), expose a builder that takes
     controller callbacks as trait objects or closures, and keep only the
     `IPlugView` stub + editor factory in `vst3_entry/editor.rs`.
   - First tests: editor crate builds without the resonator plugin in scope,
     a smoke test constructs the Vizia application from a mock binding, and
     the VST3 editor stub still attaches/detaches against the host adapter.

9. **ARCH-009: Lift hand-rolled VST3 plumbing into `ahara-plugin-shell`.**
   - Problem: the resonator plugin crate currently owns plugin-agnostic VST3
     plumbing (factory boilerplate, class-id dispatch, FFI string helpers,
     `IPlugView` platform stubs). When slicer ships, every line will be
     duplicated; future hosts (AU, CLAP) will fork it again.
   - Current examples:
     `plugins/resonator-synth/src/vst3_entry.rs::PluginFactory` boilerplate +
     `FactoryClass` enum dispatch (`1261–1401`),
     `vst3_entry.rs::copy_cstring`, `copy_wstring`, `len_wstring`
     (`1682–1717`), and the `IPlugView` lifecycle stubs in
     `plugins/resonator-synth/src/vst3_entry/editor.rs::ResonatorEditorView`
     (`25–180`).
   - Note: hand-rolled COM is intentional per CLAUDE.md — the goal here is
     centralization, not adopting a plugin framework.
   - Required fix: introduce `ahara-plugin-shell::vst3` (or
     `ahara-vst3-shell`) housing a generic factory builder, class
     registration table, FFI helpers, and `IPlugView` base stub; plugin
     crates only declare their CIDs, parameter set, and editor-construction
     hook.
   - First tests: resonator-synth registers its two CIDs through the shared
     factory, the factory enumerates them via `IPluginFactory` calls, and the
     FFI string helpers round-trip ASCII and non-ASCII inputs without
     overflow.

10. **ARCH-010: Decompose the `Voice` struct into substates.**
    - Problem: `Voice` carries 42 fields covering excitation, two resonator
      engines, routing, dual ADSR pairs, an LFO, pitch-bend smoothing,
      waveguide loop-gain smoothing, output filter, gain, pan, and saturation.
      Adding any new per-voice behavior requires touching the same monolithic
      struct and its init / `process_sample` / `set_*_config` methods.
    - Current examples:
      `plugins/resonator-synth/src/dsp/voice.rs::Voice` (`136–177`),
      `process_sample` (`318–485`), and the cluster of `set_output_config`,
      `set_routing`, modulation update helpers (`362–396`, `526–624`).
    - Required fix: extract `OutputStage` (filter + gain + pan + saturation,
      `163–169`), `ModulationState` (LFO + bend + waveguide loop gain,
      `161–162, 172–175`), and `ResonatorStack` (two engines + routing,
      `141–151`) as owned substructs with their own `process` / `update`
      methods. Keep dispatch in `Voice::process_sample` thin.
    - First tests: existing voice tests pass unchanged; each substate has at
      least one unit test exercising its update path; the audio-thread
      no-allocation assertion still holds (`engine.rs:693–707`).

11. **ARCH-011: Shared voice allocation / stealing in `ahara-plugin-shell`.**
    - Problem: voice allocation, stealing policy, per-channel/per-note
      tracking, and retrigger handling currently live inside the resonator
      engine. Slicer will need an equivalent — and the next plugin after that
      will fork it again unless lifted now.
    - Current examples:
      `plugins/resonator-synth/src/dsp/engine.rs::choose_voice_slot`,
      `VoiceSlotState`, and the per-channel/per-note bookkeeping fields on
      the engine.
    - Required fix: introduce `ahara-plugin-shell::voices` with a generic
      `VoiceManager<const N: usize, V: VoiceLike>` that handles allocation,
      stealing, retrigger policy, and per-channel pressure routing; the
      plugin only supplies the per-voice `VoiceLike` impl.
    - First tests: deterministic stealing fixture (oldest-non-sustaining
      first), retrigger respects sustaining notes, and per-channel pressure
      is delivered only to voices on that channel.

12. **ARCH-012: Generic atomic-parameter → smoothed-parameter bridge.**
    - Problem: each parameter that needs smoothing is wired by hand from its
      atomic in the plugin state to a `SmoothedParam` in the voice/engine.
      The pattern is repeated dozens of times in `lib.rs` and `runtime.rs`
      and will be redone in slicer.
    - Current examples: the per-parameter `set_*` calls in
      `plugins/resonator-synth/src/runtime.rs::set_parameter_plain` and the
      bespoke smoother-update calls scattered through `lib.rs::apply_parameter_plain`.
    - Required fix: provide a `SmoothedAtomicParam` (or similar) in
      `ahara-dsp-utils` / `ahara-plugin-shell` that pairs an
      `AtomicParameter` with a `SmoothedParam` and is fed by the parameter
      registry from ARCH-001.
    - First tests: round-trip a parameter through atomic write → smoothed
      read with sample-accurate ramp length; ensure zero allocation on the
      audio thread when the atomic changes.

13. **ARCH-013: Generic TOML patch I/O in `ahara-plugin-shell`.**
    - Problem: patch serialization is currently a resonator-shaped module
      (`patch_io.rs`) and slicer will need a parallel copy with the same
      atomic-write / version-tagging / migration scaffolding.
    - Current examples: `plugins/resonator-synth/src/patch_io.rs`
      (load/save/export), and the state stream wiring in
      `plugins/resonator-synth/src/lib.rs` plugin-impl section
      (`1704–1777`).
    - Required fix: move the version envelope, atomic file write, and
      `PluginState` round-trip into `ahara-plugin-shell`, generic over the
      plugin payload type; plugins supply only the Serde type.
    - First tests: patch round-trips via the shared layer for both
      resonator and a synthetic payload; a malformed file produces a
      typed error (no panic); a forward version tag fails cleanly with a
      migration hook.

14. **ARCH-014: Consolidate DSP magic numbers into named constants.**
    - Problem: while voice-level constants (`INTERNAL_HEADROOM_DB`,
      `PARAMETER_SMOOTH_MS`, etc.) are already named, several semantic
      constants and formulas are duplicated as bare literals across the
      DSP modules. Tuning these for a future plugin variant requires
      grepping across files.
    - Current examples:
      filter bounds `20.0 / 20_000.0` at
      `plugins/resonator-synth/src/dsp/voice.rs:467–469, 708, 717–719`,
      `plugins/resonator-synth/src/lib.rs:98, 203`;
      default waveguide loop gain `0.92` at
      `plugins/resonator-synth/src/dsp/voice.rs:241, 375, 604, 756, 804–806`;
      biquad Q formula `0.55 + resonance * 4.0` at
      `plugins/resonator-synth/src/dsp/voice.rs:818` and
      `plugins/resonator-synth/src/dsp/waveguide.rs:174`;
      default Q `0.707` at
      `plugins/resonator-synth/src/dsp/voice.rs:817, 959, 967`;
      `SeriesConditioner` alphas `0.01, 0.000_2, 0.04, 0.96` at
      `plugins/resonator-synth/src/dsp/voice.rs:977–982`;
      tube boundary coefficients `0.25, 0.75, 0.8, 0.2` at
      `plugins/resonator-synth/src/dsp/waveguide.rs:141–163`.
    - Required fix: create `plugins/resonator-synth/src/dsp/constants.rs`
      grouping these into named consts (and small parameter structs for
      the boundary model and conditioner) so each formula has a single
      definition site.
    - First tests: existing DSP unit tests pass unchanged; a new test
      pins the numerical output of the boundary model and the conditioner
      so future tuning is intentional.
