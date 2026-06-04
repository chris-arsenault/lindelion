# lamath

*Sindarin **Lamath**: "echo" or "ringing of voices." Six letters, pronounced LAH-math; paired phonetically with Glirdir.*

Lamath is Lindelion's dual modal resonator VST3 instrument. It combines sample-slot excitation, live excitation, optional sidechain note/expression input, and the retained surrounding layer on the same instrument.

## Current Behavior

- MIDI-only patches preserve the original resonator workflow: MIDI creates voices, sample slots excite resonators, and the output is rendered as a stereo VST3 instrument.
- Optional sidechain audio can create and release notes, or run alongside MIDI in a mixed MIDI/audio mode.
- Sidechain audio can drive expression and live excitation. Expression uses shared `lindelion-audio-expression` analysis; Lamath owns voice allocation, MIDI/audio ownership policy, sidechain bus policy, and continuous/note-latched excitation routing.
- Live excitation is patch-configurable as `Off`, `Continuous`, `NoteLatched`, or `ContinuousAndNoteLatched`.
- Waveguide string, reed tube, and mesh/cymbal models are now separate products: `lamath-stringed`, `lamath-tube`, and `lamath-cymbal`.
- The offline Lamath-family review render catalog produces deterministic WAVs for Lamath modal plus the extracted Cymbal, Tube, and Stringed processors, compressed MP3 previews, and a local review UI for per-file/category feedback. See the [spec](../../docs/plugins/lamath.md#8-review-render-catalog) and [development docs](../../docs/development.md#review-audio).
- The realtime path is covered by no-allocation tests for audio note creation, continuous excitation, note-latched excitation, and mixed MIDI/audio mode.

## Validation Status

Linux workspace validation and Lamath realtime/no-allocation tests pass. The macOS VST3 bundle path exists, but Steinberg validator and Ableton scan/load validation must still be run on macOS with the optional sidechain routed and unrouted.

## Links

| Topic | Link |
| ---- | ---- |
| Current implementation spec | [../../docs/plugins/lamath.md](../../docs/plugins/lamath.md) |
| Backlog | [../../docs/plugins/lamath-backlog.md](../../docs/plugins/lamath-backlog.md) |
| Workspace docs | [../../docs/README.md](../../docs/README.md) |
| Architecture | [../../docs/architecture.md](../../docs/architecture.md) |
| Audio performance contract | [../../docs/performance.md](../../docs/performance.md) |
| macOS VST3 build | [../../docs/macos-vst3-build.md](../../docs/macos-vst3-build.md) |
| Workspace README | [../../README.md](../../README.md) |

## License

Source-available/unlicensed - see [../../LICENSE](../../LICENSE).
