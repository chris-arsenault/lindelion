# 0008 — Capture-first voice-to-MIDI analysis (Glirdir)

- Status: Accepted
- Date: 2026-05-23

## Context

Real-time voice-to-MIDI must commit to pitch decisions before the syllable or note has fully developed. Vibrato, scoops, weak attacks, and momentary confidence drops produce wrong notes when the decision is made too early. Glirdir's product thesis is to defer that decision until the phrase is complete and the global pitch contour is available.

## Decision

Glirdir captures the full phrase to a scratchpad audio buffer over a fixed bar window, then runs analysis on the completed buffer to emit a MIDI clip for drag and export. The audio thread captures samples; analysis runs on a worker. The plugin produces a clean clip artifact; the host owns sequencing after export.

## Alternatives considered

- **Streaming voice-to-MIDI.** Loses the global context needed to resolve vibrato, scoops, weak attacks, and momentary confidence drops. The product becomes a real-time MIDI controller, which is a different category.
- **Hybrid streaming-plus-correction.** Significantly more complex with no clear product win at v1. Revisit if a real-time scratchpad mode is added later.
- **Multi-take buffering.** Adds session-management complexity. Single-shot songwriting scratchpad is the v1 product scope.

## Consequences

- Glirdir's audio-thread footprint is minimal: capture only. No live MIDI stream is emitted while recording.
- Analysis runs off-thread and writes back to the controller through typed messages.
- The MIDI clip is regenerated whenever marker edits or quantize settings change, without re-recording the audio (the scratchpad is preserved as the source of truth).
- Shared `lindelion-capture`, `lindelion-pitch-detect`, `lindelion-onset-detect`, `lindelion-phrase-analysis`, and `lindelion-midi` carry the host-neutral work.
