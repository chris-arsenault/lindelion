# Development

Local development uses stable Rust and the workspace `xtask` command for repeatable checks.

## Commands

| Command | Purpose |
| ---- | ---- |
| `make ci` | Run the canonical local check path. |
| `cargo run -p xtask -- check` | Run rustfmt, release clippy with cognitive complexity enabled, and all workspace tests. |
| `cargo fmt --all -- --check` | Check Rust formatting. |
| `cargo clippy --workspace --all-targets --release -- -D warnings -W clippy::cognitive_complexity` | Run the same clippy mode as `xtask`. |
| `cargo test --workspace` | Run all workspace tests. |
| `cargo check -p lamath` | Type-check the implemented Lamath plugin crate. |
| `cargo test -p lamath --lib` | Run Lamath unit tests. |
| `cargo check -p linnod` | Type-check the Linnod scaffold crate. |
| `cargo test -p linnod` | Run the Linnod test target. |
| `cargo check -p lamath --target aarch64-apple-darwin` | Type-check the macOS Lamath target when the Rust target is installed. This is not a real macOS bundle build on Linux. |

## Bundle Work

Lamath is the only current VST3 bundle target. Use [macos-vst3-build.md](macos-vst3-build.md) for macOS build, install, inspect, and validator steps.

## Commit Baseline

Run `make ci` before committing unless the user explicitly asks for a checkpoint commit. For VST3 ABI or bundle layout changes, also run the macOS target check and validate the installed bundle on macOS when the Steinberg validator is available.
