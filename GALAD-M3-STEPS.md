# Galad M3 — Step Plan

Expansion of **M3** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: **drive the VST3 chain from the audio callback** — join the host-side VST3 (M1) to the
WASAPI engine (M2) under the realtime discipline. The RT callback runs an ordered chain of VST3
processors in sequence; a **lock-free control→audio hand-off** applies chain edits
(add/remove/reorder/bypass) by swapping a prepared graph, with **no locks or allocation on the audio
thread**; aggregate + report latency.

**Source-of-truth & reference (re-derive from these, not memory):**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) "Cross-cutting constraints" (RT callback allocation-free
  + lock-free; chain edits cross via a lock-free **prepared hand-off**; single channel: one input →
  one serial chain → one output).
- [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md): **fully applies** — the chain
  processing and the hand-off read on the audio thread allocate nothing and take no locks. Use
  `lindelion-test-allocator` (`assert_no_allocations`) as in M1/M2.
- **Reuse (M1):** `host/src/vst3_host/processing.rs` `ProcessDriver` (its `prepare` sequence + the
  `ProcessData`/`AudioBusBuffers`/`channelBuffers32` build — extracted alloc-free in Step 1);
  `PluginInstance` (`instance.rs`); the `#[cfg(test)]` fixture plugin (`fixture.rs`).
- **Reuse (M2):** `host/src/audio/transport.rs` `Transport` (the ring-backed render path — Step 5
  inserts the chain); `host/src/audio/wasapi/engine.rs` (the RT thread — Step 6 runs the chain).
- VST3 `process()` uses **deinterleaved** `channelBuffers32` (separate L/R); the ring/Transport use
  **interleaved** stereo — the chain deinterleaves in, interleaves out.

## Resolved decisions (mine — internal implementation, no user-facing `[DECISION]`)

The plan lists **no** M3 decision. The internal choices I resolve:
- **Lock-free hand-off = hand-rolled `AtomicPtr` pending/retired protocol** (not `arc-swap`). Reason:
  it guarantees **reclamation happens on the control thread, never the audio thread** (the audio side
  only swaps raw pointers and parks the old graph in a `retired` slot; it never drops/frees). No new
  dependency. The invariant (Step 4) keeps at most one retired graph outstanding.
- **The chain runs in the render pump**, on interleaved stereo popped from the ring, before the
  output channel-adapt. **In-place stereo process** with **planar L/R ping-pong** buffers, all
  preallocated → alloc-free per callback. The chain is **stereo** (the single-chain width).
- **`unsafe impl Send for ChainProcessor`** — the graph is built on the control thread and processed
  only on the audio thread (handed off, never shared); the COM `ComPtr`s are used single-threaded.

**Verification strategy (no WASAPI/wine on Linux — same split as M1/M2):** the load-bearing risk —
alloc-free multi-stage chain processing and the lock-free hand-off — is **fully automated on Linux**
(`cargo test -p galad`) against the in-process fixture plugin (Steps 1–5), including
`assert_no_allocations`. The WASAPI engine integration (Step 6) is **cross-compile-verified**
(`make host-windows-check`); the live "mic → ≥2 real VST3 plugins → output, glitch-free, edits without
dropouts" is the **Windows-runtime field check** (`galad chain …`), recorded on hardware, not a
`make ci` gate (no human-audition exit).

`galad` stays excluded from `make ci`. Tests: **`cargo test -p galad`**; Windows: **`make
host-windows-check`**.

---

## Step 1 — Extract an alloc-free stereo `drive_process` primitive from `ProcessDriver`  [refactor]
- **File(s):** `host/src/vst3_host/processing.rs` (extract); optionally a new
  `host/src/vst3_host/block.rs` for the primitive (register in `mod.rs` if split).
