# Galad M4 — Step Plan

Expansion of **M4** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: **make plugin state and the whole session durable** — capture/restore each plugin's
opaque state (`IComponent` get/setState), persist the session (devices + ordered plugin list +
per-plugin state + bypass) to disk, and restore it on launch.

**Source-of-truth & reference (re-derive from these, not memory):**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) M4 + "Context / reuse map".
- **Reuse (M0):** `host/src/session.rs` already defines the **serializable** session model —
  `HostSession { input/output: Option<DeviceRef>, chain: Vec<ChainSlot>, settings }`,
  `ChainSlot { plugin_path, bypassed, state: Option<PluginStateBlob> }`,
  `PluginStateBlob { format_version, payload: Vec<u8> }` (base64), and `to_toml_string`/`from_toml_str`
  with a versioned envelope. M4 adds **disk I/O** + **real state capture** + **restore into a chain**.
- **Reuse (M1):** `PluginInstance`/`component()` (`instance.rs`), `load_module` (`module.rs`), the
  `#[cfg(test)]` fixture (`fixture.rs`).
- **Reuse (M3):** `ChainProcessor` (`chain.rs`).
- **`vst3` API:** `IBStreamTrait { read, write, seek, tell }` (seek modes `kIBSeekSet=0`,
  `kIBSeekCur=1`, `kIBSeekEnd=2`); `IComponent::getState(*mut IBStream)` / `setState(*mut IBStream)`
  (M1 fixture already stubs these). The host **implements** a memory `IBStream` (a `Class`) and hands
  it to the plugin's get/setState.
- **Persistence pattern:** `crates/lindelion-plugin-shell/src/patch_io.rs` `write_atomic` (temp file +
  rename) is the atomic-write reference.

## Verification strategy (same split as M1–M3)

The COM state path and the persistence are **platform-neutral** and tested **on Linux**
(`cargo test -p galad`) against the in-process fixture: a memory `IBStream`, `capture_state`/
`restore_state` round-trips, the session disk round-trip, and `capture_session` from fixture slots.
The **restore that loads real `.vst3` modules** (`restore_chain`) and the `session` subcommand are
**cross-compile-verified** (`make host-windows-check`); the live "save a session, restart, restore the
same plugins/order/state/devices" is the **Windows-runtime field check** — recorded on hardware, not a
`make ci` gate. `galad` stays excluded from `make ci`.

*Note on the disk test:* `galad` is excluded from `cargo test --workspace`, so a single tempfile-based
round-trip test (unique path + cleanup, run via `cargo test -p galad`) does not touch the in-memory
`make ci` suite or its determinism rule.

## Phase decision

- **[DECISION] Parameter mirror — do now, or defer to M6?** The plan's M4 line opens with "mirror
  plugin parameters host-side (read/automate where applicable)". **Recommend: defer to M6.** Rationale:
  the **opaque state** (get/setState) already makes the session fully durable — that is the M4 exit
  ("same plugins, order, state, devices") — and a parameter mirror has **no consumer until the M6 UI**
  (it needs `IEditController` instantiation + a fixture controller, which is UI/automation
  infrastructure, not session durability). Deferring is well-factored, not an MVP cut. Step 6 carries
  this decision; if you choose "do now", it expands into an `IEditController` read/automate mirror.

---

## Step 1 — Host memory `IBStream` (the stream the host hands to plugin get/setState)
- **File(s):** `host/src/vst3_host/bstream.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** A `Class` `MemoryStream` implementing `IBStreamTrait` over an in-memory
  buffer with a read/write cursor. Interior-mutable (COM methods take `&self`):
  `data: RefCell<Vec<u8>>`, `pos: Cell<i64>`. `read(buffer, numBytes, numBytesRead)` copies
  `min(numBytes, remaining)` from `data[pos..]` into `buffer`, advances `pos`, writes the count;
  `write(buffer, numBytes, numBytesWritten)` writes at `pos` (extending `data`), advances, writes the
  count; `seek(pos, mode, result)` sets the cursor per `kIBSeekSet/Cur/End` and reports the new
  position; `tell(pos)` reports the cursor. Constructors `new()` (empty) and `from_bytes(Vec<u8>)`;
  a `bytes()` accessor returning the buffer. Lint allows mirror `vst3_host` (FFI/unsafe).
- **Change:** add `bstream.rs` (`MemoryStream`, `Class`, `IBStreamTrait` impl, `new`/`from_bytes`/
  `bytes`); register the module.
- **Verify:** `cargo test -p galad`: wrap a `MemoryStream` (`ComWrapper`), get its `ComPtr<IBStream>`,
  `write` two byte runs, `seek(0, kIBSeekSet)`, `read` them back, assert the read bytes and `tell`
  match; assert `bytes()` equals what was written. **Red:** greenfield — `MemoryStream` absent.
  **Green:** the stream reads back what was written.

## Step 2 — Plugin state capture/restore + make the fixture round-trip state  [depends on #1]
- **File(s):** `host/src/vst3_host/state.rs` (new), `host/src/vst3_host/mod.rs` (register),
  `host/src/vst3_host/fixture.rs` (`#[cfg(test)]`, store/echo state).
