# Agent Guide

Agent guide for sessions in the Lindelion repository.

## Read First

| Topic | Link |
| ---- | ---- |
| Workspace overview | [README.md](README.md) |
| Documentation index | [docs/README.md](docs/README.md) |
| Architecture | [docs/architecture.md](docs/architecture.md) |
| Architecture decisions | [docs/adr/README.md](docs/adr/README.md) |
| Development commands | [docs/development.md](docs/development.md) |
| Audio performance contract | [docs/performance.md](docs/performance.md) |
| macOS VST3 build | [docs/macos-vst3-build.md](docs/macos-vst3-build.md) |
| Workspace backlog | [docs/backlog.md](docs/backlog.md) |
| Changelog | [CHANGELOG.md](CHANGELOG.md) |

## Critical Rules

- Never run destructive git commands such as `git reset --hard`, `git checkout --`, or force-push without explicit user approval.
- Work on `main` by default. Do not create, switch to, or continue work on a non-main branch unless the user explicitly instructs you to use one.
- Never commit secrets, `.env` files, credentials, DAW license files, or private SDK payloads.
- Run `make ci` as the normal verification path before committing unless the user explicitly asks for a checkpoint commit.
- Do not run lower-level formatter, lint, test, package-specific, or size-lint commands as routine verification. `make ci` already applies the repository's required Rustfmt, clippy, file/function size lint, and test settings. Use narrower commands only when the user explicitly asks for them or when debugging a specific failure after `make ci` reports one.
- Keep the realtime DSP path allocation-free. New audio-thread behavior needs focused no-allocation tests (see [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md)).
- Treat required DSP algorithms as product requirements, not optional implementation details. If pitch shifting, pitch detection, onset detection, resonators, or other difficult audio algorithms behave badly, work through the algorithm and add objective audio tests; do not replace the requested behavior with a simpler design, different semantics, or a bypass unless the user explicitly approves that change.
- Follow `../ahara/CI-WORKFLOW.md` for shared CI shape, `../ahara/INTEGRATION.md` for platform metadata, and `../ahara/skills/repo-docs/SKILL.md` for repository documentation conventions.
- Do not add a plugin framework such as JUCE, nih-plug, or iPlug2 unless the user explicitly changes the architecture (see [ADR-0002](docs/adr/0002-no-plugin-framework.md)).
- Do not treat Linux cross-checks for macOS as real macOS bundle builds; final `.vst3` linking and signing need Apple tooling (see [ADR-0007](docs/adr/0007-macos-vst3-build-path.md)).

## Product Names

| Name | Meaning | Current state |
| ---- | ---- | ---- |
| Lindelion | Quenya `lindelë` + `-ion`, bearer of the art of music | Workspace/project |
| Lamath | Sindarin, "echo" or "ringing of voices" | VST3 resonator instrument with MIDI and sidechain audio inputs |
| Linnod | Sindarin measured verse unit | Melodic slicer VST3 instrument |
| Glirdir | Sindarin `glir-` + `-dir`, singer/song-bearer | VST3 sing-to-MIDI scratchpad |

## Code Map

| Path | Purpose |
| ---- | ---- |
| `crates/lindelion-plugin-shell` | Shared plugin boundary, parameters, process context, MIDI/control events, state, typed VST3 messages, patch I/O, voice allocation. |
| `crates/lindelion-dsp-utils` | DSP support: analysis, delay/interpolation, envelopes, filters, math, smoothing, saturation. |
| `crates/lindelion-test-allocator` | Counting allocator and `assert_no_allocations!` macro for realtime-path tests. |
| `crates/lindelion-capture` | Host-synced audio capture state, scratchpad audio, capture settings, sync modes. |
| `crates/lindelion-sample-library` | Sample references, loaded-audio ownership, hashing, ingest, previews, moved-file recovery. |
| `crates/lindelion-audio-expression` | Host-neutral streaming audio-note and audio-expression bridge from pitch/onset/loudness/brightness. |
| `crates/lindelion-onset-detect` | Batch and streaming onset detection, configuration, and pitch-aware onset DTOs. |
| `crates/lindelion-pitch-detect` | SwiftF0 ONNX pitch detection, streaming pitch tracking, confidence filtering, resampling. |
| `crates/lindelion-pitch-shift` | Shared formant-preserving pitch-shift analysis cache and source-filter descriptors. |
| `crates/lindelion-plugin-metadata` | Shared VST3 bundle metadata consumed by plugin factories and `xtask`. |
| `crates/lindelion-phrase-analysis` | Pitch/onset phrase orchestration, note segmentation, segmentation heuristics. |
| `crates/lindelion-midi` | Root/scale models, timing and pitch quantization, velocity mapping, MIDI clip DTOs, SMF emission. |
| `crates/lindelion-ui` | Shared UI command model, editor services, editor surface primitives, product Vizia editors. |
| `plugins/lamath` | Lamath patch model, DSP runtime, VST3 adapter, tests. |
| `plugins/linnod` | Linnod source analysis, patch model, runtime, VST3 adapter, editor bridge, and tests. |
| `plugins/glirdir` | Glirdir capture, analysis, audition, VST3 adapter, editor, drag/export, sample-library save, bundle metadata. |
| `xtask` | Workspace checks and macOS VST3 bundle automation. |

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Canonical and default local verification path; use this instead of composing separate lower-level checks. |
| `make build` | Build and install all bundleable VST3 plugins on macOS. |
| `make build PLUGIN=lamath` | Build and install only the Lamath VST3 bundle on macOS. |
| `make build PLUGIN=glirdir` | Build and install only the Glirdir VST3 bundle on macOS. |
| `make build PLUGIN=linnod` | Build and install only the Linnod VST3 bundle on macOS. |
| `make validate-vst3 PLUGIN=linnod` | Inspect and run Steinberg validator against the installed Linnod bundle on macOS. |
| `make bench` | Run the full workspace Criterion benchmark suite. |
| `make bench-smoke` | Compile workspace benches without running Criterion measurements. |
