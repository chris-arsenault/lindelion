# Galad M6 — Step Plan

Expansion of **M6** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: the **host UI** tying everything together — input/output device pickers; the plugin
chain (scan folder, add/remove/reorder/bypass, open editor); input/output level + loudness meters
fed by an audio-thread snapshot; start/stop; session save/load. Every host capability is driveable
from the UI, and **meters never block or read on the audio thread**.

**UI framework: Vizia (winit standalone)** — per **[ADR-0024](docs/adr/0024-galad-ui-vizia.md)**, which
supersedes ADR-0022's original `egui` choice (re-evaluated 2026-06-01: Vizia's `winit` backend does
standalone Windows apps via skia, already cross-compiled here; keeping `egui` would run two UI
frameworks on Windows). One UI framework across the project — plugins use Vizia/baseview, the host uses
Vizia/winit.

**Source-of-truth & reference:**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) M6 + "Context / reuse map" (*reuse:* `lindelion-dsp-utils`
  metering for the UI meters; *build new:* the egui→**Vizia** host UI + the lock-free meter snapshot).
- [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md): the audio thread computes + publishes the
  meter snapshot lock-free and allocation-free; the UI reads it off-thread.
- **Reuse:** `crates/lindelion-dsp-utils/src/analysis.rs` — `peak_abs(&[f32]) -> f32`, `rms(&[f32]) ->
  f32` (level/loudness; a true ITU-R LUFS meter is a later dsp-utils addition). M2 `Transport`/M3 engine
  (where the meter is computed), M3 `ChainProcessor`/`Handoff` (live chain edits), M4
  `capture_session`/`restore_chain` + `HostSession::save`/`load`, M5 `EditorHost` (open editor),
  `session::DeviceRef`.
- **Vizia:** `vizia_winit::Application::new(|cx| …).run()` (standalone entry, Windows via skia);
  `crates/lindelion-ui/src/{vizia_controls,glirdir_vizia,linnod_vizia}.rs` for the Model/Lens/View +
  event patterns to mirror (host views are new code — `lindelion-ui` is baseview/plugin-bound).

## Resolved decisions (mine — internal; the framework `[DECISION]` is settled by ADR-0024)

- **Meter publish = a lock-free single-writer/single-reader latest-snapshot (seqlock)** — the audio
  thread (writer) is wait-free; the UI (reader) retries on a version change. A small `Copy`
  `MeterSnapshot`, no allocation. (Not the M3 graph `Handoff`, which is for pointers.)
- **The UI state + commands are framework-neutral** (`HostUiState` + a `UiCommand` enum), tested on
  Linux; the **Vizia Model wraps the neutral state** and the views emit `UiCommand`s. Keeps the bulk of
  M6 testable without Vizia and the framework swappable.
- **Plugin editors stay separate native windows** (M5 `EditorHost`), opened from the chain list — the
  Vizia window is the control surface, per ADR-0024.

## Verification strategy

The framework-neutral core — meter computation, the lock-free meter snapshot, the UI state model, and
the command/transition logic (Steps 1–4) — compiles and is **tested on Linux** (`cargo test -p galad`,
incl. `assert_no_allocations` for the audio-thread publish). The engine meter-wiring + the **Vizia
Model/views/app** (Steps 5–7) are **cross-compile-verified** (`make host-windows-check`); the live UI —
*every capability driveable, meters accurate and never blocking the audio thread* — is the
**Windows-runtime field check**, not a `make ci` gate. `galad` stays excluded from `make ci`.

---

## Step 1 — Meter computation (level + loudness from a block)
- **File(s):** `host/Cargo.toml` (add `lindelion-dsp-utils.workspace = true`), `host/src/audio/meter.rs`
  (new), `host/src/audio/mod.rs` (register).
- **Reference behavior:** `MeterSnapshot { input_peak, input_rms, output_peak, output_rms: f32 }`
  (`#[derive(Clone, Copy, Default)]`). A `fn levels(block: &[f32]) -> (f32, f32)` returning
  `(peak_abs(block), rms(block))` via `lindelion-dsp-utils` (`peak_abs`/`rms`); the engine fills the
  input pair from the captured block and the output pair from the post-chain block. Cheap +
  allocation-free (it is called on the audio thread). RMS stands in for "loudness" until a true LUFS
  meter lands in dsp-utils.
- **Change:** add `meter.rs` with `MeterSnapshot` + `levels`; add the dep.
- **Verify:** `cargo test -p galad`: `levels` of a known block returns the expected peak (max |x|) and
  rms (√mean(x²)) within tolerance; `levels` of silence is `(0, 0)`. **Red:** greenfield — `MeterSnapshot`/
  `levels` absent. **Green:** the computed peak/rms match.

