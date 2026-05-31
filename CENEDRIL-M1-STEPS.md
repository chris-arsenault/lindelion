# Cenedril M1 — Step Plan

Expansion of **M1** from [`CENEDRIL-VST-PLAN.md`](CENEDRIL-VST-PLAN.md): the **Windows
`IPlugView`→`HWND` Vizia editor attach** — the reusable Windows editor foundation (ADR-0023),
proven with a trivial Vizia view before any real views are built on it. [depends on M0]

## Verification reality (read first)

M1 is a **Windows-runtime spike**. Its core — does the pinned Vizia/baseview stack embed into a
host-provided child `HWND` — lives in `#[cfg(windows)]` code that **Linux `make ci` does not
compile**. So the three verification tiers are distinct, and each step says which it uses:

- **Linux `make ci`** — the cross-platform parts only: the Vizia view *builder* compiles, the
  editor host/delegate compiles, `createView` returns non-null. It **cannot** exercise the HWND
  attach.
- **`make build-windows` (cargo-xwin, runnable here)** — cross-compiles the full Windows editor
  stack, *including Vizia/baseview for `x86_64-pc-windows-msvc`*. This is the **first hard spike
  signal**: if the pinned Vizia/baseview/renderer can't cross-compile for Windows, it fails here.
- **Galad / a Windows DAW (NOT runnable here)** — the actual open/render/resize/close + no-glitch
  check. This is M1's **true exit** and where the `[DECISION]` is settled. Galad's own VST3-load
  path is **Galad-M1**, so the first end-to-end run lands when that is up.

The phase-level `[DECISION]` — *any baseview/Vizia version or `raw-window-handle` adjustment the
HWND attach forces* — can only be resolved by the cargo-xwin compile (Step 4) and the Windows
runtime (Step 5). It is lifted to those steps; the executor stops there to surface findings.

## Reference / reuse map (from `CENEDRIL-VST-PLAN.md` "Context / reuse map" + ADR-0023)

The macOS editor attach is the template to mirror for Windows:
- **Shared `IPlugView` wrapper:** `crates/lindelion-plugin-shell/src/vst3_view.rs` —
  `FixedSizePlugView<D>` + the platform-neutral `FixedSizePlugViewDelegate` trait
  (`attached(parent, size)` / `removed` / `key_down`). `isPlatformTypeSupported`/`attached` are
  macOS-only today (`is_ns_view_platform` checks `b"NSView"`; non-macOS returns
  `kResultFalse`/`kNotImplemented`).
- **Per-plugin delegate:** e.g. `plugins/glirdir/src/vst3_entry/editor.rs` — `create_editor_view`
  returns a `ComWrapper::new(FixedSizePlugView::new(<delegate>, EDITOR_SIZE))`; the delegate's
  `attached` (macOS branch) calls `lindelion_ui::glirdir_vizia::GlirdirViziaEditor::attach(parent,
  host, size)`.
- **The Vizia open:** `lindelion_ui::glirdir_vizia` (`platform_state.rs` / `platform_layout.rs`) —
  `…ViziaEditor::attach` wraps the parent pointer in vizia's `ParentWindow(parent)` and calls
  `Application::new(builder).…open_parented(&parent)`. `ParentWindow`/`open_parented` are
  cross-platform vizia APIs (baseview handles the Win32 child-window embedding); the view builder,
  size logic, and `Model` are platform-neutral.
- **Pins:** vizia (git rev `cb334600…`, baseview feature) + baseview `0.1.0` (has a Win32 backend)
  + `raw-window-handle 0.5.2`. Editors are **fixed-size**, non-resizable (`canResize` →
  `kResultFalse`; size clamped).
- **Cenedril has no parameters in M0**, and the analysis/level data is M2 — so M1's view is a
  **static placeholder** (a label + a fixed meter bar) whose only job is to prove Vizia renders in
  the HWND. Real meters/spectrogram are M3+/M5.

---

