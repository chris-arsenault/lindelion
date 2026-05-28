# Linnod - Backlog

This file tracks work that is not part of the current implemented Linnod spec in [linnod.md](linnod.md).

---

## External Validation

- Build, stage, install, inspect, and validate `Linnod.vst3` on a macOS machine with Steinberg's validator installed.
- Confirm Ableton scans Linnod as a separate instrument and lists it independently from Lamath and Glirdir.
- In Ableton, validate source load, source ingest, marker edits, slice edits, pad mode, chromatic mode, tune selected, tune all, scale snap, editor open/close, project save/reload, and missing-source recovery.
- Record Apple Silicon runtime numbers for 1, 8, and 16 active voices with formant-preserving pitch shift active.

---

## Product Polish

- Add per-patch undo/redo for marker and slice edits.
- Add waveform zoom, pan, and direct waveform audition.
- Expand status text for source-analysis failures and missing-source recovery.
- Add explicit pad remapping UI once the first Ableton validation pass confirms the host MIDI-note workflow.
- Add preset/library browser polish shared with Lamath and Glirdir if the three products converge on a common patch browser.

---

## Product Extensions

- Add bounded chromatic Resample Stretch variant preparation. Chromatic mode plays one selected slice across the keyboard, so a Resample Stretch patch needs a defined MIDI-note range, memory limit, and setup-time policy for pre-rendering shifted variants of that slice. Until that policy exists, Linnod prepares only the selected root-note variant and treats any unprepared shifted chromatic note as silence rather than falling back to unshifted audio or another pitch algorithm.
- Multiple banks of 16 pads.
- Multiple source samples per patch.
- Per-slice modulation slots.
- MIDI export for common chop patterns.
- Stereo source preservation.

---

## Open Product Questions

- Whether the first release should stay at one bank or include multiple banks.
- Whether waveform-click audition should trigger only when no MIDI note is held or always play.
- Whether start-marker plus end-offset remains sufficient, or explicit start/end regions are needed.
