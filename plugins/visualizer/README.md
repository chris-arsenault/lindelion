# Cenedril — Windows-only Visualizer VST3 (reserved)

This directory is **reserved** for **Cenedril**, a Windows-only passthrough Visualizer VST3: audio
passes through unchanged at zero latency, and the value is a rich **egui** editor showing a
spectrogram (STFT-magnitude, then reassigned), level/LUFS meters, and analysis-signal readouts
(voicing, speech presence, onset/flux, HNR). It targets **Windows only** — Windows VST3 build +
egui editor — per [ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md); it runs in the Galad
host and Windows DAWs.

It is **not yet a workspace member** — a reserved home with intent only, so the build and `make ci`
are untouched. When created, the cross-platform DSP/processor and egui view logic are tested in
`make ci` (Linux); only the `IPlugView` HWND embedding and the Windows bundle are Windows-gated.

- Decision: [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md)
- Implementation plan: [`CENEDRIL-VST-PLAN.md`](../../CENEDRIL-VST-PLAN.md)