## Step 1 — Windows `kPlatformTypeHWND` support in the shared `IPlugView` wrapper
- **File(s):** `crates/lindelion-plugin-shell/src/vst3_view.rs`.
- **Reference behavior:** Today `isPlatformTypeSupported` returns `kResultTrue` only for `b"NSView"`
  on macOS and `attached` calls `self.delegate.attached(parent, size)` only on macOS; every other
  platform returns `kResultFalse`/`kNotImplemented`. The VST3 Windows platform type is
  `kPlatformTypeHWND` = the C string `b"HWND"`, and the parent is a raw `HWND`. The
  `FixedSizePlugViewDelegate::attached(parent, size)` contract is already platform-neutral (the
  parent is "a valid platform view pointer for the host platform"). Add a **Windows branch**:
  `isPlatformTypeSupported` returns `kResultTrue` for `b"HWND"` under `#[cfg(windows)]`, and
  `attached` routes to `self.delegate.attached(parent, size)` under `#[cfg(windows)]` — mirroring
  the macOS arms. Factor the byte comparison into a small **platform-independent** helper
  `fn platform_type_is(type: FIDString, expected: &[u8]) -> bool` so it is unit-testable on Linux.
- **Change:** add the `cfg(windows)` arms in `isPlatformTypeSupported`/`attached`; add
  `platform_type_is` (used by both the NSView and HWND checks); keep macOS/other behavior
  identical.
- **Verify:** **Linux `make ci`** — add a unit test for `platform_type_is`: a NUL-terminated
  `b"HWND\0"` pointer matches `b"HWND"` and not `b"NSView"`, and null → false. **Red:** the helper
  doesn't exist (greenfield, fails to resolve). **Green:** passes. (The `cfg(windows)` routing
  itself is compiled by Step 4's cargo-xwin build, not `make ci`.)

## Step 2 — `cenedril_vizia` editor module in `lindelion-ui` (trivial view + Windows attach)
- **File(s):** `crates/lindelion-ui/src/cenedril_vizia.rs` (new) + `…/src/lib.rs` (register
  `pub mod cenedril_vizia;`); `crates/lindelion-ui/Cargo.toml` (add a
  `[target.'cfg(target_os = "windows")'.dependencies]` section mirroring the macOS one — at minimum
  `raw-window-handle = "0.5"`, plus anything the attach forces; `vizia` is already an all-platforms
  dep).
