# lamath-stringed

Lamath Stringed is the extracted Lamath-family VST3 instrument for the picked/bowed string model.

## Current Behavior

- One monophonic string voice with selectable driver: none, pick, or bow.
- Selectable body: disabled, guitar, or violin.
- Eight articulation slots with built-in fallbacks and shared drag-and-drop/file-browser loading.
- C-2 + n key switches select articulation slot n without triggering a played note.
- Seven host parameters: brightness, damping, stiffness, strike, pickup, body mix, and output.
- Model switches for body contact, bow drive, and tension.
- No dual resonators, sidechain, streamed excitation, modulation, or family selector.

## Validation

`make ci` covers the fast unit/VST3/realtime path. `make test-integration` runs heavier string sweeps. The historical Lamath review catalog invokes the same processor for string cases through `make render-lamath-audio`.

## Links

| Topic | Link |
| ---- | ---- |
| Current implementation spec | [../../docs/plugins/lamath-stringed.md](../../docs/plugins/lamath-stringed.md) |
| Lamath modal spec | [../../docs/plugins/lamath.md](../../docs/plugins/lamath.md) |
| Workspace docs | [../../docs/README.md](../../docs/README.md) |
| Architecture | [../../docs/architecture.md](../../docs/architecture.md) |
| Audio performance contract | [../../docs/performance.md](../../docs/performance.md) |
| macOS VST3 build | [../../docs/macos-vst3-build.md](../../docs/macos-vst3-build.md) |
| Workspace README | [../../README.md](../../README.md) |

## License

Source-available/unlicensed - see [../../LICENSE](../../LICENSE).
