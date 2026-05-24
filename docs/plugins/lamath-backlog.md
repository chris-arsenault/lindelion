# Lamath - Backlog

This file tracks work that is not part of the current implemented Lamath spec in [lamath.md](lamath.md). It keeps deferred validation, product expansion, and unresolved design decisions out of the current implementation document.

---

## External Validation

- Run Steinberg validator against the macOS bundle after bundle-affecting changes.
- Confirm Ableton scans Lamath, loads it as an instrument, opens and closes the editor repeatedly, and saves/reloads project state.
- Validate the optional sidechain path in Ableton with the sidechain routed and unrouted.
- Record Apple Silicon latency and CPU numbers for the all-enabled sidechain path.
- Integrate a validator runner into `xtask` so bundle build validation can fail on conformance regressions when the Steinberg SDK tools are available.

---

## Resonator And DSP Extensions

- Add a bidirectional or two-port waveguide for more accurate tube behavior, including closed-end reflection and clarinet-like response.
- Add a plate or membrane resonator model as a third resonator slot type.
- Add a banded waveguide model for bowed or glass-like timbres.
- Add cross-coupling or sympathetic-resonance routing where one resonator output can partially feed another resonator input.
- Add per-voice stereo placement for natural ensemble spread.
- Add microtuning support with Scala or `.tun` import if a product use case needs it.

---

## Product And Storage Decisions

- Add recommended default mode counts per modal template while preserving user override.
- Decide whether sample ingest should normalize stored excitations to mono 48 kHz FLAC.
- Add an explicit UI workflow for live versus offline render quality settings.

---

## UI And Workflow Polish

- Expand user-facing feedback for audio-created notes, sidechain input state, and voice ownership if Ableton validation shows ambiguity during normal use.
- Revisit realtime pitch tracker quality only if Apple Silicon host validation shows the current shared detector is the quality bottleneck.
