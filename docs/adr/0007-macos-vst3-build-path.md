# 0007 — macOS-only VST3 build path

- Status: Accepted
- Date: 2026-05-23

## Context

VST3 hosts require platform-specific bundles, code signing, and host-specific validation (Ableton, Logic, etc.). Apple Silicon and Intel macOS are the workspace's primary targets. Cross-compiling VST3 bundles from Linux is fragile and code signing requires Apple tooling.

## Decision

Final `.vst3` bundle linking and signing happen on macOS via Apple tooling and the workspace's `xtask` bundle automation. Linux is used for library and DSP development, `make ci` checks (rustfmt, clippy, file-size lint, tests), and host-agnostic Criterion benches. Linux cross-checks for macOS are not treated as real bundle builds.

## Alternatives considered

- **Cross-build VST3 from Linux.** Code signing is not reliably replicable outside Apple tooling. Rejected for production bundles.
- **CLAP-first to avoid platform-specific bundling.** Deferred. CLAP support is planned (see `docs/backlog.md`) but VST3 ships first because Ableton on macOS is the primary host target.
- **Containerized macOS build host.** Adds operational complexity; the current `make build` on a macOS workstation is simpler.

## Consequences

- `make build` exits with an error on non-Darwin hosts. The error message names the macOS requirement.
- CI's verification path is library/test only. Bundle validation is manual on macOS until automated.
- Glirdir and Lamath ship as VST3 first. CLAP and AU adapters are future work captured in `docs/backlog.md`.
- Critical rule in `AGENTS.md`: do not treat Linux cross-checks for macOS as real `.vst3` bundle builds.
