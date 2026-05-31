# Speech Coach VST — Placeholder Plan

> **Status: placeholder.** Enough to pick up on its own branch. Not a full implementation plan.

## Purpose
A standalone **speech-coaching VST3** that measures *delivery* (not audio quality) and gives
feedback: speaking rate (words/syllables per minute), cadence/rhythm, pitch dynamism
(flat ↔ animated), pause structure, and clarity. Optionally compares against a vocal reference.
Ports hot-mic's `Speech-Coach.md` + `Vocal-Reference.md`. Audio passes through unchanged.

## Scope
- Passthrough VST3 with a coaching UI + a session summary.
- **Cadence / rate:** syllable-nuclei detection (envelope-peak rate, the proxy already used to
  characterize the fixtures) → syllables/min and an estimated words/min; pause detection.
- **Pitch dynamism:** f0 variability in semitones (flat vs animated) over a window.
- **Clarity/articulation cues** derived from the analysis signals (voicing ratio, onset sharpness).
- Optional **vocal-reference** comparison (target rate/dynamism band).

## Reuse / dependencies
- `lindelion-speech-signals` `SignalAnalyzer` (voicing/onset) + `lindelion-pitch-detect` (f0).
- **The fast/slow/flat/animated fixtures already sourced in M6 are the test material for this** —
  `speech_fast` (3.8 syl/s), `speech_slow` (2.8), `speech_flat` (pitch-std 1.1),
  `speech_animated` (7.4): the coach's rate/dynamism estimators should rank these correctly.
- `lindelion-ui` + `lindelion-plugin-shell` + `vst3`.

## Open questions
- WPM estimation: syllable-nuclei → words needs a syllables-per-word assumption or a lexical pass;
  start with syllables/min + a configurable factor.
- Real-time running readout vs end-of-session summary (likely both).
- Reference model: fixed target bands vs a recorded reference clip.
