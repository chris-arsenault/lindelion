# Glirdir - Backlog

This file tracks work that is not part of the current implemented Glirdir spec in [glirdir.md](glirdir.md). It keeps deferred validation, storage changes, and product extensions out of the current implementation document.

---

## External Validation

- Build, stage, install, and inspect `Glirdir.vst3` on macOS.
- Confirm exported VST3 symbols and code signature.
- Confirm Ableton scans the plugin, shows it in the expected category, and opens/closes the editor repeatedly.
- Validate capture modes, count-in behavior, transport stop/resume, analysis status, live re-derivation, audition, drag/export, and project reload in Ableton.
- Record Apple Silicon performance numbers for 4, 8, and 16 bar captures.
- Validate direct AppKit drag from the embedded plugin NSView inside Ableton.
- If direct NSView drag is unreliable, keep pasteboard/export-to-file as the supported fallback and add an auxiliary overlay or window only if validation proves it is needed.

---

## Storage And Detection Hardening

- Replace bounded binary f32 scratchpad state with FLAC storage after a Rust encoder choice is selected and validated.
- Add recorded vocal and instrument fixture tests after manual validation identifies useful fixture categories and tolerances.
- Add high flute and piccolo fallback detection if source material above SwiftF0's current range becomes an actual product need.
- Address host-specific drag or editor lifecycle issues found during Ableton validation.

---

## Analysis Performance

- Implement streaming inference during capture so analysis is essentially complete when capture ends and the analyzing state disappears for normal captures.
- Parallelize analysis across cores if longer captures still make the worker analysis wait noticeable.
- Consider a CoreML-backed inference runtime only if pure-Rust inference is not fast enough for the intended macOS workflow.

---

## Product Extensions

- Multi-take history with recall and drag/export for prior captures.
- Percussion mode that maps unpitched onsets to user-selected MIDI notes.
- Polyphonic capture for chord-like audio input.
- Optional pitch bend output for sub-semitone expression.
- CC capture from vowel formants, breath energy, or similar analysis features.
- Optional key suggestion using pitch-class histogram analysis, with the user confirming the result.
- Loop or overdub mode where re-arm extends the capture instead of replacing it.
- MIDI 2.0 or MPE export for higher-resolution velocity and per-note expression.
- Separate audition output bus if host workflow proves main-output audition is too limiting.
- Build target that stages multiple plugin bundles in one command if maintaining separate selected-plugin builds becomes inconvenient.
