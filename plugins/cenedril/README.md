# Cenedril — Windows-only Visualizer VST3

**Cenedril** is a Windows-only passthrough Visualizer VST3: audio passes through unchanged at zero
latency, and the value is a rich **Vizia** editor (on the shared `lindelion-ui` stack) showing a
spectrogram (STFT-magnitude, then reassigned), level/LUFS meters, and analysis-signal readouts
(voicing, speech presence, onset/flux, HNR). It targets **Windows only** — Windows VST3 build + a
Windows `IPlugView`→`HWND` Vizia/baseview editor attach — per
[ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md); it runs in the Galad host and Windows
DAWs.

**Current state: feature-complete (M0–M6).** Bit-exact zero-latency passthrough; single-component
VST3; a Vizia editor with magnitude + reassigned spectrogram, level/LUFS meters, an analysis-signal
panel, and selectable view / frequency-scale / color-map / dB-range controls whose settings persist
in plugin state. The cross-platform DSP, display models, and editor build are tested in `make ci`
(Linux); the Vizia view, the `IPlugView`→`HWND` attach, and the Windows bundle are Windows-gated and
cross-built with cargo-xwin. On-target Windows load-and-run is the one unverified item
([cenedril-backlog.md](../../docs/plugins/cenedril-backlog.md)). Full spec + architecture:
[docs/plugins/cenedril.md](../../docs/plugins/cenedril.md).

## Build (Windows VST3)

Cenedril is cross-built from Linux as an MSVC-ABI `.vst3` with **cargo-xwin** (ADR-0023). It pulls
the SwiftF0/ONNX analysis stack (`tract`), whose `tract-linalg` build script compiles C/asm SIMD
kernels — so cross-compiling for Windows needs a full LLVM cross C-toolchain in addition to
cargo-xwin's MS CRT/SDK:

```sh
cargo install cargo-xwin                 # one-time; downloads the MS CRT/SDK on first build
rustup target add x86_64-pc-windows-msvc # one-time
rustup component add llvm-tools          # provides llvm-ar (multi-call) for the next step
# clang-cl (the cross C compiler) — from your distro's clang/LLVM package, e.g. `dnf install clang`
# llvm-lib (the MSVC archiver) — LLVM's llvm-ar is multi-call; symlink it onto PATH as llvm-lib:
ln -sf "$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-ar" ~/.local/bin/llvm-lib
make build-windows PLUGIN=cenedril       # stages Cenedril.vst3 in the bundle staging dir
```

Then copy the staged `Cenedril.vst3` to a Windows host, or load it in the Galad host, to verify.

> **Cache gotcha:** `make build-windows` uses a separate target dir (`~/.lindelion-cache/target`)
> from `make ci`. If that dir is on a different mount with mtime skew, cargo may not rebuild changed
> workspace crates; if a Windows build links a stale crate, force it with
> `CARGO_TARGET_DIR=~/.lindelion-cache/target cargo clean -p <crate> --target x86_64-pc-windows-msvc`.
> Inspect the bundled DLL's exports with binutils `objdump -p` (not `llvm-objdump`, which is not on
> PATH here).

- Spec / architecture: [docs/plugins/cenedril.md](../../docs/plugins/cenedril.md)
- Decisions: [ADR-0040 — Cenedril analysis and editor delivery](../../docs/adr/0040-cenedril-analysis-and-editor-delivery.md), [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md)
- Reassignment operator: [docs/dsp/reassignment.md](../../docs/dsp/reassignment.md)
- Backlog: [docs/plugins/cenedril-backlog.md](../../docs/plugins/cenedril-backlog.md)
