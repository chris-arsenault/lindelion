# Lúmedir — M4 — Execution Steps

Step-level expansion of **M4** from [`LUMEDIR-VST-PLAN.md`](LUMEDIR-VST-PLAN.md): *Vizia editor —
live running readout.* Depends on **M1/M2/M3** (the estimators and the off-thread
`DeliveryWorker`/`latest_delivery()`).

> The running-readout view (on Cenedril's Vizia stack): rate/WPM, pitch-dynamism, pause, clarity —
> live gauges updating from snapshots off the audio thread.
> **Exit:** the live readout renders in a Windows host and updates correctly from the snapshot stream.

Run these in order. Each step names its file(s), the reference behavior to re-derive from a
source-of-truth (not memory), the minimal change, and a red→green test.

## What M4 builds (the telemetry channel, first for the new-VST stack)

M3 produces `Lumedir::latest_delivery()` on the **processor** side. The editor lives on the
**controller** side — a separate COM object. So M4 must carry the snapshot processor → editor. The
repo already has this pattern: **Glirdir's `TelemetryRequest`/`TelemetryResponse` over
`IConnectionPoint`** (`plugins/glirdir/src/vst3_entry/{messages,processor,controller,editor}.rs`,
`crates/lindelion-ui/src/glirdir_vizia*`). M4 reuses it verbatim, shrunk to one payload:

```
editor (Vizia, Windows)
  └─ polls host.request_delivery() on a timer, reads host.delivery()
controller (IConnectionPoint)
  ├─ request_delivery() ─ notify_peer(TelemetryRequest) ─▶ processor
  └─ notify(TelemetryResponse(payload)) ─ stores Cell<LumedirDeliveryPayload>
processor (IConnectionPoint, owns the DeliveryWorker)
  └─ notify(TelemetryRequest) ─ reads latest_delivery() ─ notify_peer(TelemetryResponse(payload))
```

**CI shape (ADR-0023 / plan):** the **message codec, the view-model formatting, and the host-callback
routing are cross-platform and `make ci`-tested on Linux**; the COM wiring (`IConnectionPoint`,
`notify`) **compiles** on Linux (the whole `vst3_entry` does, per the Cenedril scaffold) and is
behaviour-verified on Windows; the **Vizia gauge rendering + poll timer** are `cfg(windows)` and
verified by the Windows build + host. So most steps have a `make ci` codec/logic test; the view
itself (Step 6) is the Windows-runtime exit.

**No user `[DECISION]` in this phase.** The telemetry mechanism is established (Glirdir); the gauge
visual treatment (numeric labels + horizontal meter bars, mirroring Cenedril's meter style) and the
value→fill mappings are internal presentation choices, mine to set (target-band colouring is M5).

## Context / reuse map (re-derived for M4)

- *Reuse as-is:* `lindelion_plugin_shell::vst3` — `define_vst3_plugin_messages!` (enum + codec),
  `PluginMessagePayload` (byte encode/decode), `Vst3PeerConnection` (`peer`/`connect`/`disconnect`/
  `notify`/`notify_typed`). **Glirdir is the line-by-line template**: `GlirdirStatusPayload`
  (`messages.rs`) for the payload codec; the processor's `IConnectionPoint`/`notify` reply
  (`processor.rs` ~360–410); the controller's `IConnectionPoint`/`notify` store +
  `request_status`/`status` (`controller.rs`); the `GlirdirEditorCallbacks` vtable +
  `GlirdirEditorHost` (`crates/lindelion-ui/src/glirdir_vizia.rs` ~300–330) and how `editor.rs`
  fills it; the Vizia `platform_state.rs` poll-and-render. Cenedril's M0/M1 `FixedSizePlugView` +
  `IPlugView`→`HWND` attach (already in `lumedir_vizia/platform.rs`).
- *The data:* `crate::delivery::DeliverySnapshot` (M3) — `syllables_per_second`, `words_per_minute`,
  `pitch_dynamism_semitones`, `pause_fraction`, `pause_count`, `clarity`. The editor-facing neutral
  mirror lives in `lindelion-ui` (which cannot depend on `plugins/lumedir`), exactly as
  `GlirdirEditorStatus` mirrors `GlirdirStatusPayload`.
- *Source-of-truth ADRs:* [ADR-0023](docs/adr/0023-new-vsts-windows-only.md) (Windows-only Vizia
  editor on `lindelion-ui`; `IPlugView`→`HWND` is the one Windows-gated editor piece);
  [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) — `notify` runs on the host message
  thread, not the audio thread; it reads `latest_delivery()` (a lock-free atomic load), never the
  audio path.

---

## 1. Add the neutral editor readout + the view-model formatting

- **File(s):** `crates/lindelion-ui/src/lumedir_vizia.rs`.
- **Reference behavior:** the editor needs a platform-neutral mirror of the delivery snapshot and a
  pure function turning it into display values (Glirdir's `GlirdirEditorStatus` + its label/format
  helpers are the model). The view-model maps each metric to a label string and, where a gauge bar
  is shown, a normalized fill in [0, 1]: rate as "N.N syl/s", WPM as "NNN wpm", dynamism as "N.N st",
  pause as "NN%", clarity as a [0,1] fill / "NN%". Fill mappings use sane display ranges (e.g. rate
  0–7 syl/s, dynamism 0–8 st) clamped to [0, 1].
- **Change:** add `#[derive(Clone, Copy, Default)] LumedirEditorReadout { syllables_per_second,
  words_per_minute, pitch_dynamism_semitones, pause_fraction, pause_count, clarity }` and a
  view-model (e.g. `impl LumedirEditorReadout { fn rate_label(&self) -> String; fn wpm_label(&self);
  fn dynamism_label(&self); fn pause_label(&self); fn clarity_fill(&self) -> f32; … }`) — pure,
  cross-platform, no `vizia`.
- **Verify:** a `make ci` unit test: a known `LumedirEditorReadout` formats to the expected labels
  and the fills land in [0, 1] (and at the expected fraction for a mid-range value). **Red:**
  `LumedirEditorReadout`/the view-model don't exist → won't compile. **Green:**
  `cargo test -p lindelion-ui`.

---

## 2. Add the editor host callback vtable for delivery  [depends on #1]

- **File(s):** `crates/lindelion-ui/src/lumedir_vizia.rs`.
- **Reference behavior:** the Vizia editor reaches the controller through a vtable of `unsafe fn`
  pointers over a `usize` context (Glirdir's `GlirdirEditorCallbacks` + `GlirdirEditorHost`). For
  M4 the editor needs to **read** the latest readout and **request** a refresh: `delivery: unsafe
  fn(usize) -> LumedirEditorReadout` and `request_delivery: unsafe fn(usize)`.
- **Change:** add `#[derive(Clone, Copy)] LumedirEditorCallbacks { delivery, request_delivery }` and
  extend `LumedirEditorHost` to carry a `context: usize` + the callbacks (keep `new(controller)` for
  the M0 placeholder by defaulting the callbacks to no-ops, or add `with_callbacks`), plus
  `delivery(&self) -> LumedirEditorReadout` / `request_delivery(&self)` wrappers that invoke them.
- **Verify:** a `make ci` unit test: build a host whose callbacks read/increment a test-local
  counter through the `usize` context, and assert `host.delivery()` returns the stub readout and
  `host.request_delivery()` invokes the callback. **Red:** the callbacks/host fields don't exist →
  won't compile. **Green:** `cargo test -p lindelion-ui`.

---

## 3. Define the delivery telemetry message + payload codec  [depends on M3]

- **File(s):** `plugins/lumedir/src/vst3_entry/messages.rs` (new), `plugins/lumedir/src/vst3_entry/mod.rs`
  (add `mod messages;` + re-exports), `plugins/lumedir/src/vst3_entry/tests.rs` (new).
- **Reference behavior:** `plugins/glirdir/src/vst3_entry/messages.rs` — `define_vst3_plugin_messages!`
  generates the enum + COM codec; a payload struct implements `PluginMessagePayload` (byte
  encode/decode). Lúmedir needs one request and one response: `TelemetryRequest` (empty) and
  `TelemetryResponse(LumedirDeliveryPayload)`, where `LumedirDeliveryPayload` carries the six
  `DeliverySnapshot` fields.
- **Change:** add `messages.rs` with `define_vst3_plugin_messages! { … prefix "lindelion.lumedir.";
  messages { empty { TelemetryRequest => "telemetry_request" } payload { TelemetryResponse(LumedirDeliveryPayload) => "telemetry_response" } } }`,
  the `LumedirDeliveryPayload` struct (+ `Default`, a fixed-width little-endian `encode`/`decode`,
  and `impl PluginMessagePayload`). Register the module.
- **Verify:** a `make ci` roundtrip test in `tests.rs` (template Glirdir's
  `plugin_message_roundtrips_*`): `decode(encode(msg)) == msg` for `TelemetryRequest` and a populated
  `TelemetryResponse`, and `LumedirDeliveryPayload` byte-roundtrips. **Red:** the enum/payload don't
  exist → won't compile. **Green:** `cargo test -p lumedir`.

---

## 4. Reply to telemetry requests from the processor  [depends on #3]

- **File(s):** `plugins/lumedir/src/vst3_entry/processor.rs`.
- **Reference behavior:** `plugins/glirdir/src/vst3_entry/processor.rs` — the processor adds
  `IConnectionPoint` to its `Interfaces`, holds a `Vst3PeerConnection`, and in `notify` handles a
  request by reading plugin state and replying via `peer.notify_typed(...)`. `notify` runs on the
  host message thread, so it may borrow the plugin (`try_borrow` — `process` borrows it mutably on
  the audio thread) and call `latest_delivery()` (a lock-free read). The snapshot → wire bridge is a
  pure `LumedirDeliveryPayload::from_delivery(&DeliverySnapshot)`.
- **Change:** add `IConnectionPoint` to `LumedirVst3Processor::Interfaces`; add `peer:
  Vst3PeerConnection`; implement `connect`/`disconnect`/`notify`; on `TelemetryRequest`,
  `try_borrow` the plugin, build `LumedirDeliveryPayload::from_delivery(&plugin.latest_delivery())`,
  and `peer.notify_typed(TelemetryResponse(payload))`. Add the pure `from_delivery` bridge (in
  `messages.rs` or `processor.rs`).
- **Verify:** a `make ci` unit test of the pure bridge: `LumedirDeliveryPayload::from_delivery(&snap)`
  copies every field. (The COM `notify` reply is compile-checked here and behaviour-verified on
  Windows in Step 6.) **Red:** `from_delivery` doesn't exist → won't compile. **Green:**
  `cargo test -p lumedir`.

---

## 5. Receive telemetry in the controller + bridge the editor host  [depends on #2, #4]

- **File(s):** `plugins/lumedir/src/vst3_entry/controller.rs`, `plugins/lumedir/src/vst3_entry/editor.rs`.
- **Reference behavior:** `plugins/glirdir/src/vst3_entry/controller.rs` — the controller adds
  `IConnectionPoint`, holds a `Vst3PeerConnection` + a `Cell<…Payload>`, in `notify` stores a
  `TelemetryResponse`, and exposes `request_status` (`notify_peer(StatusRequest)`) + `status`
  (read the cell). `editor.rs` builds the `…EditorHost` with callbacks bound to the controller
  pointer. Lúmedir's analogue: store `Cell<LumedirDeliveryPayload>`; `request_delivery()` =
  `notify_peer(TelemetryRequest)`; `delivery()` = read the cell → `LumedirEditorReadout`; the
  editor host's `delivery`/`request_delivery` callbacks cast `usize` → `&LumedirVst3Controller` and
  call those.
- **Change:** add `IConnectionPoint` to `LumedirVst3Controller::Interfaces`; add `peer:
  Vst3PeerConnection` + `delivery: Cell<LumedirDeliveryPayload>`; implement `connect`/`disconnect`/
  `notify` (store `TelemetryResponse`); add `request_delivery()` + `delivery() ->
  LumedirEditorReadout`. In `editor.rs`, construct `LumedirEditorHost` with the
  `LumedirEditorCallbacks` bound to the controller pointer (passed into the `attach` host, replacing
  the M0 placeholder host).
- **Verify:** a `make ci` unit test: store a populated `LumedirDeliveryPayload` into a freshly
  built controller (via the same path `notify` uses, or a small helper) and assert `delivery()`
  returns the matching `LumedirEditorReadout`. **Red:** the controller's `delivery`/`request_delivery`
  don't exist → won't compile. **Green:** `cargo test -p lumedir`.

---

## 6. Render the live running-readout Vizia view (Windows)  [depends on #1, #5]

- **File(s):** `crates/lindelion-ui/src/lumedir_vizia/platform.rs`.
- **Reference behavior:** `crates/lindelion-ui/src/glirdir_vizia/platform_state.rs` — the Vizia app
  holds the host, polls `host.request_delivery()` on a timer (Vizia timer / `cx.spawn` tick),
  reads `host.delivery()`, sets a `LumedirEditorReadout` signal, and the view renders from the
  view-model (Step 1). Replace Cenedril's static placeholder with the readout: a labelled row /
  gauge bar per metric (rate-WPM, pitch-dynamism, pause, clarity) using the Step-1 labels and fills.
  This is the one `cfg(windows)` editor piece.
- **Change:** rebuild `build_lumedir_application` / `build_placeholder_view` into a live readout: a
  `LumedirEditorReadout` model signal, a periodic poll that calls `request_delivery()` then
  `delivery()` and updates the signal, and a view binding labels + meter bars to the view-model.
- **Verify:** the **phase exit** — a Windows-runtime check (not `make ci`): `make build-windows`
  produces `Lumedir.vst3`; loaded in the Galad host / a Windows DAW with speech playing, the readout
  **renders and updates** — rate/WPM, dynamism, pause, clarity track the input live. (`make ci` stays
  green: the cross-platform codec/view-model/bridge tests from Steps 1–5; the `cfg(windows)` view is
  not built on Linux.)

---

## Phase exit checklist

- [ ] `make ci` green (view-model formatting, host-callback routing, message roundtrip, the
      `from_delivery` bridge, and the controller store/read all pass on Linux; the `vst3_entry` COM
      wiring compiles).
- [ ] `make build-windows` builds `Lumedir.vst3`; in the Galad host / a Windows DAW the live readout
      **renders and updates** from the snapshot stream (rate/WPM, pitch-dynamism, pause, clarity).

Then expand **M5** (session summary + target-band scoring) with `plan-phase`.
