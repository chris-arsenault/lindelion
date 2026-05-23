# Development

Local development uses stable Rust and Makefile entrypoints for repeatable checks.

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Run the canonical local check path. |
| `make build` | Build, stage, and install the default macOS VST3 bundle on macOS. |
| `make build PLUGIN=glirdir` | Build, stage, and install Glirdir on macOS. |
| `make inspect-vst3` | Inspect the installed default VST3 bundle on macOS. |
| `make inspect-vst3 PLUGIN=glirdir` | Inspect the installed Glirdir VST3 bundle on macOS. |

## Bundle Work

Lamath and Glirdir are the current VST3 bundle targets. Use [macos-vst3-build.md](macos-vst3-build.md) for macOS build, install, inspect, and validator steps.

## Commit Baseline

Run `make ci` before committing unless the user explicitly asks for a checkpoint commit. For VST3 ABI or bundle layout changes, validate the installed bundle on macOS when the Steinberg validator is available.
