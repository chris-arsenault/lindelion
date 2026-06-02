# Development

Local development uses stable Rust and Makefile entrypoints for repeatable checks.

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Run the canonical local check path: workspace checks, a macOS-target workspace check on macOS hosts, and bench compile smoke tests. |
| `make build` | Build, stage, and install all bundleable macOS VST3 plugins on macOS. |
| `make build PLUGIN=lamath` | Build, stage, and install only Lamath on macOS. |
| `make build PLUGIN=glirdir` | Build, stage, and install only Glirdir on macOS. |
| `make build PLUGIN=linnod` | Build, stage, and install only Linnod on macOS. |
| `make inspect-vst3` | Inspect the installed default VST3 bundle on macOS. |
| `make inspect-vst3 PLUGIN=glirdir` | Inspect the installed Glirdir VST3 bundle on macOS. |
| `make inspect-vst3 PLUGIN=linnod` | Inspect the installed Linnod VST3 bundle on macOS. |
| `make validate-vst3 PLUGIN=linnod` | Run the shared validator wrapper against the installed Linnod bundle. |
| `make host-windows-check` | Cross-compile the Galad Windows host (`galad`) for `x86_64-pc-windows-msvc` via cargo-xwin. |
| `make build-windows` | Cross-compile and stage all Windows-only VST3 plugins (`$(WINDOWS_PLUGINS)`) for `x86_64-pc-windows-msvc` via cargo-xwin. |
| `make build-windows PLUGIN=cenedril` | Cross-compile and stage a single Windows-only plugin (mirrors macOS `make build PLUGIN=`). |
| `make test-integration` | Run the heavy suite excluded from `make ci`: multi-second DSP fidelity/stability/tuning renders plus filesystem/thread-touching tests (per-crate `integration-tests` feature). |
| `make test-models` | Run the `#[ignore]`d neural-network model-integration tests (ONNX Runtime). |
| `make docs` | Run the `#[ignore]`d doc-data generators (plot/CSV/baseline writers). |

## Testing

`make ci` runs **only the fast unit suite** (the default `cargo test --workspace`). It must stay fast and deterministic, so heavier tests live in separate, explicitly-invoked suites. There are four buckets:

| Bucket | Runs in | Gated by | What belongs here |
| ---- | ---- | ---- | ---- |
| Fast unit | `make ci` | (default) | Pure, in-memory behaviour tests **and the audio-hygiene invariant guards** — allocation-free ([ADR-0001](adr/0001-allocation-free-audio-thread.md)) checks and finite/NaN/bounded-output checks — even though a few cost ~1–3s because they exercise the real audio path under the counting allocator. |
| Heavy DSP | `make test-integration` | `#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]` | Multi-second DSP **fidelity/stability/tuning renders and sweeps**: range/matrix/extreme-drive stability, decay-across-range, tuning matrices, timbre and A/B characterizations, the pitch-shift fidelity battery, fixture renders. Also any test that writes files, spawns threads, sleeps, or reads wall-clock. |
| NN models | `make test-models` | `#[ignore]` | ONNX Runtime model-integration tests (they saturate the CPU). |
| Doc data | `make docs` | `#[ignore]` | plot/CSV/baseline generators. |

**The decision rule when a test is slow.** Ask *why* it is slow:

- It is a **render / sweep** — slow because it processes a lot of audio to measure fidelity, stability, or tuning across a range. → **Move it** to `make test-integration` with the gate above.
- It is an **invariant guard** — slow because it must run the real audio path, but what it asserts is an always-true property ("allocates nothing", "stays finite", "output bounded"). → **Keep it in `make ci`.** A regression in audio hygiene must be caught on every commit, not only in the heavier suite. **Never gate a no-alloc / ADR-0001 test to integration to save time** — this is non-negotiable.

In short: *move the render, keep the invariant.*

**Never delete a test purely to make the suite faster.** Speed is solved by moving the test (or trimming an oversized sweep), which keeps the coverage. Deleting a test needs a separate, explicit *low-value* justification (redundant, trivial, or asserting nothing) — cost alone is not one.

**Keep a cheap unit fallback.** Where the only coverage of a behaviour is a heavy gated test, also keep a small in-memory unit test for the core behaviour in `make ci`.

**Maintenance — re-audit after large DSP merges.** Heavy renders repeatedly slip the gate when new DSP lands (this is how the suite grew to ~119s before being cut back to ~15s). After a sizable DSP merge, re-measure per-test timing and gate any new multi-second renders. A quick way without nextest/nightly: build the test binaries with `cargo test --workspace --no-run`, then time each `target/debug/deps/<binary>` directly (and, for a hot binary, time individual tests with `<binary> --exact <full::test::path> --test-threads 1`). The longest poles are the gate candidates.

## Bundle Work

Lamath, Glirdir, and Linnod are the current macOS VST3 bundle targets. Use [macos-vst3-build.md](macos-vst3-build.md) for macOS build, install, inspect, and validator steps.

The Galad Windows host (`galad`) is target-gated and excluded from `make ci`; it is cross-compiled and checked with `make host-windows-check` (debug). See [galad/README.md](../galad/README.md) and [architecture.md](architecture.md#windows-vst3-host-galad).

## Release builds

`make release` builds every distributable `--release` into a **separate in-repo target dir** (`./target-release`, gitignored; `$(LINDELION_RELEASE_TARGET_DIR)`) — release artifacts live in the repo, not a hidden home-dir cache — so release-profile cache invalidation never touches the dev/CI cache (`./target`, used by `make ci`/tests) or the iteration cache (`./target-build`, used by `build`/`build-windows`/`host-windows-check`). It dispatches by host OS:

- `make release-windows` (Linux/Windows): `galad.exe` plus the Windows VST3 plugin bundles (`$(WINDOWS_PLUGINS)`), cross-compiled via cargo-xwin.
- `make release-macos` (macOS): the instrument VST3 bundles (`$(PLUGINS)`), staged (not installed).

The dev targets (`build`, `build-windows`, `host-windows-check`) are unchanged and keep using the iteration cache for fast install-to-DAW iteration.

## Commit Baseline

Run `make ci` before committing unless the user explicitly asks for a checkpoint commit. On macOS hosts, the CI path checks macOS-gated Rust code against `MACOS_TARGET` with warnings denied. On non-macOS hosts, that step is skipped because Apple C tooling is required by platform dependencies. Final `.vst3` linking, signing, installation, and validator runs still require macOS. For VST3 ABI or bundle layout changes, validate the installed bundle on macOS when the Steinberg validator is available.
