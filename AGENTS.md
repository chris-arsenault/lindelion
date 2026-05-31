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
- The `make ci` unit path must stay fast and deterministic: every test in the default `cargo test --workspace` run is in-memory, contention-free, and finishes in milliseconds. Unit tests must not write files, spawn threads, sleep, or read wall-clock time — those contend for shared resources and make timing non-deterministic. Gate anything that does behind the per-crate `integration-tests` feature with `#[cfg_attr(not(feature = "integration-tests"), ignore = "…")]` and run it via `make test-integration`. This same gate also holds the multi-second DSP fidelity/stability/tuning sweeps. Doc-data generators (plot/CSV/baseline writers) are `#[ignore]`d and run via `make docs`. Neural-network model-integration tests are `#[ignore]`d and run via `make test-models`. Keep a cheap, in-memory unit test for behavior wherever the heavier test is the only coverage.
- Treat required DSP algorithms as product requirements, not optional implementation details. If pitch shifting, pitch detection, onset detection, resonators, or other difficult audio algorithms behave badly, work through the algorithm and add objective audio tests; do not replace the requested behavior with a simpler design, different semantics, or a bypass unless the user explicitly approves that change.
- Put temporary implementation plans intended for immediate consumption at the repository root. Do not file them in `docs/`, backlog files, or index/link surfaces unless the user explicitly asks for durable documentation.
- Follow `../ahara/CI-WORKFLOW.md` for shared CI shape, `../ahara/INTEGRATION.md` for platform metadata, and `../ahara/skills/repo-docs/SKILL.md` for repository documentation conventions.
- Do not add a plugin framework such as JUCE, nih-plug, or iPlug2 unless the user explicitly changes the architecture (see [ADR-0002](docs/adr/0002-no-plugin-framework.md)).
- Do not treat Linux cross-checks for macOS as real macOS bundle builds; final `.vst3` linking and signing need Apple tooling (see [ADR-0007](docs/adr/0007-macos-vst3-build-path.md)).
- The speech effect port under `speech/` targets spoken-word clarity and intelligibility, not musicality. Tune its defaults, thresholds, band centers, and tests for speech, and keep speech-specific tuning out of the shared `crates/` foundations (see [ADR-0012](docs/adr/0012-speech-effect-port-shared-workspace.md)).
- Keep the effect core host-agnostic: `lindelion-effect` and the `speech/` effect crates depend only on the pure-DSP crates (`lindelion-dsp-utils`, `lindelion-pitch-detect`, `lindelion-onset-detect`), never on `lindelion-plugin-shell`, `vst3`, or `lindelion-ui`. Do not add VST3 entry points, app shells, or a fixed signal flow to ported effects in this phase (see [ADR-0013](docs/adr/0013-host-agnostic-effect-core.md)).
- Reuse existing Lindelion DSP where it overlaps, re-tuned for speech; build new only where the existing primitive is musical or absent (envelope follower, saturation, standalone STFT). See [HOTMIC-PORT-PLAN.md](HOTMIC-PORT-PLAN.md).
- The Galad Windows host (`host/`) is a Windows-only standalone application and the *host* side of VST3 — distinct from the plugins, which are the guest side. Keep it target-gated so it never enters the Linux/macOS `make ci` path, and do not pull its Windows-only deps (WASAPI, `egui`) into shared crates. ADR-0007's macOS-only bundle path governs plugin `.vst3` bundles only, not the host (see [ADR-0022](docs/adr/0022-windows-vst3-host.md)).
- The new VSTs (Cenedril, Calóma, Speech Coach) target **Windows only**, with a Windows VST3 build path and an **egui** editor — never `lindelion-ui` (macOS-only). Do not plan them macOS-first or defer the Windows build/editor as a "follow-on"; it is foundational. ADR-0007 (macOS bundles) governs only the existing instruments (Lamath/Linnod/Glirdir); the new VSTs follow [ADR-0023](docs/adr/0023-new-vsts-windows-only.md).

## Product Names