## Step 2 — Lock-free meter snapshot (audio→UI), allocation-free  [depends on #1]
- **File(s):** `host/src/audio/meter.rs` (extend), `host/src/audio/mod.rs` (re-export).
- **Reference behavior:** A single-writer/single-reader **seqlock** over a `MeterSnapshot`:
  `MeterPublisher::publish(&self, MeterSnapshot)` (writer, wait-free: bump an `AtomicU32` version to
  odd `Release`, write the snapshot, bump to even `Release`) and `MeterReader::read(&self) ->
  MeterSnapshot` (reader: read version `Acquire`, copy, read version again — retry while odd or
  changed). Share via `Arc`; `MeterPublisher`/`MeterReader` are two handles over one `Arc<MeterCell>`.
  Allocation-free on both sides; the writer never blocks (ADR-0001).
- **Change:** add `MeterCell` (version + `UnsafeCell<MeterSnapshot>`, `unsafe impl Sync`), `MeterPublisher`,
  `MeterReader`, and `meter_channel() -> (MeterPublisher, MeterReader)`.
- **Verify:** `cargo test -p galad`: publish a snapshot, `read()` returns it exactly; publishing a second
  overwrites; `assert_no_allocations("meter publish", || publisher.publish(snap))`. **Red:** greenfield.
  **Green:** the latest snapshot round-trips and publish allocates nothing.

## Step 3 — `HostUiState`: the framework-neutral UI model + transitions
- **File(s):** `host/src/ui/mod.rs` (new), `host/src/ui/state.rs` (new), `host/src/main.rs`
  (`mod ui;`).
- **Reference behavior:** `HostUiState { inputs: Vec<DeviceRef>, outputs: Vec<DeviceRef>, selected_input:
  Option<DeviceRef>, selected_output: Option<DeviceRef>, chain: Vec<UiSlot>, scan_dirs: Vec<PathBuf>,
  running: bool, meter: MeterSnapshot }` with `UiSlot { path: PathBuf, name: String, bypassed: bool }`.
  Pure transitions: `select_input`/`select_output`, `add_slot(path)`, `remove_slot(i)`, `move_slot(i,
  dir)` (reorder with bounds checks), `toggle_bypass(i)`, `set_devices(inputs, outputs)`,
  `set_meter(snap)`, `scan_dir(dir) -> Vec<PathBuf>` (list `*.vst3` under a folder). Reuses
  `crate::session::DeviceRef`; convertible to/from `HostSession`'s `chain` (paths + bypass) for save/load.
- **Change:** add the `ui` module + `state.rs` with `HostUiState`, `UiSlot`, and the transitions.
- **Verify:** `cargo test -p galad`: `add_slot` then `move_slot(1, Up)` reorders; `remove_slot` shrinks +
  reindexes; `toggle_bypass` flips; `move_slot` at the ends is a no-op; round-trip `HostUiState.chain` ⇄
  `HostSession.chain` preserves path+bypass. **Red:** greenfield. **Green:** the transitions hold.

## Step 4 — `UiCommand` model + apply-to-state  [depends on #3]
- **File(s):** `host/src/ui/command.rs` (new), `host/src/ui/mod.rs` (register).
- **Reference behavior:** A `UiCommand` enum covering every UI action — `SelectInput(DeviceRef)`,
  `SelectOutput(DeviceRef)`, `AddPlugin(PathBuf)`, `RemovePlugin(usize)`, `MoveSlot(usize, Dir)`,
  `ToggleBypass(usize)`, `Start`, `Stop`, `OpenEditor(usize)`, `SaveSession(PathBuf)`,
  `LoadSession(PathBuf)`. A pure `apply(state: &mut HostUiState, cmd: &UiCommand)` performs the
  **state** part of each command (device selection, chain edits, `running` flag); the **effectful**
  part (start/stop the engine, open an editor, save/load to disk + rebuild the chain) is performed by
  the Windows controller (Step 7), which calls `apply` for the state and then acts. This keeps the
  command→state mapping testable without the engine.
- **Change:** add `command.rs` with `UiCommand`, `Dir`, and `apply`.
- **Verify:** `cargo test -p galad`: `apply(&mut state, &AddPlugin(p))` then `&ToggleBypass(0)` leaves the
  expected chain; `&Start`/`&Stop` set/clear `running`; `&MoveSlot(0, Down)` reorders. **Red:** greenfield.
  **Green:** each command maps to the right state.

## Step 5 — Publish meters from the engine's audio thread  [depends on #2] [windows]
- **File(s):** `host/src/audio/wasapi/engine.rs`.
- **Reference behavior:** `AudioEngine` gains a `MeterPublisher` used on the audio thread and exposes a
  `MeterReader` to the UI (`AudioEngine::meter_reader()`). In the render pump, after the chain runs,
  compute output `levels` (Step 1) from the post-chain stereo block; in capture, compute input `levels`
  from the captured block; combine into a `MeterSnapshot` and `publish` it — **no allocation, no locks**
  (ADR-0001). `start`/`start_with_chain` create the meter channel and stash the reader.
- **Change:** thread a `MeterPublisher` into `RunningState`/the pumps; compute + publish; add
  `meter_reader()`.
