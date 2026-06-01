# Galad M5 — Step Plan

Expansion of **M5** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: **host plugins' native editor windows** — create/attach/resize/close a plugin
`IPlugView` in a child `HWND`, focus handling, multiple open editors. This is how Galad lets you edit
plugin parameters (the host keeps **no** parameter mirror — decided in M4); it opens each plugin's
**own** editor.

**Source-of-truth & reference (re-derive from these, not memory):**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) M5 + "Context / reuse map" (*build new:* the `IPlugView`
  host-attach). [ADR-0022](docs/adr/0022-windows-vst3-host.md) (Windows host; `egui` UI is M6 — M5's
  editor windows are raw Win32 for now).
- **Reuse (M1/M4):** `PluginInstance` (`instance.rs`) gives the `IComponent`; `load_module`
  (`module.rs`); the `#[cfg(test)]` fixture (`fixture.rs`); `capture_state`/the host `MemoryStream`
  (`bstream.rs`) for `setComponentState`.
- **`vst3` API:** `IComponent::getControllerClassId(*mut TUID)`; `IEditController::createView(name:
  FIDString) -> *mut IPlugView` with `ViewType::kEditor` (`b"editor"`); `IEditController::
  setComponentState(*mut IBStream)`; `IConnectionPoint::{connect, disconnect, notify}`; `IPlugView`
  (`isPlatformTypeSupported(FIDString)`, `attached(*mut c_void, FIDString)`, `removed()`,
  `getSize(*mut ViewRect)`, `onSize(*mut ViewRect)`, `setFrame(*mut IPlugFrame)`, `canResize()`,
  `onFocus(TBool)`); `IPlugFrame::resizeView(*mut IPlugView, *mut ViewRect)`; `ViewRect { left, top,
  right, bottom: i32 }`; `kPlatformTypeHWND` (`b"HWND"`).

## Resolved decisions (mine — internal implementation, no user-facing `[DECISION]`)

- **Raw Win32 editor window via the `windows` crate** (not baseview/winit) — consistent with the M2
  WASAPI approach (no extra windowing dep); add `Win32_UI_WindowsAndMessaging` (+ keyboard/GDI as the
  cross-compiler requires). Each editor is a standalone top-level host window the plugin's `IPlugView`
  attaches into; M6 integrates editor management into the `egui` UI.
- **Component↔controller connection is direct best-effort** — query `IConnectionPoint` on both sides
  and `connect` them directly (sufficient for same-process). If a real plugin needs a marshalling
  connection proxy, that surfaces in the Windows field check; the host stays minimal.

## Verification strategy (M5 is the most GUI-bound phase)

The plugin-editor **COM protocol** is platform-neutral and tested **on Linux** (`cargo test -p galad`)
against a fixture controller + fixture `IPlugView`: controller instantiation, `createView`, the
`IPlugView` query/size/`setFrame` lifecycle, and the host `IPlugFrame::resizeView` callback (Steps
1–4). The **Win32 child-window creation + real `IPlugView::attached(hwnd)` + the message loop** are
inherently Windows-runtime; they are **cross-compile-verified** (`make host-windows-check`). The exit —
*"a hosted plugin's editor opens, edits parameters live (audible), resizes, and closes cleanly; opening/
closing an editor does not glitch audio"* — is the **Windows-runtime field check** (run `galad editor
<plugin.vst3>` on hardware), **not** a `make ci` gate (no human-audition exit). `galad` stays excluded
from `make ci`.

---

## Step 1 — Fixture editor controller + fixture `IPlugView` (the editor-protocol harness)
- **File(s):** `host/src/vst3_host/fixture.rs` (`#[cfg(test)]`).
- **Reference behavior:** Extend the fixture so it exposes an editor, the way a real plugin does:
  - Add a `FixtureController` `Class` implementing `IEditController` (mostly stubs — `getParameterCount`
    → 0, `getParameterInfo`/`getParamStringByValue`/… → `kResultFalse`/`kNotImplemented`,
    `setComponentState`/`setState`/`getState` → `kResultOk`, `IPluginBase::initialize`/`terminate` →
    `kResultOk`) whose `createView(name)` returns, for `b"editor"`, a `ComWrapper<FixtureView>` as
    `*mut IPlugView` (else null).
  - Add a `FixtureView` `Class` implementing `IPlugView`: `isPlatformTypeSupported(type)` →
    `kResultTrue` for `b"HWND"` else `kResultFalse`; `getSize(rect)` → a fixed `ViewRect` (e.g.
    320×240); `attached`/`removed`/`onSize`/`onFocus` → `kResultOk`; `setFrame` → store the frame ptr;
    `canResize` → `kResultTrue`; the rest → `kNotImplemented`.
  - Register the controller as a second class in `FixtureFactory` (`countClasses` → 2, `getClassInfo[1]`
    = `"Component Controller Class"` with a `FIXTURE_CONTROLLER_CID`; `createInstance` dispatches it);
    `FixtureProcessor::getControllerClassId` returns `FIXTURE_CONTROLLER_CID` (was `kNotImplemented`).
