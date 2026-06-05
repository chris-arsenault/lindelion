# lamath-tube

Lamath Tube is the extracted Lamath-family VST3 instrument for the reed-driven wind tube model.

## Current Behavior

- One monophonic reed-driven tube voice.
- Eight articulation slots with built-in fallbacks and shared drag-and-drop/file-browser loading.
- C-2 + n key switches select articulation slot n without triggering a played note.
- Eight host parameters: pressure, reed, embouchure, humanize, brightness, damping, bell, and output.
- Model-switch UI for reed, bell, bore steepening, and body; the reed remains enabled in the current DSP.
- No dual resonators, sidechain, streamed excitation, modulation, or family selector.

## Validation

`make ci` covers the fast unit/VST3/realtime path. `make test-integration` runs heavier tube sweeps. The historical Lamath review catalog invokes the same processor for tube cases through `make render-lamath-audio`.

## Links

| Topic | Link |
| ---- | ---- |
| Current implementation spec | [../../docs/plugins/lamath-tube.md](../../docs/plugins/lamath-tube.md) |
| Lamath modal spec | [../../docs/plugins/lamath.md](../../docs/plugins/lamath.md) |
| Workspace docs | [../../docs/README.md](../../docs/README.md) |
| Architecture | [../../docs/architecture.md](../../docs/architecture.md) |
| Audio performance contract | [../../docs/performance.md](../../docs/performance.md) |
| macOS VST3 build | [../../docs/macos-vst3-build.md](../../docs/macos-vst3-build.md) |
| Workspace README | [../../README.md](../../README.md) |

## License

Source-available/unlicensed - see [../../LICENSE](../../LICENSE).
