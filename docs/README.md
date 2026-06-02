# Lindelion Docs

Documentation index for the Lindelion audio-plugin workspace.

## Product Docs

| Product | Description | Docs |
| ---- | ---- | ---- |
| Lamath | VST3 resonator instrument with MIDI and sidechain audio inputs | [Spec](plugins/lamath.md), [backlog](plugins/lamath-backlog.md), [README](../plugins/lamath/README.md) |
| Linnod | VST3 melodic slicer instrument with source analysis, slice playback, editor surface, and bundle support | [Spec](plugins/linnod.md), [backlog](plugins/linnod-backlog.md), [README](../plugins/linnod/README.md) |
| Glirdir | VST3 sing-to-MIDI scratchpad with editor, drag/export, sample-library save, and bundle support | [Spec](plugins/glirdir.md), [backlog](plugins/glirdir-backlog.md), [README](../plugins/glirdir/README.md) |
| Calóma | Windows-only speech-clarity VST3 — 20-effect serial chain, 3 signal orders, self-contained Vizia editor (no host params), per-order tuned defaults | [Spec](plugins/caloma.md), [backlog](plugins/caloma-backlog.md), [README](../plugins/caloma/README.md) |
| Galad | Standalone Windows realtime VST3 *host* application: mic → an arbitrary VST3 chain → output device | [README](../galad/README.md), [architecture](architecture.md#windows-vst3-host-galad) |

## Architecture And Development

| Topic | Doc |
| ---- | ---- |
| Workspace architecture | [architecture.md](architecture.md) |
| Architecture decisions | [adr/README.md](adr/README.md) |
| Speech effects family | [../speech/README.md](../speech/README.md) |
| DSP module docs | [dsp/README.md](dsp/README.md) |
| Local development commands | [development.md](development.md) |
| Testing (suites; what stays in `make ci` vs moves to integration) | [development.md#testing](development.md#testing) |
| Real-time audio performance contract | [performance.md](performance.md) |
| macOS VST3 build and validation | [macos-vst3-build.md](macos-vst3-build.md) |
| Workspace backlog | [backlog.md](backlog.md) |
| Changelog | [../CHANGELOG.md](../CHANGELOG.md) |

## Agent Guide

| File | Purpose |
| ---- | ---- |
| [../AGENTS.md](../AGENTS.md) | Canonical agent guide for every coding agent (Claude, Codex, etc.) |
