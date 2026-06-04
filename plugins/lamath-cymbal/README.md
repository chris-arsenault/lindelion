# lamath-cymbal

Lamath Cymbal is the extracted Lamath-family VST3 instrument for the shared-body idiophone / cymbal mesh model.

## Current Behavior

- One persistent cymbal body, struck by MIDI note-ons.
- C-2 through B-2 damp the body with a short choke ramp.
- One excitation slot with built-in fallback and shared drag-and-drop/file-browser loading.
- Seven host parameters: material, size, damping, tension, strike, spread, and output.
- No dual resonators, sidechain, streamed excitation, modulation, or family selector.

## Validation

`make ci` covers the fast unit/VST3/realtime path. `make test-integration` runs heavier cymbal sweeps. The historical Lamath review catalog invokes the same processor for mesh/cymbal cases through `make render-lamath-audio`.

## Links

| Topic | Link |
| ---- | ---- |
| Current implementation spec | [../../docs/plugins/lamath-cymbal.md](../../docs/plugins/lamath-cymbal.md) |
| Lamath modal spec | [../../docs/plugins/lamath.md](../../docs/plugins/lamath.md) |
| Workspace docs | [../../docs/README.md](../../docs/README.md) |
| Architecture | [../../docs/architecture.md](../../docs/architecture.md) |
| Audio performance contract | [../../docs/performance.md](../../docs/performance.md) |
| macOS VST3 build | [../../docs/macos-vst3-build.md](../../docs/macos-vst3-build.md) |
| Workspace README | [../../README.md](../../README.md) |

## License

Source-available/unlicensed - see [../../LICENSE](../../LICENSE).
