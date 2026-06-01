# 0026 — Galad contains misbehaving plugins in-process, not via an out-of-process sandbox

- Status: Accepted
- Date: 2026-06-01

## Context

Galad ([ADR-0022](0022-windows-vst3-host.md)) loads arbitrary third-party VST3 plugins, which are
untrusted native code running on the realtime audio thread in the host's own process. A plugin can be
incompatible (no audio class), fail mid-`process`, emit NaN/Inf, or — in the worst case — abort,
corrupt memory, or hang inside its own C++. "A misbehaving plugin must not take down the host" is a
goal, but it is only *fully* achievable by running plugins in a separate process, which a host can
isolate from a crash. That isolation costs a subprocess per plugin, shared-memory audio transport,
and an IPC protocol on the realtime path.

## Decision

Galad contains the misbehaving-plugin failure modes it can reach **in process**, and does not adopt
an out-of-process plugin sandbox.

- **Load-time validation:** before a plugin enters the chain it is probed end-to-end (instantiate →
  prepare → process one silent block); an incompatible or mid-process-failing plugin is rejected at
  the add step instead of failing later on the audio thread.
- **Output sanitation:** the chain's output is finite-guarded, so a plugin emitting NaN/Inf cannot
  propagate non-finite samples to the device.
- The residual is documented: a plugin that aborts, corrupts memory, or hangs in its own code can
  still bring the host down. Validation reduces the odds (a plugin that crashes during the probe is
  rejected before it is ever added), but it is not a guarantee.

## Alternatives considered

- **Out-of-process plugin sandbox.** Run each plugin in a subprocess with shared-memory audio + IPC,
  giving true crash isolation that fully satisfies "cannot crash the host." Rejected: it is a large
  architecture with added latency, complexity, and an IPC layer on the realtime path — disproportionate
  for a lightweight single-channel router whose other realtime guarantees
  ([ADR-0001](0001-allocation-free-audio-thread.md)) assume in-process processing.

## Consequences

- The host stays a single process: lowest latency, simplest realtime path, no IPC.
- Incompatible and NaN-emitting plugins are contained; an aborting/UB/hanging plugin is a documented
  residual risk, surfaced to the user rather than hidden.
- Real-plugin validation across a third-party matrix and a stability/leak soak are field activities,
  recorded alongside the host (`galad/PLUGIN-MATRIX.md`).