- **Verify:** **`make host-windows-check`** cross-compiles. The **automated** proof of the meter math +
  the lock-free publish is Steps 1–2; the live meter values are read in the Step 7 field check. **Red:**
  greenfield (the engine API doesn't resolve). **Green:** cross-compiles; `cargo test -p galad` green.

## Step 6 — Vizia Model + views (pickers, chain list, meters, controls)  [depends on #3, #4] [windows]
- **File(s):** `host/Cargo.toml` (target-gated `vizia` with the `winit` feature),
  `host/src/ui/vizia_app.rs` (new, `#[cfg(windows)]` — Model + views), `host/src/ui/mod.rs`
  (`#[cfg(windows)] mod vizia_app;`).
- **Reference behavior:** A Vizia `Model` wrapping `HostUiState`, mirroring the Model/Lens/View + event
  patterns in `crates/lindelion-ui/src/{vizia_controls,glirdir_vizia}.rs` (host views are new code).
  Views: **device pickers** (two dropdowns bound to `inputs`/`outputs`, emitting `SelectInput`/
  `SelectOutput`); the **chain list** (a row per `UiSlot` with a bypass toggle, up/down, remove, and an
  "open editor" button → `ToggleBypass`/`MoveSlot`/`RemovePlugin`/`OpenEditor`; an "add plugin" control
  that scans a folder → `AddPlugin`); **meters** (input/output level + rms bars bound to the model's
  `meter`, dB-scaled); **start/stop** (→ `Start`/`Stop`, label reflects `running`); **session save/load**
  (file pickers → `SaveSession`/`LoadSession`). Each view emits a `UiCommand` (Step 4) via a Vizia event
  the Model handles. Add the `vizia` dep with `features = ["winit"]`, target-gated Windows.
- **Change:** add the dep; `vizia_app.rs` with the Model + the view tree + the `UiCommand` event plumbing.
- **Verify:** **`make host-windows-check`** cross-compiles (vizia/winit/skia for `x86_64-pc-windows-msvc`
  via cargo-xwin — reuse the existing skia link case-fix). **Red:** greenfield. **Green:** the Vizia view
  tree cross-compiles. (Widget behaviour is the Step 7 field check.)

## Step 7 — Vizia `Application` + integration + entry point  [depends on #5, #6] [windows]
- **File(s):** `host/src/ui/vizia_app.rs` (the `Application` + controller), `host/src/main.rs` (launch
  the UI when run with no subcommand, plus a `ui` alias; `#[cfg(not(windows))]` message), `host/README.md`
  (note the UI).
- **Reference behavior:** `vizia_winit::Application::new(|cx| { build the Model + views })`, sized and
  titled, `.run()`. A **controller** owns the `AudioEngine` (M2/M3), the `EditorHost` (M5), and the
  loaded `LoadedModule`s, and executes `UiCommand`s: `Start`/`Stop` start/stop the engine with the
  current chain (rebuilt from `HostUiState.chain` via M3 + the modules kept alive); chain edits republish
  the chain through the M3 `Handoff` while running (no dropouts); `OpenEditor` opens the plugin's editor
  via `EditorHost`; `SaveSession`/`LoadSession` use M4 `capture_session`+`save` / `load`+`restore_chain`;
  device selection enumerates via M2. A Vizia **timer** ticks each frame: `engine.meter_reader().read()`
  → `set_meter` on the Model (so meters update **off** the audio thread). On non-Windows, `main` prints
  that the UI is Windows-only.
- **Change:** add the `Application` + controller + the meter timer; wire the default-launch + README note.
- **Verify:** **`make host-windows-check`** cross-compiles the app + entry. The **exit** — *every host
  capability driveable from the UI; meters accurate and never block/read on the audio thread; chain edits
  apply live without dropouts* — is the Windows-runtime field check (`galad` with no args), **not** a
  `make ci` gate. **Red:** greenfield. **Green:** cross-compiles and `cargo test -p galad` stays green.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| (phase) | UI framework: egui vs Vizia. | **Resolved by you (2026-06-01) → Vizia (winit standalone)**; recorded in [ADR-0024](docs/adr/0024-galad-ui-vizia.md), superseding ADR-0022's egui bullet. |
| (phase) | Meter audio→UI transport. | **Resolved (mine):** lock-free seqlock latest-snapshot (wait-free writer); framework-neutral, Linux-tested. |
| (phase) | UI state/commands location. | **Resolved (mine):** framework-neutral `HostUiState`+`UiCommand` (Linux-tested); the Vizia Model wraps them. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. **No `[DECISION]` stops remain** — the
framework choice is settled (ADR-0024) and the internal choices are resolved above. Steps 1–4 are
verified automatically by `cargo test -p galad` on Linux (meter math, the lock-free no-alloc snapshot,
the UI state/command model); Steps 5–7 are cross-compile-verified by `make host-windows-check`, with the
live UI recorded as the Windows field check. M6 completes the host: device pickers + chain editor +
meters + start/stop + session save/load, all driveable from a Vizia window. **M7** (robustness,
device hot-swap, plugin matrix, soak) follows. One milestone at a time.
