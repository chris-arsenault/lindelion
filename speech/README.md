# Speech effects

The speech-effect product family: a port of the channel-strip effects from the `hot-mic` C#
microphone processor into Rust, tuned for **spoken word in a meeting / oration context** —
speech clarity and intelligibility, not musicality.

This tree holds the speech-specific code. Use-case-neutral foundations it builds on live in
`crates/` (`lindelion-effect`, `lindelion-fidelity`, `lindelion-dsp-utils`,
`lindelion-pitch-detect`, `lindelion-onset-detect`). The split keeps the product boundary
physical and the workspace's shared assets reusable across both the music instruments and the
speech effects — see [ADR-0012](../docs/adr/0012-speech-effect-port-shared-workspace.md).

Packaging is deliberately undecided (standalone app, single VST with a prebaked flow, or one
VST per effect). Nothing here depends on a host, a chain position, or a VST entry point — see
[ADR-0013](../docs/adr/0013-host-agnostic-effect-core.md).

## Layout

| Path | Purpose |
| ---- | ---- |
| `speech/signals` | Speech analysis-signal derivation library (pitch, voicing, presence, sibilance, fricative, onset flux, HNR), self-derived per effect from shared crates and re-tuned for speech. |
| `speech/<effect>` | One crate per ported effect (package `lindelion-speech-<effect>`). |

## Effect roster

- **Tier 1 (classic time-domain):** gain, noise gate, compressor, limiter, de-esser,
  high-pass filter, 5-band EQ, saturation, upward expander, dynamic EQ.
- **Tier 2 (spectral / FFT):** FFT noise removal, air exciter, bass enhancer, consonant
  transient, dereverberation, room tone, spectral contrast, vitalizer.
- **Tier 3 (ML / ONNX):** RNNoise, speech denoiser, voice gate (VAD).

Milestones, reuse map, and the analysis-signal strategy: [HOTMIC-PORT-PLAN.md](../HOTMIC-PORT-PLAN.md).
Future-state work: [docs/backlog.md](../docs/backlog.md).