- **Reference behavior:** `capture_state(component: &ComPtr<IComponent>) -> Vec<u8>` — make a
  `MemoryStream`, call `component.getState(stream_iface)`, return `stream.bytes()`.
  `restore_state(component: &ComPtr<IComponent>, bytes: &[u8])` — make a `MemoryStream::from_bytes`,
  call `component.setState(stream_iface)`. To make the round-trip observable, extend `FixtureProcessor`
  with `state: RefCell<Vec<u8>>`: `getState` **writes** its stored bytes to the stream (via
  `IBStream::write`), `setState` **reads** the stream into its stored bytes (via `IBStream::read`/
  `seek`/`tell`) — a faithful echo (was: both returned `kResultOk` doing nothing). `new(gain, latency)`
  starts with empty state.
- **Change:** add `state.rs` (`capture_state`, `restore_state`); rewrite the fixture's `getState`/
  `setState` to use the stream + the new `state` field.
- **Verify:** `cargo test -p galad`: build a fixture instance, `restore_state(component, b"abc123")`,
  then `capture_state(component) == b"abc123"`; round-trip it through a **fresh** fixture
  (`restore_state(fresh, captured)` → `capture_state(fresh)` equals the original). **Red:** greenfield —
  `capture_state`/`restore_state` and the echoing fixture state don't exist (a no-op `getState` returns
  empty, failing the equality). **Green:** state survives capture→restore→capture.

## Step 3 — Session disk save/load  [depends on M0 serialization]
- **File(s):** `host/src/session.rs`.
- **Reference behavior:** Add `HostSession::save(path)` and `HostSession::load(path)` over the existing
  `to_toml_string`/`from_toml_str` (M0). `save` writes **atomically** (temp file in the same dir +
  rename, mirroring `patch_io.rs::write_atomic`); `load` reads the file and parses. A small
  `SessionIoError` wraps `io::Error` + the existing `SessionError`.
- **Change:** add `save`/`load` + `SessionIoError`; reuse `to_toml_string`/`from_toml_str` unchanged.
- **Verify:** `cargo test -p galad` (tempfile; `galad` is outside `make ci`): build a `HostSession`
  with both devices set, two chain slots (one bypassed, one carrying a non-empty `PluginStateBlob`),
  `save` to a unique temp path, `load` it back, assert it equals the original (opaque bytes included),
  then remove the temp file. **Red:** greenfield — `save`/`load` absent. **Green:** the on-disk session
  round-trips exactly.

