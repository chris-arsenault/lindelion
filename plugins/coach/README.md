# Lúmedir — Windows-only Speech-Coach VST3 (reserved)

This directory is **reserved** for **Lúmedir**, a Windows-only passthrough Speech-Coach VST3: audio
passes through unchanged, and the value is delivery feedback — speaking rate/cadence (syllable
nuclei), pitch dynamism (flat↔animated), pause structure, and clarity — shown as a live readout
and an end-of-session summary, scored against configurable target bands. Acoustic only (no
transcription/ASR); no reference recording.

It targets **Windows only** — Windows VST3 build + egui editor — per
[ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md), and reuses Cenedril's M0/M1 Windows
platform foundation (the Windows VST3 build path and the egui-in-`IPlugView` editor). It runs in
the Galad host and Windows DAWs.

It is **not yet a workspace member** — a reserved home with intent only, so the build and `make ci`
are untouched. When created, the cross-platform delivery-analysis DSP and egui view logic are
tested in `make ci`; only the `IPlugView` HWND embedding and the Windows bundle are Windows-gated.

- Decision: [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md)
- Implementation plan: [`LUMEDIR-VST-PLAN.md`](../../LUMEDIR-VST-PLAN.md)
