# macOS VST3 Build

Lamath is the current DAW-loadable VST3 bundle target. Linnod and Glirdir are not part of bundle automation yet.

## Prerequisites

- macOS with Xcode Command Line Tools: `xcode-select --install`
- Rust stable: `rustup update stable`
- Apple Silicon target: `rustup target add aarch64-apple-darwin`

For Intel DAWs, also install `x86_64-apple-darwin` and set `MACOS_TARGET=x86_64-apple-darwin`.

## Build

```bash
make build
```

The Makefile creates `~/.lindelion-cache`, uses `~/.lindelion-cache/target` as the local Cargo target directory, enables incremental compilation for the build, stages the bundle under `~/.lindelion-cache/bundles`, and installs the final VST3 into the system VST3 folder:

```text
/Library/Audio/Plug-Ins/VST3/Ahara/Lamath.vst3
```

The `xtask` bundle command creates the macOS VST3 bundle layout, copies the release `cdylib` into `Contents/MacOS`, writes `Info.plist`, `PkgInfo`, and `Contents/Resources/moduleinfo.json`, and runs ad-hoc `codesign` when available. `make build` sets `LINDELION_BUNDLE_DIR` to the cache staging folder, then uses `sudo ditto` for the final install into `/Library/Audio/Plug-Ins/VST3/Ahara`.

## Install Locally

Enable Ableton's VST3 system folders. A custom VST3 folder is not required for this build path.

Then restart Ableton or rescan VST3 plugins after each rebuild. If using a downloaded CI artifact, clear quarantine before scanning:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Audio/Plug-Ins/VST3/Ahara/Lamath.vst3"
```

## Validate

Before running a host or validator, inspect the installed bundle:

```bash
make inspect-vst3
```

This prints the installed bundle path, the `CFBundleExecutable`, the Mach-O architecture, the exported VST3 entry symbols, and the code signature verification result. The export list should include `GetPluginFactory`, `bundleEntry`, and `bundleExit`.

When the Steinberg VST3 SDK validator is installed:

```bash
validator "/Library/Audio/Plug-Ins/VST3/Ahara/Lamath.vst3"
```