## Step 4 — Session capture + restore orchestration (chain ⇄ `HostSession`)  [depends on #2, #3]
- **File(s):** `host/src/vst3_host/session_runtime.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:**
  - `capture_session(slots, input, output, settings) -> HostSession` where each `slot` carries
    `{ plugin_path: PathBuf, component: &ComPtr<IComponent>, bypassed: bool }`: build a `ChainSlot`
    per slot with `plugin_path`, `bypassed`, and `state: Some(PluginStateBlob { format_version:
    SESSION_FORMAT_VERSION, payload: capture_state(component) })`; assemble the `HostSession`. (The
    caller — the `chain`/`session` subcommand — knows the paths; the chain itself does not store them.)
  - `restore_chain(session, host, sample_rate, max_frames) -> Result<(Vec<LoadedModule>,
    ChainProcessor), HostError>`: for each `ChainSlot`, `load_module(plugin_path)` (M1),
    `PluginInstance::from_factory`, `restore_state(instance.component(), &blob.payload)` when present,
    collect instances + bypass flags; build a `ChainProcessor::new(instances, bypass, sample_rate,
    max_frames)`; return the modules (kept alive) + chain. `load_module` runs at runtime only on Windows.
- **Change:** add `session_runtime.rs` with `SessionSlot`, `capture_session`, `restore_chain`.
- **Verify:** `cargo test -p galad` (Linux, fixtures as stand-ins for loaded plugins, with **no**
  `restore_chain` module load): build 2 fixture instances, give each a distinct state
  (`restore_state`), construct `SessionSlot`s (fake paths + bypass), `capture_session` → a `HostSession`;
  assert each `ChainSlot` has the right path/bypass and its `state.payload == capture_state(component)`.
  Then the **end-to-end data round-trip** (combining #2/#3): `capture_session` → `save` →
  `load` → for each restored `ChainSlot`, `restore_state` into a fresh fixture → `capture_state` equals
  the original. **Red:** greenfield — `capture_session` absent. **Green:** the captured session carries
  each plugin's state and round-trips through disk exactly. (`restore_chain`'s real-module path is
  cross-compile-verified in Step 5; its state-restore *logic* is proven here.)

## Step 5 — `session` save/restore subcommand (restart round-trip)  [depends on #4] [windows]
- **File(s):** `host/src/main.rs` (extend `chain` with `--save <file>`; add a `session <file>`
  subcommand, `#[cfg(windows)]`), `host/README.md` (note session persistence).
- **Reference behavior:** `galad chain <in> <out> <plugin.vst3>... [--save <file>]` — on exit,
  `capture_session` (paths from argv, devices from the `DeviceRef`s, state from the live components) and
  `HostSession::save(file)`. `galad session <file>` — `HostSession::load(file)`, `restore_chain` (Step 4;
  devices + plugins + order + state + bypass all from the file), `AudioEngine::start_with_chain`, run;
  keep the returned `LoadedModule`s alive past the engine (as the `chain` subcommand already does).
- **Change:** thread `--save` through the chain subcommand; add `run_session_command` (`#[cfg(windows)]`)
  + a `#[cfg(not(windows))]` "Windows-only" stub + the match arm + banner line; README note.
- **Verify:** **`make host-windows-check`** cross-compiles the subcommands. The **automated** proof of
  the exact round-trip is Step 4 (capture → save → load → restore-state). The **live** "save a session,
  relaunch `galad session <file>`, and get the same plugins/order/state/devices" is the Windows-runtime
  field check, **not** a `make ci` gate (no human-audition exit). **Red:** greenfield — the subcommand
  doesn't resolve; **green:** cross-compiles and `cargo test -p galad` stays green.

## Step 6 — Parameter mirror, or defer to M6  [DECISION]
- **Decision:** see the **Phase decision** above. **Recommended: defer to M6** — the session is already
  fully durable via opaque state (Steps 2–5 satisfy the M4 exit), and a parameter mirror has no consumer
  until the UI. If deferred, this step is a no-op (record the deferral in `GALAD-HOST-PLAN.md`'s
  backlog/M6 note).
- **If "do now":** instantiate the plugin's `IEditController` (via `IComponent::getControllerClassId` →
  `factory.createInstance`), `setComponentState` to sync, read `getParameterCount`/`getParameterInfo`/
  `getParamNormalized` into a host-side `ParameterMirror`, and support `setParamNormalized` (automate).
  Add a `#[cfg(test)]` fixture **controller** class (1–2 params) registered in the fixture factory, with
  `FixtureProcessor::getControllerClassId` returning its CID. **Verify (Linux):** read the mirror's
  param count/info/values from the fixture controller; set a param and read it back. **Red:** greenfield.
- The executor **stops here** for your call before doing or skipping this step.

---

## Decisions table
| Step | Decision | Status / recommendation |
| ---- | -------- | ----------------------- |
| 6 | Parameter mirror now, or defer to M6. | **Recommend defer to M6** — opaque state already makes the session durable (the exit); the mirror needs the UI as a consumer + a fixture controller. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. One `[DECISION]` stop remains (Step 6 —
parameter mirror vs defer). Steps 1–4 are verified automatically by `cargo test -p galad` on Linux
(memory `IBStream`, state capture/restore, disk round-trip, session capture); Step 5 is
cross-compile-verified by `make host-windows-check`, with the live restart round-trip recorded as a
Windows field check. M4 done makes the whole session durable; **M5** (plugin editor window hosting via
`IPlugView`/child `HWND`) follows. One milestone at a time.