- **Change:** add the two fixture classes + the controller CID; extend the factory's class count /
  `getClassInfo`/`createInstance`; wire `getControllerClassId`.
- **Verify:** `cargo test -p galad`: the fixture factory now reports `countClasses() == 2`;
  `createInstance(FIXTURE_CONTROLLER_CID, IEditController::IID, …)` yields a non-null controller; a
  fixture instance's `getControllerClassId` returns `FIXTURE_CONTROLLER_CID`. **Red:** greenfield — the
  controller/view classes don't exist (the M1 `countClasses()==1` assertion is updated to 2). **Green:**
  the controller + view resolve. (Pre-existing fixture tests still pass.)

## Step 2 — `EditorController`: instantiate the controller, connect, `createView`  [depends on #1]
- **File(s):** `host/src/vst3_host/editor_controller.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** `EditorController::new(factory: &ComPtr<IPluginFactory>, component:
  &ComPtr<IComponent>, host: &ComPtr<IHostApplication>) -> Result<Self, HostError>`:
  `component.getControllerClassId(&cid)`; `factory.createInstance(cid, IEditController::IID, …)` →
  `ComPtr<IEditController>`; `controller.initialize(host_funknown)`; **best-effort connect** — query
  `IConnectionPoint` on both `component` and `controller`; if both present,
  `component_cp.connect(controller_cp)` and `controller_cp.connect(component_cp)`; **sync** — capture
  the component's state (M4's `MemoryStream`) and `controller.setComponentState(stream)`. Hold the
  controller. `create_view(&self) -> Option<ComPtr<IPlugView>>` calls `createView(ViewType::kEditor)`
  and wraps the non-null result. (`HostError` gains a `NoController` variant.) Platform-neutral COM.
- **Change:** add `editor_controller.rs`; register + re-export `EditorController`.
- **Verify:** `cargo test -p galad` (fixture): build a fixture `PluginInstance` + the fixture factory +
  a `HostContext`; `EditorController::new(...)` is `Ok`; `create_view()` returns `Some` (a non-null
  `IPlugView`). **Red:** greenfield. **Green:** the controller instantiates and yields a view.

## Step 3 — Host `IPlugFrame` (the plugin's resize-request callback)  [depends on #1]
- **File(s):** `host/src/vst3_host/editor_frame.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** A `HostPlugFrame` `Class` implementing `IPlugFrame`. `resizeView(view,
  new_size)` records the requested `ViewRect` into shared interior-mutable state (`Cell<Option<(i32,
  i32)>>` width/height) and returns `kResultOk` — the actual `HWND` resize is applied by the Windows
  window code (Step 5), which reads the recorded request. Expose `new() -> ComWrapper<HostPlugFrame>`
  and a `take_requested_size()`/`requested_size()` accessor. Platform-neutral COM.
- **Change:** add `editor_frame.rs` (`HostPlugFrame`, `Class`, `IPlugFrameTrait`, the recorded-size
  state + accessor); register + re-export.
- **Verify:** `cargo test -p galad`: build a `HostPlugFrame`, get its `ComPtr<IPlugFrame>`, call
  `resizeView(null_view, &mut ViewRect{0,0,640,480})`, assert it returns `kResultOk` and
  `requested_size()` is `Some((640, 480))`. **Red:** greenfield — `HostPlugFrame` absent. **Green:** the
  resize request is recorded.

