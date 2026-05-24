# Linnod - Current Implementation Spec

**Name:** Linnod
**Name etymology:** Sindarin, a measured verse unit: a half-line of 4+3 syllables forming a distinct portion of a larger song.
**Target:** macOS VST3 instrument, Apple Silicon primary.
**Status:** Cargo scaffold and patch model implemented. Slicing DSP, editor, bundle automation, and host validation are tracked in [linnod-backlog.md](linnod-backlog.md).

This document describes the behavior implemented in the workspace today. The planned melodic slicer product spec and remaining implementation work live in [linnod-backlog.md](linnod-backlog.md).

---

## 1. Product Intent

Linnod is the planned Lindelion melodic sample slicer. The intended product centers monophonic melodic source material such as wind, voice, and bowed strings, then turns detected slices into MIDI-triggered rhythmic chops.

The implemented code is currently a silent instrument scaffold with the product identity, parameter surface, and serializable patch model needed to continue implementation.

---

## 2. Current Runtime Boundary

Current crate: `plugins/linnod`.

Implemented in `src/lib.rs`:

- `DESCRIPTOR` for a VST3 instrument named `Linnod`;
- a minimal host parameter list:
  - master gain;
  - detection sensitivity;
  - tuning reference;
- `LinnodPatch` with source sample reference, detection config, markers, slice parameters, tuning config, trigger mode, active chromatic pad, and pad map;
- default 16-slice patch layout;
- per-slice parameter model for offsets, pitch, gain, pan, reverse, playback mode, ADSR, filter cutoff, and cached pitch analysis;
- pad assignment model;
- tuning scale and root enums;
- `AudioPlugin` implementation that exposes descriptor/parameters, stores setup, clears output, and returns empty state.

Current runtime behavior:

- The plugin is silent.
- No sample loading, marker editing, detection, PSOLA playback, editor surface, patch serialization, or bundle automation is implemented in the Linnod plugin crate.
- Shared crates that support the planned product already exist: `lindelion-onset-detect`, `lindelion-psola`, `lindelion-plugin-shell`, `lindelion-sample-library`, `lindelion-dsp-utils`, and `lindelion-ui`.

---

## 3. Current Patch Model

`LinnodPatch` contains:

- patch name;
- optional shared-library source sample reference;
- onset detection config;
- marker list;
- 16 default slice parameter entries;
- tuning reference, scale, and root;
- trigger mode;
- active chromatic pad;
- 16 default pad assignments mapped to MIDI notes 36-51.

Each `SliceParams` entry contains:

- name;
- start and end offsets;
- pitch shift in semitones and cents;
- gain and pan;
- reverse flag;
- playback mode: one-shot, gated, or looped;
- ADSR values;
- one-pole filter cutoff;
- cached pitch analysis placeholder.

---

## 4. Current Shared-Crate Position

Linnod's planned implementation is intended to reuse:

- `lindelion-plugin-shell` for VST3 integration, parameter surfaces, MIDI normalization, process context, and state helpers;
- `lindelion-sample-library` for sample ingest, hashing, metadata, and shared library resolution;
- `lindelion-dsp-utils` for filters, smoothing, interpolation, envelopes, and math helpers;
- `lindelion-ui` for shared Vizia controls and product editor contracts;
- `lindelion-onset-detect` for reusable onset algorithms;
- `lindelion-psola` for pitch-analysis boundaries and formant-preserving pitch shifting.

The current plugin crate has not yet wired those shared capabilities into a working instrument.

---

## Appendix A - Glossary

- **Slice:** a planned playback region derived from a source sample marker.
- **Marker:** a planned start point in a source sample.
- **PSOLA:** pitch-synchronous overlap-add, planned for formant-preserving pitch shifts of monophonic pitched material.
- **Pad mode:** planned trigger mode where pads play assigned slices at original pitch.
- **Chromatic mode:** planned trigger mode where one selected slice plays across the keyboard.
