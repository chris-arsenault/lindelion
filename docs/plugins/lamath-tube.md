# Lamath Tube - Current Implementation Spec

**Name:** Lamath Tube
**Target:** VST3 instrument bundle on the Lamath-family build paths; Linux validates the library and DSP tests.
**Status:** Reed-driven tube product with a reference-matched two-register clarinet voice, eight articulation slots, nine host parameters, model-switch UI scaffolding, and a sparse Vizia editor.

---

## 1. Product Boundary

Lamath Tube is the focused extraction of Lamath's reed-driven wind tube model. It keeps the validated reed/tube DSP and discards the original Lamath machinery around dual resonators, waveguide selection, modulation, streamed excitation, and sidechain input.

The reusable physical model lives in `lindelion-wind` as `ReedDriver` plus `ReedTube`. The product crate owns patch serialization, the nine-parameter host surface, MIDI/key-switch policy, articulation-slot state, VST3 entry points, and editor plumbing.

The product boundary is a single monophonic wind voice: one reed, one bore, one register
mechanism. Resonator selection, sidechain/streamed excitation, the Lamath modulation matrix, and
polyphony stay in the original Lamath product.

---

## 2. Signal Path

```mermaid
flowchart LR
    MIDI[MIDI] --> KEYS[Key-switch / note policy]
    ART[Built-in or loaded articulation] --> INJ[Articulation injector]
    KEYS --> INJ
    KEYS --> GATE[Breath gate]
    INJ --> REED[ReedDriver]
    GATE --> REED
    TUBE[ReedTube feedback] --> REED
    REED --> TUBE
    TUBE --> GAIN[Output gain]
    GAIN --> LIMIT[Soft limit]
    LIMIT --> OUT[Stereo out]
```

The tube is monophonic. A note-on sets the current pitch, opens the breath gate, and injects the selected articulation into the reed driver. A note-off for the held note closes the gate. The processor oversamples the reed/tube model by 2x and returns mono wind output to both stereo channels.

---

## 3. Patch And Parameters

The patch stores nine sound controls, three applied model switches, a selected articulation, and eight articulation slots:

| Host parameter | Patch field | Range | Default |
| ---- | ---- | ---- | ---- |
| `Pressure` | `pressure` | `0..1` | `0.58` |
| `Reed` | `reed_stiffness` | `0..1` | `0.48` |
| `Embouchure` | `embouchure` | `0..1` | `0.52` |
| `Humanize` | `humanize` | `0..1` | `0.0` |
| `Register Break` | `register_break_note` | `48..96 MIDI note` | `69` |
| `Brightness` | `brightness` | `0..1` | `0.52` |
| `Damping` | `damping` | `0..1` | `0.28` |
| `Bell` | `bell` | `0..1` | `0.5` |
| `Output` | `output_gain_db` | `-24..12 dB` | `-8.0 dB` |

The model-switch patch fields are:

- `bell_enabled`;
- `bore_steepening_enabled`;
- `body_enabled`;
- `reed_radiation_enabled`;
- `clarinet_contour_enabled` (internal legacy comparator, off in the shipped model).

The editor also shows a `Reed` switch as physical-model scaffolding, but the current DSP keeps the reed enabled because this product is specifically a reed-driven tube.

`Humanize` drives independent steady-state random walks for reed pressure, embouchure, and vocal-tract/body-formant voicing. `0.0` disables variance; `1.0` reaches an intentionally unmusical boundary for auditioning.

`Register Break` sets the MIDI note where the Tube opens its register vent. Notes below the break use the sounding pitch as the bore fundamental. Notes at or above the break keep the sounding pitch but tune the bore as a third-mode pipe (sounding-to-bore ratio 2.994) and open a side-hole shunt near one third of the bore length, mimicking the clarinet register key. The vented register is a coordinated set of register-scoped mechanisms ([ADR-0049](../adr/0049-lamath-tube-register-key-voice.md)), all inert below the break:

- the vent shunt is resistive everywhere except a zero-phase notch at the played mode (the register chimney's anti-resonance), so the bore fundamental stays suppressed while the played mode keeps its oscillation margin;
- the register body color radiates from the reed's coherent source spectrum (a fourth-order h3 extraction plus an h4–h7 band window), because the vented bore's standing wave does not carry the sounding h3;
- the in-loop breath dither and shed-jet turbulence stay out of the radiated paths above the break;
- the reed aperture resonance tracks three times the sounding pitch (embouchure firming), keeping the reed's pumping gain constant up the register;
- each vented note-on applies a tongue-release overpressure transient (toward 0.75 effective pressure, 70 ms decay, ceilinged at 0.78) so attacks bloom in ~70 ms;
- the register fingering persists through release, so the vented bore rings down without retuning.

The supported vented range is concert A4–G5 within ±2.4 cents at consistent levels; concert C6 and above speaks at altissimo-grade tuning.

---

## 4. Articulations And Key Switches

Lamath Tube has eight articulation slots. Each slot has a built-in excitation and can optionally hold a loaded audio file through the shared audio-file slot-list UI. If a loaded file is absent or empty, the slot falls back to its built-in excitation.

Built-in slot names:

| Slot | Key switch | Name |
| ---- | ---- | ---- |
| 0 | C-2 | `Tongue` |
| 1 | C#-2 | `Sforzando` |
| 2 | D-2 | `Legato` |
| 3 | D#-2 | `Staccato` |
| 4 | E-2 | `Marcato` |
| 5 | F-2 | `Breath` |
| 6 | F#-2 | `Accent` |
| 7 | G-2 | `Slur` |

MIDI notes C-2 + n select slot n and do not trigger a played note. Other note-ons play the current tube voice with the selected articulation.

---

## 5. UI

The Vizia editor is deliberately sparse:

- an instrument-style tube layout;
- nine knobs for the complete host parameter surface;
- model switches for reed, bell, bore steepening, and body;
- eight articulation slots with shared drag-and-drop/file-browser assignment.

The UI does not expose Lamath's old resonator family choices, sidechain controls, live-excitation controls, or modulation controls.

---

## 6. VST3 And State

`lamath-tube` builds as both `cdylib` and `rlib`. The default `vst3-entry` feature exports the VST3 factory for DAW bundles. Consumers such as the Lamath render catalog can depend on the library with `default-features = false` to call the processor without exporting duplicate VST3 entry points.

Patch state is versioned TOML through the product's `patch_io` adapter and shared shell state helpers.

---

## 7. Validation

`make ci` covers the fast path, including:

- patch and VST3 state roundtrips;
- VST3 factory/editor/bus/parameter behavior;
- no-allocation processing;
- default-note audibility;
- loaded excitation behavior;
- switch materiality for the applied model switches;
- sustain/release behavior;
- articulation key-switch phrasing;
- tuning/register behavior;
- onset and brightness-axis behavior.

`make test-integration` covers heavier tube processor sweeps. Shared `lindelion-wind` tests cover the reed/tube kernel's tuning, register, and extreme-drive behavior.

The Lamath-family review renderer exercises the same processor through the historical `make render-lamath-audio` invocation for tube catalog cases.
