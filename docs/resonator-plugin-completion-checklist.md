# Resonator Plugin Completion Checklist

This is the implementation checklist for the resonator plugin. Keep it honest: checked items
must have code and verification, not just a placeholder UI control.

## Implemented

- [x] Full host parameter surface has stable IDs and component/controller tests.
- [x] Parameter updates mutate the patch and update active voices where the parameter is live.
- [x] Live output, loop-gain, filter, pitch-bend, and saturation changes are smoothed on active voices.
- [x] Structural resonator/modulation changes update future voices without killing active notes.
- [x] DSP render, automation stress, sample-rate/buffer-size, offline, and no-allocation tests cover the audio path.
- [x] Patch TOML save/load preserves sample references.
- [x] File-backed sample library can ingest, index, resolve, recover moved files by hash, and create previews.
- [x] Runtime can load sample-backed excitation slots from a `SampleLibrary`.
- [x] Runtime can restore absolute `last_known_path` sample references from DAW state without audio-thread allocation.
- [x] macOS VST3 bundle layout, signing, install path, and build docs are in place.
- [x] Native Vizia editor opens on macOS and uses real VST3 parameters for its visible controls.
- [x] Editor excitation slots are populated from patch state and selection commands are resolved through the shared UI model.
- [x] Editor visible controls poll controller values so host-side parameter changes can update the open view.

## Remaining Host/UI Integration

- [ ] Patch load/save/export buttons need real host-safe file actions and state transfer to the processor.
- [ ] Sample library browser needs actual file/list interaction, ingest, waveform previews, and selected-slot assignment.
- [ ] Loaded or cleared excitation slots from the editor need a controller-to-processor bridge, not just UI command state.
- [ ] Meters/scopes need processor telemetry instead of parameter-derived placeholder graphics.
- [ ] macOS validation still needs to be run on the target machine after each bundle-affecting change.
