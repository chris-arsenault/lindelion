# Lindelion

Lindelion is a Rust workspace for Ahara audio plugins. The current implemented plugin is **Ahara Resonator Synth**, a polyphonic physical-modeling instrument that feeds sample excitations into modal and waveguide resonators and exposes a DAW-loadable VST3 entry point.

The runtime path, DSP tests, state roundtrip, VST3 shell, native macOS editor, and macOS bundle automation are in place.

## Contents

- [Local Development](#local-development)
- [Build The Resonator VST3](#build-the-resonator-vst3)
- [Architecture Overview](#architecture-overview)
- [Workspace Layout](#workspace-layout)
- [Documentation](#documentation)

## Local Development

Prerequisites:

- Rust stable
- `aarch64-apple-darwin` Rust target for macOS VST3 type-checks
- macOS with Xcode Command Line Tools for actual `.vst3` linking and signing

Run the same local check path expected before commit:

```bash
make ci
```

Useful focused commands:

```bash
cargo run -p xtask -- check
cargo check -p resonator-synth --target aarch64-apple-darwin
cargo build -p resonator-synth --release
```

## Build The Resonator VST3

On macOS:

```bash
make build
```

`make build` creates `~/.lindelion-cache`, uses `~/.lindelion-cache/target` as Cargo's local build directory with incremental compilation enabled, stages the bundle under `~/.lindelion-cache/bundles`, and installs the final VST3 into the system VST3 folder:

```text
/Library/Audio/Plug-Ins/VST3/Ahara/Ahara Resonator Synth.vst3
```

Enable Ableton's VST3 system folders, then restart or rescan Ableton after rebuilding.

Linux can type-check the macOS target, but it cannot produce the final macOS dylib without an Apple SDK and Darwin linker.

## Architecture Overview

The project keeps host ABI code separate from plugin/DSP logic:

- `ahara-plugin-shell` defines the shared process, event, parameter, and state boundary.
- `resonator-synth` owns patch serialization, realtime processor state, voices, resonators, modulation, and VST3 adaptation.
- `vst3_entry.rs` contains the VST3 COM factory, processor, controller, state stream, MIDI event input, and stereo output binding.
- `xtask` owns repeatable checks and macOS `.vst3` bundle layout, `moduleinfo.json`, and signing automation.

The main audio-thread contract is no heap allocation during note handling and rendering. See [docs/performance.md](docs/performance.md).

## Workspace Layout

| Path | Purpose |
|------|---------|
| `crates/ahara-plugin-shell` | Shared plugin host boundary, parameters, process context, MIDI/control events, and state container. |
| `crates/ahara-dsp-utils` | Shared DSP math, filters, delay/interpolation, smoothing, envelope, and analysis helpers. |
| `crates/ahara-sample-library` | Sample identity, metadata, and library resolution types. |
| `crates/ahara-ui` | UI model types kept separate from the plugin runtime. |
| `crates/ahara-onset-detect` | Slicer onset detection interfaces and initial detectors. |
| `crates/ahara-psola` | Pitch analysis and PSOLA boundary types. |
| `plugins/resonator-synth` | Resonator synth patch model, DSP runtime, VST3 adapter, and plugin tests. |
| `plugins/slicer` | Melodic slicer patch model and descriptor. |
| `xtask` | Repository automation for checks and macOS VST3 bundle creation. |
| `docs` | Focused development, performance, and build documentation. |

## Documentation

- [Agent guide](CLAUDE.md)
- [Audio performance contract](docs/performance.md)
- [macOS VST3 build instructions](docs/macos-vst3-build.md)

## CI

The repo uses the shared Ahara CI workflow through `.github/workflows/ci.yml` and declares its Rust-only stack in `platform.yml`. A separate macOS workflow builds the VST3 bundle artifact on Apple-hosted runners.

## License

This repository is unlicensed/source-available. See [LICENSE](LICENSE).
