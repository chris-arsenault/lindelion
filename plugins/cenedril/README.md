# Cenedril — Windows-only Visualizer VST3

**Cenedril** is a Windows-only passthrough Visualizer VST3: audio passes through unchanged at zero
latency, and the value is a rich **Vizia** editor (on the shared `lindelion-ui` stack) showing a
spectrogram (STFT-magnitude, then reassigned), level/LUFS meters, and analysis-signal readouts
(voicing, speech presence, onset/flux, HNR). It targets **Windows only** — Windows VST3 build + a
Windows `IPlugView`→`HWND` Vizia/baseview editor attach — per
[ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md); it runs in the Galad host and Windows
DAWs.

**Current state (M0):** the crate exists with a bit-exact, zero-latency passthrough processor and
the Windows `.vst3` bundle path. The Vizia editor and its Windows attach (M1), realtime analysis
(M2), and the spectrogram/meter views (M3+) are later milestones. The cross-platform DSP/processor
logic is tested in `make ci` (Linux); the Vizia view logic (later), the `IPlugView`→`HWND` baseview
attach, and the Windows bundle are Windows-gated.

## Build (Windows VST3)

Cenedril is cross-built from Linux as an MSVC-ABI `.vst3` with **cargo-xwin** (ADR-0023):

```sh
cargo install cargo-xwin                 # one-time; downloads the MS CRT/SDK on first build
rustup target add x86_64-pc-windows-msvc # one-time
make build-windows                       # stages Cenedril.vst3 in the bundle staging dir
```

Then copy the staged `Cenedril.vst3` to a Windows host, or load it in the Galad host, to verify it
passes audio through bit-exact at zero latency.

- Decision: [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md)
- Implementation plan: [`CENEDRIL-VST-PLAN.md`](../../CENEDRIL-VST-PLAN.md)
- M0 steps: [`CENEDRIL-M0-STEPS.md`](../../CENEDRIL-M0-STEPS.md)
