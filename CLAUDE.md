# CLAUDE.md

Agent guide for Claude sessions in the Lindelion repository.

## Read First

| Topic | Link |
| ---- | ---- |
| Workspace overview | [README.md](README.md) |
| Documentation index | [docs/README.md](docs/README.md) |
| Architecture | [docs/architecture.md](docs/architecture.md) |
| Development commands | [docs/development.md](docs/development.md) |
| Audio performance contract | [docs/performance.md](docs/performance.md) |
| macOS VST3 build | [docs/macos-vst3-build.md](docs/macos-vst3-build.md) |

## Critical Rules

- Never run destructive git commands such as `git reset --hard`, `git checkout --`, or force-push without explicit user approval.
- Work on `main` by default. Do not create, switch to, or continue work on a non-main branch unless the user explicitly instructs you to use one.
- Never commit secrets, `.env` files, credentials, DAW license files, or private SDK payloads.
- Run `make ci` before committing unless the user explicitly asks for a checkpoint commit.
- Keep the realtime DSP path allocation-free. New audio-thread behavior needs focused no-allocation tests.
- Follow `../ahara/CI-WORKFLOW.md` for shared CI shape and `../ahara/INTEGRATION.md` for platform metadata.
- Do not add a plugin framework such as JUCE, nih-plug, or iPlug2 unless the user explicitly changes the architecture.
- Do not treat Linux `cargo check --target aarch64-apple-darwin` as a real macOS bundle build; final `.vst3` linking and signing need Apple tooling.

## Product Names

| Name | Meaning | Current state |
| ---- | ---- | ---- |
| Lindelion | Quenya `lindelë` + `-ion`, bearer of the art of music | Workspace/project |
| Lamath | Sindarin, "echo" or "ringing of voices" | Implemented VST3 instrument |
| Linnod | Sindarin measured verse unit | Melodic slicer scaffold |
| Glirdir | Sindarin `glir-` + `-dir`, singer/song-bearer | Planned sing-to-MIDI product directory |

## Code Map

| Path | Purpose |
| ---- | ---- |
| `crates/lindelion-plugin-shell` | Shared plugin boundary, parameters, process context, MIDI/control events, state, patch I/O, VST3 helpers, and voice management. |
| `crates/lindelion-dsp-utils` | Shared DSP math, filters, delay/interpolation, smoothing, envelope, and analysis helpers. |
| `crates/lindelion-sample-library` | Sample identity, metadata, hashing, previews, and library resolution. |
| `crates/lindelion-ui` | Shared UI command model, services, and Lamath Vizia editor surface. |
| `crates/lindelion-onset-detect` | Onset detection used by Linnod and Glirdir. |
| `crates/lindelion-psola` | Pitch analysis and PSOLA boundary types. |
| `plugins/lamath` | Lamath patch model, DSP runtime, VST3 adapter, and tests. |
| `plugins/linnod` | Linnod descriptor, parameters, patch model, and scaffold plugin implementation. |
| `plugins/glirdir` | Glirdir product directory. No Cargo package yet. |
| `xtask` | Workspace checks and macOS VST3 bundle automation. |

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Canonical local check path. |
| `cargo run -p xtask -- check` | Rustfmt, clippy, and workspace tests. |
| `cargo test --workspace` | Workspace tests. |
| `cargo check -p lamath` | Type-check Lamath. |
| `cargo test -p lamath --lib` | Lamath unit tests. |
| `cargo check -p linnod` | Type-check Linnod. |
| `cargo test -p linnod` | Linnod test target. |
| `make build` | Build and install the Lamath VST3 bundle on macOS. |