- **Reference behavior:** Mirror `glirdir_vizia`'s public shape, **minimal**: editor-size consts
  (`CENEDRIL_EDITOR_WIDTH`/`HEIGHT`), a `CenedrilEditorHost` carrying just the controller `usize`
  (no parameter surface — Cenedril has none yet), a `build_cenedril_application(host, size,
  parent_view)` returning a `vizia::Application` whose builder lays out a **static placeholder**
  (a titled label + a fixed meter bar — no data binding; real data is M2/M5), and a
  `CenedrilViziaEditor::attach(parent, host, size)` that wraps `ParentWindow(parent)` and calls
  `.open_parented(&parent)` (the same call Glirdir uses). The view builder, size, and any `Model`
  are platform-neutral and must compile on Linux; the `attach` compiles on all platforms but is
  only invoked at runtime on Windows (via Step 1's routing).
- **Change:** add the module + lib.rs registration + the Windows dep section. Keep files ≤600
  lines. Do **not** add drag-drop/clipboard/file-dialog (those are instrument-only macOS features
  Cenedril does not use).
- **Verify:** **Linux `make ci`** — a unit test that `build_cenedril_application(host, size, 0)`
  constructs without panicking (the builder runs; the window is not opened), and that the editor
  size consts are the expected fixed dimensions. **Red:** module/symbols don't exist (greenfield).
  **Green:** passes. (Windows embedding is Step 4/5.)

## Step 3 — Cenedril editor delegate + `createView` wiring  [depends on #1, #2]
- **File(s):** `plugins/cenedril/src/vst3_entry/editor.rs` (new) + `…/vst3_entry/mod.rs` (add
  `mod editor;`) + `…/vst3_entry/controller.rs` (`createView`) + `plugins/cenedril/Cargo.toml` (add
  `lindelion-ui.workspace = true` — the dep deferred from M0).
- **Reference behavior:** Mirror `plugins/glirdir/src/vst3_entry/editor.rs` **stripped to the
  minimum**: `EDITOR_SIZE` from the `cenedril_vizia` consts; `create_editor_view(controller) ->
  *mut IPlugView` returning `ComWrapper::new(FixedSizePlugView::new(CenedrilEditorView::new(...),
  EDITOR_SIZE)).to_com_ptr::<IPlugView>()…into_raw()`; a `CenedrilEditorView` delegate holding the
  controller pointer and (non-gated, since it must compile for Windows and Linux) an
  `Option<CenedrilViziaEditor>` opened in `attached` and dropped in `removed`. The controller's
  `createView` (currently `ptr::null_mut()` from M0) now returns `super::editor::create_editor_view(self)`.
- **Change:** add `editor.rs`; register `mod editor`; change `createView`; add the `lindelion-ui`
  dep. The controller must expose itself to the delegate the way Glirdir does (controller `usize`
  in the host).
- **Verify:** **Linux `make ci`** — a unit test that `create_editor_view(&controller)` returns a
  non-null `*mut IPlugView` (the COM object constructs cross-platform), and that the M0 controller
  test still passes. **Red:** before the change `createView` is null / `create_editor_view` doesn't
  exist. **Green:** non-null view is produced. (The attach only fires on Windows at runtime.)

## Step 4 — Cross-compile the full Windows editor stack (cargo-xwin)  [depends on #3]  [DECISION]
- **File(s):** none (build verification); if the build forces it, `Cargo.toml` pin changes land
  here under the `[DECISION]`.
- **Reference behavior:** With Steps 1–3 in, `make build-windows` now compiles Cenedril **with its
  editor** — which pulls `lindelion-ui` and therefore **Vizia + baseview + their renderer for
  `x86_64-pc-windows-msvc`**. This is the first concrete test that the pinned stack cross-compiles
  for Windows at all (the M0 build had no editor, so this surface is new).
- **[DECISION]:** if the pinned vizia (git rev) / baseview `0.1.0` / `raw-window-handle 0.5` **fail
  to cross-compile** for Windows (renderer/system-lib linkage, baseview Win32 backend, or a
  raw-window-handle API mismatch), stop and surface the findings + options (bump vizia/baseview to
  a rev with working Windows embedding, add a missing windows-target dep, or — fallback — a thin
  raw-`windows`-crate child-HWND host). Do not guess a version bump silently.
- **Verify:** **`make build-windows`** exits 0 and stages a `Cenedril.vst3` whose DLL still exports
  `GetPluginFactory`/`InitDll`/`ExitDll` (as in M0). **Red:** before Step 3 the editor stack wasn't
  compiled for Windows. **Green:** the editor-bearing bundle cross-compiles and stages. **Also:**
  `make ci` (Linux) stays green.

## Step 5 — Windows-host runtime validation (Galad / DAW)  [depends on #4]  [DECISION]
- **File(s):** none (runtime validation); document the result in `plugins/cenedril/README.md`.
- **Reference behavior:** Load the staged `Cenedril.vst3` in a Windows host and open the editor.
  Per ADR-0023 / the plan's M1 exit: the editor **opens in the host's child `HWND`, renders the
  Vizia placeholder view, resizes (fixed-size: host honors the reported size), and closes** —
  with **no audio glitch on open/close** (the passthrough keeps running). This exercises Step 1's
  HWND routing → Step 3's delegate → Step 2's `open_parented` end to end.
- **[DECISION]:** this is where the baseview/Vizia HWND-embedding viability is finally settled.
  If embedding misbehaves (black view, wrong parent, focus/resize bugs, crash on close), surface
  the findings and the adjustment (version bump / raw-window-handle fix / fallback host) before
  proceeding to M3 views.
- **Verify:** **Galad / a Windows DAW** — manual: editor opens, renders the placeholder, closes
  cleanly, audio stays bit-exact through open/close. **Not runnable in this Linux environment** —
  the verify target is **Galad**, gated on **Galad-M1** (host-side VST3 load). This is M1's true
  exit gate.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| 4 | Does the pinned Vizia/baseview/`raw-window-handle` cross-compile for Windows? If not: version bump / add windows dep / raw-`windows` fallback. | **Open** — resolved by running `make build-windows` with the editor in. |
| 5 | Does baseview/Vizia actually embed + render in the host child `HWND` (open/resize/close, no glitch)? Any version/handle adjustment it forces. | **Open** — needs a Windows host (Galad-M1). |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. Steps 1–3 are Linux-`make ci`-verifiable
(cross-platform parts) and gate-checked by Step 4's cargo-xwin cross-build (runnable here). Step 5
is the Windows-host runtime exit, deferred to Galad-M1. The executor **stops at the Step 4 and
Step 5 `[DECISION]`s** to surface spike findings rather than guessing version changes.
