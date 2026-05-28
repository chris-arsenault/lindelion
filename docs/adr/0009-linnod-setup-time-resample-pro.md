# ADR-0009: Linnod Setup-Time Resample Pro Rendering

## Status

Accepted

## Date

2026-05-28

## Context

Linnod plays sliced source samples as an instrument. Resample Stretch pitch shifting must preserve each slice's duration while changing pitch, and the audible quality target is clean wind-sample playback at tiny cent offsets and musical semitone shifts.

The Resample Pro algorithm performs STFT analysis, phase-aware time scaling, transient handling, and bandlimited resampling. That work allocates and operates over whole source regions. Linnod's audio thread has an allocation-free contract, and MIDI note playback should perform bounded sample reads plus normal voice processing.

Chromatic playback can imply many possible pitch ratios from one selected slice. Preparing every possible ratio has a memory and setup-time cost, while rendering ratios on demand would put the pitch shifter back on the note path.

## Decision

Linnod renders Resample Stretch variants during source or patch preparation. The plugin-local prepared cache stores complete shifted slice buffers keyed by source cache key, slice index, source range, pitch/formant ratios, playback direction, and render-config version.

MIDI note playback reads prepared buffers for non-identity Resample Stretch requests. A requested shifted variant that is not prepared produces silence rather than direct unshifted playback or another hidden pitch algorithm.

Pad mode prepares the exact variants implied by the current pad map and slice settings. Chromatic mode prepares the selected root-note variant; broader chromatic range preparation is tracked in the Linnod backlog.

## Consequences

- Resample Pro rendering stays off the realtime MIDI note path.
- Prepared-buffer playback remains allocation-free and testable with the existing realtime allocator checks.
- Missing shifted variants fail explicitly at playback instead of producing the wrong pitch.
- Chromatic Resample Stretch coverage requires a bounded prepared-variant policy before it can cover a larger keyboard range.

## Alternatives

- Render Resample Pro on MIDI note trigger. This keeps memory lower but violates the realtime boundary and makes note latency depend on slice length and pitch-shift rendering cost.
- Fall back to unshifted playback for missing variants. This keeps notes audible but lies about requested pitch and hides preparation bugs.
- Fall back to another pitch algorithm for missing variants. This keeps notes audible but changes artifact profile per note and undermines Resample Stretch quality guarantees.
- Pre-render a wide chromatic range immediately. This improves coverage but needs explicit memory/setup-time bounds and UI/product policy.
- Implement Resample Stretch as PSOLA or plain rate transposition. PSOLA is a different F0/epoch-dependent time-domain algorithm with its own overlap artifacts, and rate transposition moves formants with pitch instead of preserving fixed-duration slice semantics.
