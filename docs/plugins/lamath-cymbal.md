# Lamath Cymbal - Current Implementation Spec

**Name:** Lamath Cymbal
**Target:** VST3 instrument bundle on the Lamath-family build paths; Linux validates the library and DSP tests.
**Status:** Extracted shared-body idiophone/cymbal product with one excitation slot, seven host parameters, and a sparse Vizia editor.

---

## 1. Product Boundary

Lamath Cymbal is the focused extraction of Lamath's shared-body idiophone / mesh cymbal model. It keeps the validated cymbal DSP and discards the original Lamath internal routing around dual resonators, waveguide selection, modulation, streamed excitation, and sidechain input.

The reusable physical model lives in `lindelion-idiophone`. The product crate owns patch serialization, the seven-parameter host surface, MIDI strike/damp policy, excitation-slot state, VST3 entry points, and editor plumbing.

Non-goals in this product:

- no dual resonator A/B design;
- no modal/tube/string selector;
- no sidechain or streamed live excitation;
- no Lamath modulation matrix;
- no multi-slot articulation system.

---

## 2. Signal Path

```mermaid
flowchart LR
    MIDI[MIDI Note On] --> POL[Strike Or Damp Policy]
    SLOT[Built-in or loaded excitation] --> INJ[Preallocated injector pool]
    POL --> INJ
    INJ --> MESH[MeshResonator]
    MESH --> ENERGY[Energy follower]
    ENERGY -.-> MESH
    MESH --> GAIN[Output gain]
    GAIN --> LIMIT[Soft limit]
    LIMIT --> OUT[Stereo out]
```

Ordinary note-ons strike the persistent cymbal body. MIDI notes C-2 through B-2 (notes 0 through 11) damp the body with a 60 ms choke ramp and clear the ring when the choke reaches silence.

The processor keeps a fixed 16-injector pool so repeated strikes can overlap without allocating on the audio thread.

---

## 3. Patch And Parameters

The patch stores seven sound controls plus an optional loaded excitation sample:

| Host parameter | Patch field | Range | Default |
| ---- | ---- | ---- | ---- |
| `Material` | `material` | `0..1` | `0.65` |
| `Size` | `size` | `0..1` | `0.74` |
| `Damping` | `damping` | `0..1` | `0.28` |
| `Tension` | `tension` | `0..1` | `0.55` |
| `Strike` | `strike_position` | `0..1` | `0.42` |
| `Spread` | `pickup_spread` | `0..1` | `0.42` |
| `Output` | `output_gain_db` | `-24..12 dB` | `-3.0 dB` |

The editor derives its knob list from the same parameter metadata used by the VST3 controller. The optional sample is stored as a shared `SampleReference`; if it is absent or resolves to an empty buffer, the built-in excitation is used.

---

## 4. UI And Excitation Slot

The Vizia editor is deliberately sparse:

- one audio-file excitation slot with built-in fallback;
- drag-and-drop and file-browser assignment through shared audio-file slot components;
- waveform/slot state provided by shared UI services;
- seven knobs for the complete host parameter surface.

The slot is a cymbal excitation source, not a sampled-instrument voice. Played pitch still affects the mesh configuration/strike behavior, while the resonating body owns the audible decay.

---

## 5. VST3 And State

`lamath-cymbal` builds as both `cdylib` and `rlib`. The default `vst3-entry` feature exports the VST3 factory for DAW bundles. Consumers such as the Lamath render catalog can depend on the library with `default-features = false` to call the processor without exporting duplicate VST3 entry points.

Patch state is versioned TOML through the product's `patch_io` adapter and shared shell state helpers.

---

## 6. Validation

`make ci` covers the fast path, including:

- patch and VST3 state roundtrips;
- VST3 factory/editor/bus/parameter behavior;
- no-allocation MIDI strike processing;
- finite and bounded output;
- loaded excitation behavior;
- retune/restrike behavior;
- damp key behavior;
- material, size, damping, tension, strike, spread, and output-gain axes.

`make test-integration` covers heavier cymbal sweeps, including sustained ring behavior and finite/bounded/audible timbre sweeps.

The Lamath-family review renderer exercises the same processor through the historical `make render-lamath-audio` invocation for mesh/cymbal catalog cases.
