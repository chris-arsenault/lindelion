# 0025 — Galad preserves plugin state across chain edits via a persistent instance pool

- Status: Accepted
- Date: 2026-06-01

## Context

Galad ([ADR-0022](0022-windows-vst3-host.md)) lets the user edit a running plugin chain — reorder,
bypass, add, and remove slots — while audio flows. Chain edits cross from the control thread to the
audio thread through a lock-free prepared-graph hand-off ([ADR-0001](0001-allocation-free-audio-thread.md)):
the audio thread owns one immutable `ChainProcessor`, and an edit publishes a replacement.

If the replacement is rebuilt by re-instantiating each plugin from its `.vst3` module, every edit
hands the audio thread brand-new plugin instances at their defaults — so reordering one slot silently
resets every plugin's parameters. A VST3 plugin's state lives in its instance (`IComponent`
get/setState; parameters survive `setActive`/`setupProcessing` cycles), and reordering changes the
processing order, not the instances, so this loss is an artifact of recreating instances, not a
property of VST3. The lock-free single-owner hand-off is what creates the tension: the live instances
sit on the audio thread, out of the control thread's reach, so a naive rebuild recreates them.

## Decision

The controller owns a **persistent pool of plugin instances** (one per chain slot), and the
`ChainProcessor` is an *ordering over* the pool rather than the owner of the instances. Each instance
is shared as an `Arc<PluginInstance>` between the pool and the chains that reference it.

- An instance is created once (when its slot is added) and kept alive for the life of the slot.
- A chain edit rebuilds only the ordering — a new `ChainProcessor` holding `Arc` clones of the same
  live instances in the new order/bypass — published gaplessly through the hand-off. Instances are
  never recreated on an edit, so parameters and transient DSP state (reverb tails, filter history)
  survive.
- Teardown rides the `Arc`: an instance's `Drop` (`setActive(false)`/`terminate`) runs only when the
  pool slot **and** every chain that referenced it are gone. The hand-off reclaims retired chains on
  the control thread, so teardown is always control-side and never races the audio thread that is
  processing the instance.
- Session save reads each plugin's opaque state from the pool on the UI thread (a `getState` call,
  valid while audio runs), so saving is also gapless. Session restore rebuilds the pool with each
  plugin's state restored.

## Alternatives considered

- **Capture/restore on rebuild.** Keep recreating instances per edit, but `getState` each plugin
  before and `setState` after (reusing the session machinery). Simpler, but it preserves only
  serialized parameters — not transient DSP state — needs the chain pulled off the audio thread to
  read state (a brief gap per edit), and round-trips serialization on every reorder. Rejected for
  routine edits; it remains the right tool for cross-session save/load.
- **Keep recreating instances (accept state loss).** Smallest code, but it resets plugin state on
  every edit, which is wrong for a host whose purpose is hosting stateful plugins. Rejected.

## Consequences

- `ChainProcessor` holds `Arc<PluginInstance>` (shared with the pool) instead of owning instances;
  the audio thread only dereferences the `Arc` to call `process`, never clones or drops it, so the
  realtime path stays allocation-free.
- Reorder/bypass/add/remove are gapless and state-preserving; session save is gapless.
- Instances are prepared at Start (and on add-while-running) at the device rate; a rate change
  re-prepares the same instances without destroying them, so their state survives that too.
- The pool is the single home for the live instances, so state capture (save) and the chain build
  both read from one place; a separate "pending chain" carrier is unnecessary.
