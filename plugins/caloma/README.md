# Calóma

Quenya `cala` ("bright/clear") + `óma` ("voice") — "clear voice".

The single **Windows-only** VST3 that packages the ported speech effects (`speech/`) as one serial
clarity chain for spoken word: a 3-valued **signal-order** parameter (three curated topologies —
Clarity / Broadcast / Light — each with a committed default tuning), **compute-once** shared
analysis feeding the chain, and a **self-contained Vizia editor as the sole control surface** (no
host parameters). Built as a single-component VST3 on `lindelion-plugin-shell` + the `vst3` crate.

See the [implementation spec](../../docs/plugins/caloma.md) for the current behavior (architecture,
the editor/control model, the default tunings, and the gates),
[ADR-0020](../../docs/adr/0020-caloma-speech-vst-packaging.md) for the packaging decision, and
[ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md) for the Windows-only / Vizia-editor decision.

- Build + stage the Windows bundle: `make build-windows` (cross-compiles `Caloma.vst3` via
  cargo-xwin, including the Vizia editor).
- Re-derive the committed per-order default tunings: `make tune-defaults` (heavy, release).
- Verify: `make ci` (fast unit suite) and `make test-models` (heavy NN end-to-end fidelity gates).
