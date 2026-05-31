# Galad — Windows realtime VST3 host (reserved)

This directory is **reserved** for **Galad**, a standalone general-purpose realtime VST3 host
application for Windows: live microphone → an ordered chain of arbitrary VST3 plugins → output
device, with full device management. It is the *host* side of VST3 (distinct from the workspace's
plugins, which are the guest side), built on the `vst3` crate's COM bindings, native WASAPI audio
I/O, and an `egui` UI.

It is **not yet a workspace member** — this is a reserved home with intent only, so the build and
`make ci` are untouched. When the crate is created it will be **target-gated** (Windows-only) and
kept out of the Linux/macOS `make ci` path.

- Decision: [ADR-0022 — Windows realtime VST3 host application](../docs/adr/0022-windows-vst3-host.md)
- Implementation plan: [`GALAD-HOST-PLAN.md`](../GALAD-HOST-PLAN.md)
