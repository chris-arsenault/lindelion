# lindelion-effect

Host-agnostic audio-effect contract: the trait every ported effect implements, plus the
neutral parameter, state, and latency primitives around it.

This is the effect-processor counterpart to `lindelion-plugin-shell`'s `AudioPlugin`. Where
`AudioPlugin` is the VST3-coupled host boundary for the instrument plugins, `lindelion-effect`
is deliberately **host-free**: it depends only on pure-DSP crates and exposes neutral
primitives — a sample-block `process`, indexed float parameters, opaque byte-blob state, and
latency in samples — so an effect can be wrapped by a standalone app, a single VST, or a
VST-per-effect without change.

- Allocation-free `process` on the audio thread (see [ADR-0001](../../docs/adr/0001-allocation-free-audio-thread.md)).
- No dependency on `lindelion-plugin-shell`, `vst3`, or `lindelion-ui` (see [ADR-0013](../../docs/adr/0013-host-agnostic-effect-core.md)).

Scope and milestones: [HOTMIC-PORT-PLAN.md](../../HOTMIC-PORT-PLAN.md). Effect roster and
future work: [docs/backlog.md](../../docs/backlog.md).