- **Reference behavior:** `ProcessDriver::process_block` (M1) builds the per-call
  `AudioBusBuffers`/`ProcessData` over `Vec`-allocated planar buffers. Extract the **allocation-free
  core** — `unsafe fn drive_process(processor: &ComPtr<IAudioProcessor>, input: [&[f32]; 2], output:
  [&mut [f32]; 2])` — that drives one `process()` over **caller-owned** stereo planar buffers using
  **stack** `[*mut Sample32; 2]` channel-pointer arrays + stack `AudioBusBuffers`/`ProcessData`
  (mirroring `vst3_process.rs`'s `channelBuffers32` layout). `process_block` becomes a thin
  *allocating* wrapper (for M1's tests) that fills `Vec`s and calls `drive_process`. No behavior
  change for stereo.
- **Change:** add `drive_process` (stereo, alloc-free); rewrite `process_block` to call it; keep the
  M1 `prepare` sequence as-is.
- **Verify:** M1's existing `silence_stays_silent` and `sine_passes_through_bit_exact_at_zero_latency`
  still pass (behavior-preserving). **Add** a no-alloc test: prepare a fixture instance, then
  `assert_no_allocations("drive_process", || drive_process(processor, [&l_in,&r_in], [&mut l_out,&mut
  r_out]))` over preallocated buffers. **Red:** `drive_process` doesn't exist yet; **green:** M1 tests
  unchanged + the new no-alloc test passes. (`cargo test -p galad`.)

## Step 2 — Parameterize the fixture with gain + latency (so bypass ≠ process is observable)  [depends on #1]
- **File(s):** `host/src/vst3_host/fixture.rs` (`#[cfg(test)]`).
- **Reference behavior:** The M1 fixture is a pure passthrough; a chain of passthroughs can't show
  that bypass differs from processing, nor a non-zero aggregate latency. Make `FixtureProcessor {
  gain: f32, latency: u32 }`: `process` multiplies each sample by `gain` (was a verbatim copy);
  `getLatencySamples` returns `latency`. `FixtureFactory { gain, latency }` creates configured
  processors. Keep `fixture_factory()` = gain **1.0**, latency **0** (so M1's tests stay bit-exact —
  ×1.0 is identity, latency 0). Add `gain_fixture_factory(gain, latency) -> ComPtr<IPluginFactory>`.
- **Change:** add the two fields + the multiply + the parameterized factory constructor; default
  factory unchanged.
- **Verify:** M1's `fixture_factory_creates_audio_processor` still passes. **Add** a test: instantiate
  a `gain_fixture_factory(0.5, 7)` via `PluginInstance::from_factory`, drive a block through
  `drive_process`, assert output = input × 0.5 and `getLatencySamples() == 7`. **Red:** the gain/latency
  fields/factory don't exist; **green:** scaling + latency hold. (`cargo test -p galad`.)

## Step 3 — `ChainProcessor`: alloc-free serial stereo chain with per-slot bypass + aggregate latency  [depends on #1, #2]
- **File(s):** `host/src/vst3_host/chain.rs` (new), `host/src/vst3_host/mod.rs` (register + re-export).
- **Reference behavior:** Hold an ordered `Vec` of slots, each `{ instance: PluginInstance, bypassed:
  bool }`, plus **preallocated** planar ping-pong buffers (`a_l,a_r,b_l,b_r`, each `max_frames`).
  `prepare` each instance via the M1 `ProcessDriver::prepare` sequence (stereo bus arrangement,
  `setupProcessing`, activate). `process_in_place(&mut self, stereo: &mut [f32])`: deinterleave
  `stereo`→`a_l/a_r`; for each slot, **if bypassed skip** (signal stays in `a`), else
  `drive_process(slot.processor, [a_l,a_r] → [b_l,b_r])` (Step 1) then swap `a`/`b`; interleave final
  `a_l/a_r`→`stereo`. **Allocation-free** per call (only preallocated buffers + stack `drive_process`).
  `aggregate_latency() -> u32` sums `getLatencySamples` over **non-bypassed** slots.
  **`unsafe impl Send for ChainProcessor`** (single-audio-thread discipline; see preamble).
- **Change:** add `chain.rs` with `ChainProcessor`, `ChainSlot`, `fn new(instances, bypass_flags,
  sample_rate, max_frames) -> Result<Self, HostError>` (prepares each), `process_in_place`,
  `aggregate_latency`, `unsafe impl Send`.
- **Verify:** `cargo test -p galad` with `gain_fixture_factory` instances: (a) a 2-slot chain of
  ×0.5 gains scales a block by **×0.25**; (b) bypassing one slot scales by **×0.5**, bypassing both is
  **identity**; (c) `aggregate_latency` of `[lat 3, lat 4]` is **7**, and **0** when both bypassed;
  (d) `assert_no_allocations` around `process_in_place`. **Red:** `ChainProcessor` absent; **green:**
  all hold.

## Step 4 — Lock-free `Handoff<T>`: publish on control, take/retire on audio, reclaim off the audio thread  [depends on #3 for the payload, but generic]
- **File(s):** `host/src/vst3_host/handoff.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** A single-producer (control) → single-consumer (audio) hand-off over two
  `AtomicPtr<T>` slots, `pending` (control→audio) and `retired` (audio→control). **Control**
  `publish(Box<T>)`: first `reclaim()` (`retired.swap(null, Acquire)`, drop if non-null), then
  `let prev = pending.swap(Box::into_raw(new), AcqRel); if !prev.is_null() { drop(Box::from_raw(prev))
  }` (drops an un-taken prior graph **on the control thread**). **Audio** `try_take() -> Option<*mut T>`:
  `pending.swap(null, Acquire)` → the raw pointer, or `None`. **Audio** `retire(old: *mut T)`:
  `retired.store(old, Release)` (the invariant — control reclaims before each publish, audio retires
  only after taking a pending — keeps **at most one** retired outstanding, so the audio side **never
  drops/frees**). `reclaim()` drops the retired graph on the control thread. The audio side holds its
  current `*mut T` externally and uses it via `&mut *current`.
- **Change:** add `handoff.rs` with `Handoff<T>` (the two `AtomicPtr`s), `publish`, `reclaim`,
  `try_take`, `retire`, and a `Drop` that reclaims both slots.
- **Verify:** `cargo test -p galad` with a drop-counting payload: publish A → `try_take` returns A;
  publish B (reclaims nothing yet) → `try_take` returns B, `retire(A)`; assert **A not yet dropped**
  (audio `retire` does not free); then control `reclaim()` → **A dropped** (reclamation on the control
  side); a second publish-while-pending reclaims the un-taken graph; `assert_no_allocations` around
  `try_take`+`retire`. **Red:** `Handoff` absent; **green:** the swap sequence, the "retire doesn't
  drop / reclaim drops" split, and the no-alloc hold.

## Step 5 — `Transport::render_through`: run an in-place stereo processor between ring-pop and output-adapt  [refactor of M2 render]
- **File(s):** `host/src/audio/transport.rs`.
- **Reference behavior:** M2's `Transport::render(out)` pops stereo from the ring and channel-adapts
  to the output device. Generalize to `render_through(&mut self, out: &mut [f32], process_stereo: impl
  FnOnce(&mut [f32]))`: pop stereo into the (preallocated) render scratch, call
  `process_stereo(&mut scratch_stereo)` **in place**, then channel-adapt the scratch to `out`. Keep
  `render(out)` as `render_through(out, |_| {})` (no-op) so M2's tests are behavior-preserving. The
  closure runs on the audio thread → it (and thus the chain it will carry) must be alloc-free, but
  `Transport` stays plugin-agnostic (no `vst3_host` dependency — the chain is injected by the engine
  in Step 6).
- **Change:** add `render_through`; reimplement `render` to delegate with a no-op closure.
- **Verify:** M2's `stereo_loopback_passes_signal_through`, `mono_capture_duplicates_into_stereo_output`,
  `render_under_run_is_silence` still pass (via the delegating `render`). **Add** a test: push a block,
  `render_through(out, |s| s.iter_mut().for_each(|x| *x *= 0.5))`, assert the output is the popped
  signal scaled ×0.5 (the closure runs between pop and output-adapt). **Red:** `render_through` absent;
  **green:** M2 tests unchanged + the scaling test passes. (`cargo test -p galad`.)

## Step 6 — Engine integration + `galad chain` subcommand (drive the chain from the RT callback)  [depends on #3, #4, #5] [windows]
- **File(s):** `host/src/audio/wasapi/engine.rs`, `host/src/audio/mod.rs` (re-export the publisher
  type), `host/src/main.rs` (a `chain <in-id> <out-id> <plugin.vst3>...` subcommand, `#[cfg(windows)]`),
  `host/README.md` (note the chain flow).
- **Reference behavior:** The engine joins M1+M2. `RunningState` gains a current chain `*mut
  ChainProcessor` and a `Handoff<ChainProcessor>`. The render pump replaces `transport.render(out)`
  with: `if let Some(next) = handoff.try_take() { handoff.retire(current); current = next; }` then
  `transport.render_through(out, |stereo| (&mut *current).process_in_place(stereo))` — the chain runs
  on the audio thread, edits swap in via the lock-free hand-off, **no alloc/locks** (ADR-0001). On
  teardown, `retire(current)` so the control thread reclaims it after `join`. `MeasuredLatency` gains
  the **aggregate chain latency** (`ChainProcessor::aggregate_latency`) added to the total.
  `AudioEngine::start_with_chain(in, out, initial: Box<ChainProcessor>)` and a
  `publish_chain(Box<ChainProcessor>)` for live edits. The `chain` subcommand: `load_module` each
  `.vst3` (M1), `PluginInstance::from_factory`, build a `ChainProcessor`, `start_with_chain`, run until
  Enter.
- **Change:** wire the chain + hand-off into `RunningState`/the render loop; add `start_with_chain`,
  `publish_chain`, aggregate-latency reporting; the `chain` subcommand + a `#[cfg(not(windows))]`
  "Windows-only" stub; README note.
- **Verify:** **`make host-windows-check`** cross-compiles the engine + subcommand. The **automated**
  proof is Steps 3–5 (the same `ChainProcessor`/`Handoff`/`render_through` the engine drives). The
  **live** "mic → ≥2 real VST3 plugins → output, glitch-free; per-slot bypass = identity; edits without
  dropouts; audio thread alloc/lock-free" is the Windows-runtime field check (`galad chain <in> <out>
  Cenedril.vst3 <third-party>.vst3`), **not** a manual `make ci` gate. **Red:** greenfield (the engine
  API/subcommand don't resolve); **green:** cross-compiles and `cargo test -p galad` stays green.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| (phase) | Lock-free hand-off mechanism. | **Resolved (mine):** hand-rolled `AtomicPtr` pending/retired — reclamation guaranteed off the audio thread; no `arc-swap` dep. |
| (phase) | Where/how the chain runs. | **Resolved (mine):** in the render pump, in-place stereo, planar ping-pong, preallocated (alloc-free); `unsafe impl Send` for the handed-off graph. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. **No `[DECISION]` stops** (the plan lists
none; the internal choices are resolved above). Steps 1–5 are verified automatically by `cargo test -p
galad` on Linux (including `assert_no_allocations` for the ADR-0001 path and the lock-free hand-off
protocol); Step 6 is cross-compile-verified by `make host-windows-check`, with the live ≥2-plugin chain
recorded as a Windows field check. M3 done means mic → a chain of real VST3 plugins → output with
lock-free edits — the core host is functional; **M4** (parameter + state bridging, session persistence)
follows. One milestone at a time.
