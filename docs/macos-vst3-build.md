# macOS VST3 Build

The resonator synth builds as a DAW-loadable VST3 bundle on macOS.

## Prerequisites

- macOS with Xcode Command Line Tools: `xcode-select --install`
- Rust stable: `rustup update stable`
- Apple Silicon target: `rustup target add aarch64-apple-darwin`

For Intel DAWs, also install `x86_64-apple-darwin` and pass that target instead.

## Build

```bash
cargo run -p xtask -- check
cargo run -p xtask -- bundle resonator-synth --target aarch64-apple-darwin
```

The bundle is written to:

```text
target/bundles/Ahara Resonator Synth.vst3
```

The `xtask` bundle command creates the macOS VST3 bundle layout, copies the release `cdylib` into `Contents/MacOS`, writes `Info.plist` and `PkgInfo`, and runs ad-hoc `codesign` when available.

## Install Locally

```bash
mkdir -p "$HOME/Library/Audio/Plug-Ins/VST3"
rm -rf "$HOME/Library/Audio/Plug-Ins/VST3/Ahara Resonator Synth.vst3"
cp -R "target/bundles/Ahara Resonator Synth.vst3" "$HOME/Library/Audio/Plug-Ins/VST3/"
```

Then rescan VST3 plugins in the DAW. If using a downloaded CI artifact, clear quarantine before scanning:

```bash
xattr -dr com.apple.quarantine "$HOME/Library/Audio/Plug-Ins/VST3/Ahara Resonator Synth.vst3"
```

## Validate

When the Steinberg VST3 SDK validator is installed:

```bash
validator "target/bundles/Ahara Resonator Synth.vst3"
```

