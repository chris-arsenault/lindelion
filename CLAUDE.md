# CLAUDE.md

Primary working guide for agents operating in the Lindelion repository.

## Critical Rules

- Never run destructive git commands such as `git reset --hard`, `git checkout --`, or force-push without explicit user approval.
- Never commit secrets, `.env` files, credentials, DAW license files, or private SDK payloads.
- Run `make ci` before committing unless the user explicitly asks for a checkpoint commit.
- Keep the realtime DSP path allocation-free. New audio-thread behavior needs focused no-allocation tests.
- Follow `../ahara/CI-WORKFLOW.md` for shared CI shape and `../ahara/INTEGRATION.md` for platform metadata.
- Do not add a plugin framework such as JUCE, nih-plug, or iPlug2 unless the user explicitly changes the architecture.
- Do not treat Linux `cargo check --target aarch64-apple-darwin` as a real macOS bundle build; final `.vst3` linking and signing need Apple tooling.

## Project Overview

Lindelion is a Rust workspace for Ahara audio plugins. The current implemented plugin is `resonator-synth`, a polyphonic physical-modeling instrument that feeds sample excitations into modal and waveguide resonators and exposes a framework-light VST3 entry point.

UI is intentionally outside the current slice. The plugin should still load in a DAW as a VST3 and render audio from MIDI input.

## Code Layout

| Path | Purpose |
|------|---------|
| `crates/ahara-plugin-shell` | Shared plugin boundary, parameters, process context, MIDI/control events, and state container. |
| `crates/ahara-dsp-utils` | Shared DSP math, filters, delay/interpolation, smoothing, envelope, and analysis helpers. |
| `crates/ahara-sample-library` | Sample identity, metadata, and library resolution types. |
| `crates/ahara-ui` | UI model types kept separate from the plugin runtime. |
| `crates/ahara-onset-detect` | Slicer onset detection interfaces and initial detectors. |
| `crates/ahara-psola` | Pitch analysis and PSOLA boundary types. |
| `plugins/resonator-synth` | Resonator synth patch model, DSP runtime, VST3 adapter, and plugin tests. |
| `plugins/slicer` | Melodic slicer patch model and descriptor. |
| `xtask` | Repository automation for checks and macOS VST3 bundle creation. |
| `docs` | Focused development, performance, and build documentation. |
| `.github/workflows` | Shared Ahara CI caller and macOS VST3 bundle workflow. |

## Architecture Patterns

- The audio plugin core uses `ahara-plugin-shell`; VST3-specific COM and host ABI code stays in `plugins/resonator-synth/src/vst3_entry.rs`.
- DSP code owns preallocated runtime structures. Note-on, rendering, and event handling must avoid heap allocation after setup.
- Patch serialization uses TOML payloads inside `PluginState` for debuggable state roundtrips.
- The VST3 path exposes a component, edit controller, parameter metadata, MIDI event input, stereo output, state streams, and platform exports.
- `xtask` is the canonical local automation entry point. Add repo commands there when they need Rust logic or cross-platform behavior.

## Development Commands

| Command | Purpose |
|---------|---------|
| `make ci` | Run the local Ahara-standard check path. |
| `cargo run -p xtask -- check` | Run rustfmt, release clippy with cognitive complexity, and all workspace tests. |
| `cargo check -p resonator-synth --target aarch64-apple-darwin` | Type-check the macOS VST3 target from any host with the Rust target installed. |
| `cargo run -p xtask -- bundle resonator-synth --target aarch64-apple-darwin` | Build and bundle the VST3 on macOS. |
| `cargo build -p resonator-synth --release` | Build the local platform cdylib/rlib. |

## Verification Notes

- Baseline before commit: `make ci`.
- For VST3 ABI changes, also run `cargo check -p resonator-synth --target aarch64-apple-darwin`.
- For actual DAW-loadable macOS artifacts, build on macOS or in the macOS GitHub Action; Linux lacks the Apple linker and SDK.
- Run Steinberg `validator` against `target/bundles/Ahara Resonator Synth.vst3` when the SDK is available.
