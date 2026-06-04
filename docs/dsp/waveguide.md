# Extracted String And Wind Waveguide Models

The old Lamath `WaveguideResonator` documentation has been replaced by the current extracted model boundary. String and wind waveguide DSP now live in separate shared crates:

| Model | Crate | Product consumer |
| ---- | ---- | ---- |
| String waveguide, pick/bow driver, reduced body coupling | `crates/lindelion-string` | [Lamath Stringed](../plugins/lamath-stringed.md) |
| Reed driver and tube waveguide | `crates/lindelion-wind` | [Lamath Tube](../plugins/lamath-tube.md) |

Lamath itself is now a dual modal resonator and does not own waveguide string or tube DSP. The historical Lamath render catalog still exercises the extracted processors for string and tube audition cases.

## 1. Purpose

Both extracted models are physical resonators driven by short excitation/articulation signals. The excitation is not treated as pitched sample playback; pitch and sustain come from the waveguide model.

`lindelion-string` owns:

- `StringModel` - traveling-wave string loop, loop damping, stiffness dispersion, pickup/strike positions, energy-driven tension, and reduced-body radiation/coupling;
- `StringDriver` - `None`, `Pick`, and `Bow` driver modes;
- `StringBodyMode` - disabled, guitar, and violin body families.

`lindelion-wind` owns:

- `ReedDriver` - beating-reed valve driven by effort, embouchure, stiffness, feedback, and articulation excitation;
- `ReedTube` - driven wind tube with bore feedback, bell radiation, bore steepening, and optional body coloration.

Product crates own MIDI, articulation slots, patch serialization, UI controls, key-switch policy, VST3 entry points, and output staging.

## 2. Shared Techniques

The detailed technique catalog is [waveguide-techniques.md](waveguide-techniques.md). Current shared ideas include:

- allocation-free traveling-wave storage after construction;
- fractional-delay tuning using shared interpolation/delay helpers;
- calibrated loop damping with bounded feedback gain;
- finite-output clamps and denormal cleanup at loop boundaries;
- product-local oversampling in Lamath Tube and Lamath Stringed processors;
- objective tuning, stability, bounded-output, and audibility tests in the extracted crates.

## 3. String Model

`StringModel` is a half-wave string resonator. A played pitch maps to the loop delay; damping controls decay; brightness maps to loop-filter cutoff; stiffness adds dispersion; strike and pickup positions shape the mode balance.

The reduced-body model is coupled at the bridge and radiates a guitar or violin body response. Body coupling is passive and allocation-free after construction. The product-level `body_contact`, `bow_drive`, and `tension` switches in Lamath Stringed map into this model boundary.

The product-level driver selector chooses:

- `None` - no active physical driver;
- `Pick` - transient pick excitation;
- `Bow` - continuous bow/friction driver while the note gate is open.

See [Lamath Stringed](../plugins/lamath-stringed.md) for the product patch, parameters, key switches, and UI surface.

## 4. Wind Model

`ReedTube` is a driven wind resonator, not a struck tube. The reed driver terminates the bore mouth and processes articulation excitation, effort, and bore feedback. The tube then renders the pressure wave with brightness, damping, bell, bore steepening, and body-coloring controls.

The current Lamath Tube product keeps the reed enabled. Bell, bore steepening, and body switches map into the shared `ReedTubeSwitches`.

See [Lamath Tube](../plugins/lamath-tube.md) for the product patch, parameters, key switches, and UI surface.

## 5. Realtime Contract

- Construction may allocate fixed buffers; per-sample and per-block processing must not allocate.
- Processing must not perform file I/O, patch serialization, logging, UI calls, or host callbacks.
- All user-controlled parameters are clamped at the model or product boundary.
- Output must remain finite and bounded across register and drive sweeps.
- Product processors own loaded-articulation buffers before audio processing begins; the model only sees slices and scalar parameters.

## 6. Current Test Coverage

`make ci` includes fast unit coverage for the extracted crates and products:

- `lindelion-string`: finite/audible default renders, pitch tracking, body coupling, body-family behavior, dispersion, source/body balance, tension modulation, and extreme-drive boundedness.
- `lindelion-wind`: finite/audible reed-driven output, low/mid-register tuning, bell/body/bore switch behavior, effort dynamics, and extreme-drive boundedness.
- `lamath-stringed`: no-allocation processing, driver/body/switch materiality, articulation slots, loaded excitation, register behavior, and VST3 state/parameter/editor boundaries.
- `lamath-tube`: no-allocation processing, model-switch materiality, articulation slots, loaded excitation, sustain/release, register behavior, and VST3 state/parameter/editor boundaries.

`make test-integration` runs the heavier register and stability sweeps excluded from the fast unit path.

## 7. References

- Product specs: [Lamath Stringed](../plugins/lamath-stringed.md), [Lamath Tube](../plugins/lamath-tube.md).
- Technique catalog: [Waveguide resonator techniques](waveguide-techniques.md).
- Building blocks: [DelayLine](delay-line.md), [FirstOrderAllpass](allpass.md), [Biquad](biquad.md).
- ADR-0001: [Allocation-free audio thread](../adr/0001-allocation-free-audio-thread.md).
- ADR-0011: [Waveguide tube tuning correction and 2D mesh resonator](../adr/0011-waveguide-tube-tuning-and-2d-mesh.md).
- ADR-0032: [Lamath Tube is a driven wind voice](../adr/0032-lamath-tube-driven-wind-voice.md).
