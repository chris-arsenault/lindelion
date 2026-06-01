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
| `make build-windows` | Cross-compile the Windows-only VST3 plugins for `x86_64-pc-windows-msvc` via cargo-xwin. |

## Bundle Work

Lamath, Glirdir, and Linnod are the current macOS VST3 bundle targets. Use [macos-vst3-build.md](macos-vst3-build.md) for macOS build, install, inspect, and validator steps.

The Galad Windows host (`galad`) is target-gated and excluded from `make ci`; it is cross-compiled and checked with `make host-windows-check` (debug). See [galad/README.md](../galad/README.md) and [architecture.md](architecture.md#windows-vst3-host-galad).

## Release builds

`make release` builds every distributable `--release` into a **separate target dir** (`$(LINDELION_RELEASE_TARGET_DIR)`, default `$(CACHE_DIR)/target-release`) so release-profile cache invalidation never touches the dev/CI cache (`./target`, used by `make ci`/tests) or the iteration cache (`$(CACHE_DIR)/target`, used by `build`/`build-windows`/`host-windows-check`). It dispatches by host OS:

- `make release-windows` (Linux/Windows): `galad.exe` plus the Windows VST3 plugin bundles (`$(WINDOWS_PLUGINS)`), cross-compiled via cargo-xwin.
- `make release-macos` (macOS): the instrument VST3 bundles (`$(PLUGINS)`), staged (not installed).

The dev targets (`build`, `build-windows`, `host-windows-check`) are unchanged and keep using the iteration cache for fast install-to-DAW iteration.

## Commit Baseline

Run `make ci` before committing unless the user explicitly asks for a checkpoint commit. On macOS hosts, the CI path checks macOS-gated Rust code against `MACOS_TARGET` with warnings denied. On non-macOS hosts, that step is skipped because Apple C tooling is required by platform dependencies. Final `.vst3` linking, signing, installation, and validator runs still require macOS. For VST3 ABI or bundle layout changes, validate the installed bundle on macOS when the Steinberg validator is available.