| Name | Meaning | Current state |
| ---- | ---- | ---- |
| Lindelion | Quenya `lindelë` + `-ion`, bearer of the art of music | Workspace/project |
| Lamath | Sindarin, "echo" or "ringing of voices" | VST3 resonator instrument with MIDI and sidechain audio inputs |
| Linnod | Sindarin measured verse unit | Melodic slicer VST3 instrument |
| Calóma | Quenya `cala` (bright/clear) + `óma` (voice), "clear voice" | Planned single VST3 packaging the speech-effect chain (see [ADR-0020](docs/adr/0020-caloma-speech-vst-packaging.md)) |
| Glirdir | Sindarin `glir-` + `-dir`, singer/song-bearer | VST3 sing-to-MIDI scratchpad |
| Galad | Sindarin, "radiance/light" (working name) | Planned Windows realtime VST3 host application (mic → arbitrary VST3 chain → output); see [ADR-0022](docs/adr/0022-windows-vst3-host.md), plan at `GALAD-HOST-PLAN.md` |
| Cenedril | Quenya/Sindarin "mirror, looking-glass" (working name) | Planned Windows-only passthrough Visualizer VST3 (spectrogram, level/LUFS meters, analysis readouts); see [ADR-0023](docs/adr/0023-new-vsts-windows-only.md), plan at `CENEDRIL-VST-PLAN.md` |

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
| `crates/lindelion-effect` | Host-agnostic effect-processor trait and neutral parameter/state/latency primitives (distinct from `plugin-shell`'s VST-coupled `AudioPlugin`). |
| `crates/lindelion-fidelity` | Shared general-signal audio-fidelity test harness for effect crates. |
| `speech/` | Speech-effect port of `hot-mic`: per-effect crates plus `speech/signals` analysis-signal derivation, tuned for spoken word. |
| `plugins/lamath` | Lamath patch model, DSP runtime, VST3 adapter, tests. |
| `plugins/linnod` | Linnod source analysis, patch model, runtime, VST3 adapter, editor bridge, and tests. |
| `plugins/glirdir` | Glirdir capture, analysis, audition, VST3 adapter, editor, drag/export, sample-library save, bundle metadata. |
| `host/` | (Reserved, not yet a workspace member) Galad — standalone Windows realtime VST3 host application: WASAPI audio I/O, host-side VST3 protocol, device management, egui UI. Target-gated; excluded from `make ci`. See [ADR-0022](docs/adr/0022-windows-vst3-host.md). |
| `plugins/visualizer` | (Reserved, not yet a workspace member) Cenedril — Windows-only passthrough Visualizer VST3 (spectrogram, meters, analysis readouts), egui editor. See [ADR-0023](docs/adr/0023-new-vsts-windows-only.md). |
| `xtask` | Workspace checks and macOS VST3 bundle automation. |

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Canonical and default local verification path; use this instead of composing separate lower-level checks. Runs only the fast, in-memory unit suite. |
| `make test-integration` | Run the heavier suite excluded from `make ci`: multi-second DSP fidelity/stability/tuning sweeps plus filesystem- and thread-touching tests (per-crate `integration-tests` feature). |
| `make test-models` | Run the `#[ignore]`d neural-network model-integration tests. |
| `make build` | Build and install all bundleable VST3 plugins on macOS. |
| `make build PLUGIN=lamath` | Build and install only the Lamath VST3 bundle on macOS. |
| `make build PLUGIN=glirdir` | Build and install only the Glirdir VST3 bundle on macOS. |
| `make build PLUGIN=linnod` | Build and install only the Linnod VST3 bundle on macOS. |
| `make validate-vst3 PLUGIN=linnod` | Inspect and run Steinberg validator against the installed Linnod bundle on macOS. |
| `make bench` | Run the full workspace Criterion benchmark suite. |
| `make bench-smoke` | Compile workspace benches without running Criterion measurements. |
| `make test-models` | Run the heavy NN model-integration tests (ONNX Runtime inference); excluded from `make ci`. |