## Step 4 — Editor view attach-prep (platform-neutral `IPlugView` lifecycle)  [depends on #2, #3]
- **File(s):** `host/src/vst3_host/editor_view.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** The platform-neutral half of attaching: given a `ComPtr<IPlugView>` and a
  `ComPtr<IPlugFrame>`, `prepare(view, frame) -> Result<ViewRect, HostError>` checks
  `view.isPlatformTypeSupported(kPlatformTypeHWND) == kResultTrue` (else `HostError::EditorUnsupported`),
  reads `view.getSize(&mut rect)`, and `view.setFrame(frame_ptr)`; returns the initial `ViewRect`. (The
  real `attached(hwnd, …)` + window creation is Step 5.) `HostError` gains `EditorUnsupported`.
- **Change:** add `editor_view.rs` with `prepare`; register + re-export.
- **Verify:** `cargo test -p galad` (fixture): from an `EditorController::create_view()` (Step 2) +
  `HostPlugFrame` (Step 3), `prepare(...)` returns `Ok(rect)` with the fixture's 320×240 size; a stub
  view reporting `isPlatformTypeSupported != kResultTrue` yields `Err(EditorUnsupported)`. **Red:**
  greenfield. **Green:** the prepared rect matches and the unsupported case errors.

## Step 5 — Win32 child window + real `IPlugView::attached`  [depends on #4] [windows]
- **File(s):** `host/Cargo.toml` (add `Win32_UI_WindowsAndMessaging` etc. to the `windows` features),
  `host/src/vst3_host/editor_window.rs` (new, `#[cfg(windows)]`), `host/src/vst3_host/mod.rs`
  (`#[cfg(windows)] mod editor_window;`).
- **Reference behavior:** `EditorWindow::open(view, frame, title) -> Result<Self, HostError>`: register
  a window class (once), `CreateWindowExW` a top-level host window sized to the `prepare` rect (Step 4),
  `view.attached(hwnd as *mut c_void, kPlatformTypeHWND)`; on `WM_SIZE` call `view.onSize(&rect)`; apply
  the frame's recorded resize request (Step 3) via `SetWindowPos`; `onFocus` on activate;
  `view.removed()` + `DestroyWindow` on close. `Drop` calls `removed()` + destroys the window
  (idempotent). Add `windows` features as the cross-compiler requires.
- **Change:** add the dep features; `editor_window.rs` (`EditorWindow`, the `wndproc`, open/close/resize);
  cfg-gated module registration.
- **Verify:** **`make host-windows-check`** cross-compiles. **Red:** greenfield — `EditorWindow` doesn't
  resolve. **Green:** it cross-compiles; the real open/attach/render is the Windows field check (Step 6).

## Step 6 — Editor manager + `galad editor` subcommand (open/close, multiple editors)  [depends on #5] [windows]
- **File(s):** `host/src/vst3_host/editor.rs` (new, `#[cfg(windows)]`) or fold into `editor_window.rs`;
  `host/src/main.rs` (an `editor <plugin.vst3>...` subcommand, `#[cfg(windows)]` + a `not(windows)`
  stub + match arm + banner); `host/README.md` (note editor hosting).
- **Reference behavior:** An `EditorHost` that, per plugin, loads the module (M1), instantiates the
  `PluginInstance` + `EditorController` (Step 2), `create_view`, `prepare` (Step 4), and `EditorWindow::
  open` (Step 5); tracks **multiple** open editors; closes them cleanly (keeping each `LoadedModule`
  alive past its editor). `galad editor <plugin.vst3>...` opens an editor per path and runs a Win32
  message loop (`GetMessageW`/`DispatchMessageW`) until the windows close.
- **Change:** add the manager + subcommand + stub + README note.
- **Verify:** **`make host-windows-check`** cross-compiles. The **exit** — *editor opens, edits params
  live (audible), resizes, closes cleanly; no audio glitch on open/close; multiple editors* — is the
  Windows-runtime field check (`galad editor Cenedril.vst3 <third-party>.vst3`, ideally alongside a
  running `galad chain`/`session` to confirm no audio glitch), **not** a `make ci` gate. **Red:**
  greenfield. **Green:** cross-compiles and `cargo test -p galad` stays green.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| (phase) | Editor windowing toolkit. | **Resolved (mine):** raw Win32 via the `windows` crate (no baseview/winit); standalone top-level host window per editor; M6 integrates into `egui`. |
| (phase) | Component↔controller connection. | **Resolved (mine):** direct best-effort `IConnectionPoint` connect; a marshalling proxy is added only if a real plugin needs it (surfaced in the field check). |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. **No `[DECISION]` stops** (the plan lists
none; internal choices resolved above). Steps 1–4 are verified automatically by `cargo test -p galad`
on Linux (the editor COM protocol against a fixture controller + `IPlugView`); Steps 5–6 are
cross-compile-verified by `make host-windows-check`, with the live "editor opens / edits / resizes /
closes, no audio glitch" recorded as the Windows field check. M5 done means hosted plugin editors;
**M6** (the `egui` host UI tying devices + chain + meters + editor management together) follows. One
milestone at a time.
