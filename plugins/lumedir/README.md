# Lúmedir — Windows-only Speech-Coach VST3

**Lúmedir** (Quenya `lúme` "time/rhythm" + `-dir` "keeper") is a Windows-only passthrough
Speech-Coach VST3: audio passes through unchanged at zero latency, and the value is feedback on
*delivery* — speaking **rate/cadence**, **pitch dynamism**, **pause structure**, and **clarity** —
as a live running readout and an end-of-session summary, scored against configurable target bands. It
targets **Windows only** — a Windows VST3 build + a Windows `IPlugView`→`HWND` Vizia/baseview editor
attach — per [ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md), reusing Cenedril's M0/M1
platform foundation; it runs in the Galad host and Windows DAWs. (The crate/dir is `lumedir`,
matching the product name; an earlier codename was `coach`.)

**Current state (M0–M4):** a bit-exact, zero-latency passthrough processor that **feeds the
off-thread delivery worker** (`DeliveryWorker` over `lindelion-speech-signals`), the Windows `.vst3`
bundle path, the M1–M3 delivery-metric estimators (speaking rate, pitch dynamism, pause structure,
clarity) assembled into a `DeliverySnapshot`, and the M4 **live running-readout** Vizia editor —
delivery gauges rendered through the shared `lindelion-ui::vizia_meter::meter_row`, polling the
worker's snapshots off the audio thread. It is a **single-component VST3** (one COM object is
processor + controller), so the editor reads the worker's snapshots directly through a lock-free
`DeliveryReader` — mirroring Cenedril and Calóma, which also expose no host parameters. The
end-of-session summary + target-band scoring (M5) and persistence (M6) are later milestones. The
cross-platform DSP/processor, the delivery source/host wiring, and the metric→gauge mappings are
tested in `make ci` (Linux); the Vizia view, the `IPlugView`→`HWND` baseview attach, and the Windows
bundle are Windows-gated (compile-checked via the Windows build, visuals verified on a Windows host).

## Build (Windows VST3)

Lúmedir is cross-built from Linux as an MSVC-ABI `.vst3` with **cargo-xwin** (ADR-0023):

```sh
cargo install cargo-xwin                 # one-time; downloads the MS CRT/SDK on first build
rustup target add x86_64-pc-windows-msvc # one-time
make build-windows                       # stages Lumedir.vst3 in the bundle staging dir
```

Then copy the staged `Lumedir.vst3` to a Windows host, or load it in the Galad host, to verify it
passes audio through bit-exact at zero latency.

- Decision: [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md)
- Implementation plan: [`LUMEDIR-VST-PLAN.md`](../../LUMEDIR-VST-PLAN.md)
- M0 steps: [`LUMEDIR-M0-STEPS.md`](../../LUMEDIR-M0-STEPS.md)
