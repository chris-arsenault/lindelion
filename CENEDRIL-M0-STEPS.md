# Cenedril M0 — Step Plan

Expansion of **M0** from [`CENEDRIL-VST-PLAN.md`](CENEDRIL-VST-PLAN.md) into execution-ready,
red→green steps. Scope: the `plugins/cenedril` crate, the Windows `.vst3` bundle path, and a
bit-exact 0-latency passthrough processor. **No editor** (M1), **no analysis** (M2).

**Name decision (resolved):** product **Cenedril**; the reserved `plugins/visualizer/` dir is
renamed to **`plugins/cenedril/`** with package **`cenedril`** (repo convention
package==dir==product). The metadata const is **`CENEDRIL_VST3_BUNDLE_METADATA`** (the plan's
working name `VISUALIZER_…` is superseded by the confirmed product name).

Mirror source of truth: **Glirdir** (`plugins/glirdir/src/vst3_entry/`) — the existing Fx effect
with the same `audio_input(2)`→`audio_output(2)` MAIN passthrough bus shape and 0 latency. Use
Linnod's factory test (`plugins/linnod/src/vst3_entry/factory.rs`) and metadata wiring as the
secondary pattern. Reuse map: `CENEDRIL-VST-PLAN.md` "Context / reuse map";
[ADR-0023](docs/adr/0023-new-vsts-windows-only.md) (Windows build path, Vizia editor in M1);
[ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (allocation-free `process`).

Both phase-level `[DECISION]`s are now resolved: the product **name** (Cenedril; above) and the
**Windows build toolchain** — **cargo-xwin**, target **`x86_64-pc-windows-msvc`**, cross-built from
Linux (Step 6/7). No `[DECISION]` stops remain in M0.

---

## Step 1 — Register `plugins/cenedril` as a workspace member + crate skeleton
- **File(s):** `Cargo.toml` (workspace `members`), `plugins/cenedril/Cargo.toml` (new),
  `plugins/cenedril/src/lib.rs` (new); rename `plugins/visualizer/README.md` →
  `plugins/cenedril/README.md`; update the path references in `AGENTS.md` (Code Map),
  `docs/adr/0023-new-vsts-windows-only.md`, and `CENEDRIL-VST-PLAN.md` from `plugins/visualizer`
  to `plugins/cenedril` (a factual path rename of the reserved home — keep wording, just the path).
- **Reference behavior:** Mirror `plugins/glirdir/Cargo.toml`: `crate-type = ["cdylib", "rlib"]`;
  `[package] name = "cenedril"`; dependencies trimmed to what a passthrough+metadata crate needs —
  `lindelion-plugin-shell`, `lindelion-plugin-metadata`, `lindelion-dsp-utils`, `vst3`;
  `[dev-dependencies] lindelion-test-allocator`; the `integration-tests` feature stanza verbatim
  from Glirdir/Linnod. **No `lindelion-ui` yet** — M0 has no editor (the Vizia editor + its Windows
  `IPlugView`→`HWND` attach are M1, ADR-0023); add `lindelion-ui` in M1, not here. No
  `lindelion-midi`/`-sample-library` (not an instrument). `lib.rs` declares the module tree
  (`mod plugin; mod vst3_entry;` added in later steps) — start with `pub mod plugin;` only.
- **Change:** add `"plugins/cenedril"` to `Cargo.toml` members; create the two new files; move the
  README; fix the doc path references; `lib.rs` holds `pub mod plugin;` and one smoke
  `#[test] fn crate_builds() { assert!(true) }` placeholder until Step 3 replaces it.
- **Verify:** `cargo test -p <pkg>` — **red:** before the member exists, cargo errors *"no such
  package"* (greenfield: the crate/test doesn't resolve); **green:** the smoke test compiles and
  passes. `make ci` (clippy `--workspace`) must still pass.

## Step 2 — Add `CENEDRIL_VST3_BUNDLE_METADATA` to `lindelion-plugin-metadata`  [depends on #1]
- **File(s):** `crates/lindelion-plugin-metadata/src/lib.rs`.
- **Reference behavior:** Mirror `GLIRDIR_VST3_BUNDLE_METADATA` (the Fx exemplar):
  `vst3_sub_categories: "Fx"`, `module_sub_categories: &["Fx"]`. Set `package`/`bundle_name`/
  `executable_name`/`bundle_identifier`/`library_stem`/`controller_name` from the confirmed name
  (e.g. `bundle_name: "Cenedril"`, `bundle_identifier: "com.ahara.cenedril"`, `library_stem:
  "cenedril"`). Assign **fresh, globally-unique CIDs** distinct from Lamath/Glirdir/Linnod —
  proposed processor `[0xCE9ED713, 0x1A5B4C20, 0x8F3D6E94, 0xB2470FA1]`, controller
  `[0xCE9EDC72, 0x6D8E4F31, 0xA1B05C28, 0x73E2941D]`. Extend `metadata_for_package` with the new
  arm.
- **Change:** add the `pub const` block; add the `match` arm in `metadata_for_package`.
- **Verify:** extend `metadata_for_package`'s unit test (`crates/lindelion-plugin-metadata/src/lib.rs`)
  to assert `metadata_for_package("cenedril").unwrap() == CENEDRIL_VST3_BUNDLE_METADATA`. **Red:**
  arm absent → `None` → unwrap panics; **green:** passes. (`cargo test -p lindelion-plugin-metadata`.)

## Step 3 — Passthrough plugin core (`Cenedril: AudioPlugin`)  [depends on #1]
- **File(s):** `plugins/cenedril/src/plugin.rs` (new), `plugins/cenedril/src/lib.rs`
  (re-export `Cenedril`, `DESCRIPTOR`, and `VST3_BUNDLE_METADATA` like Linnod's `lib.rs`).
- **Reference behavior:** Implement the small `AudioPlugin` trait
  (`crates/lindelion-plugin-shell/src/process.rs:212`): `descriptor` =
  `PluginDescriptor::effect("Cenedril", *b"...")` (effect, not instrument); `parameters` = `&[]`
  (no params in M0); `reset` stores the `ProcessSetup`; `state`/`load_state` =
  empty `PluginState` round-trip (no persisted fields yet — M6 adds editor settings). `process`
  is a **bit-exact stereo passthrough**: copy `ctx.input.left[i] → output.left[i]` and
  `right → right` verbatim, sample-for-sample, no averaging/`mono_sample` (that would not be
  bit-exact), no sanitization, no allocation. When an input channel is absent, write silence to
  that output channel (defined behavior, still 0 latency). Per
  [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) the body allocates nothing.
- **Change:** add `plugin.rs` with `struct Cenedril { setup: ProcessSetup }`, `Default`, and the
  trait impl; wire `lib.rs` re-exports; replace Step 1's placeholder smoke test.
- **Verify:** unit test in `plugin.rs`: build a stereo input with known/arbitrary values, run
  `process`, assert output equals input sample-for-sample; wrap the `process` call in
  `assert_no_allocations!` (`lindelion-test-allocator`). **Red:** before the impl exists the
  symbol doesn't resolve (greenfield); a stub that clears output fails the equality assertion.
  **green:** equality holds and no allocations occur. (`cargo test -p cenedril`.)

## Step 4 — VST3 processor (`IComponent`/`IAudioProcessor`): passthrough buses, 0 latency  [depends on #2, #3]
- **File(s):** `plugins/cenedril/src/vst3_entry/mod.rs` (new),
  `plugins/cenedril/src/vst3_entry/processor.rs` (new); register `mod vst3_entry;` in `lib.rs`.
- **Reference behavior:** Mirror `plugins/glirdir/src/vst3_entry/processor.rs` **stripped of
  capture/messaging**: `CENEDRIL_BUSES = [Vst3BusInfo::audio_input(2, "Input"),
  Vst3BusInfo::audio_output(2, "Output")]` (both MAIN, default-active —
  `crates/lindelion-plugin-shell/src/vst3_component.rs:225`). `Class::Interfaces =
  (IComponent, IAudioProcessor, IProcessContextRequirements)` (no `IConnectionPoint` in M0 — no
  controller messaging yet). `getBusCount`/`getBusInfo` via `vst3_bus_count`/`fill_vst3_bus_info`.
  `setBusArrangements` accepts only `num_ins == 1 && num_outs == 1` with both `== SpeakerArr::kStereo`
  (strict stereo keeps passthrough trivially bit-exact). `getLatencySamples` → `0`,
  `getTailSamples` → `0`. `process`: read input via `audio_input_buffer_from_vst_process_data`,
  outputs via `stereo_output_buffers_from_vst_process_data`, drive `Cenedril::process` with
  `ShellProcessContext::new(setup, buffer, &[]).with_input(input)`; on non-`kSample32` or missing
  output, `clear_vst_outputs`. `mod.rs` follows Linnod's header
  (`#![allow(non_snake_case)]` … ), but the dead-code gate keys on **`windows`** not `macos`
  (Cenedril's live entry-point target is Windows): `#![cfg_attr(not(target_os = "windows"),
  allow(dead_code))]` — so it compiles dead-code-clean on Linux `make ci`.
- **Change:** add the two files and the `lib.rs` module; processor `CID` from
  `VST3_BUNDLE_METADATA.processor_cid`.
- **Verify:** cross-platform unit test (runs in `make ci` on Linux): assert
  `vst3_bus_count(&CENEDRIL_BUSES, kAudio, kInput) == 1` and `… kOutput == 1`, and a constructed
  `CenedrilVst3Processor::new()` reports `getLatencySamples() == 0`. **Red:** consts/type absent
  (greenfield); **green:** passes. (Driving COM `process()` end-to-end is deferred to the Windows
  host verify in Step 7 — the bit-exact core itself is already proven in Step 3.)

## Step 5 — Minimal VST3 controller + factory + entry points  [depends on #2, #4]
- **File(s):** `plugins/cenedril/src/vst3_entry/controller.rs` (new),
  `plugins/cenedril/src/vst3_entry/factory.rs` (new); register both in `vst3_entry/mod.rs`.
- **Reference behavior:** Mirror `plugins/glirdir/src/vst3_entry/controller.rs` **stripped to the
  minimum**: `Class::Interfaces = (IEditController,)`; `getParameterCount` → `0`;
  `IEditController` init/state methods return `kResultOk`; **`createView` returns
  `core::ptr::null_mut()`** — no `IPlugView` yet (the Vizia editor + its Windows HWND attach are M1, ADR-0023). Factory mirrors
  `plugins/linnod/src/vst3_entry/factory.rs`: register the processor (`SUBCATEGORY` =
  `VST3_BUNDLE_METADATA.vst3_sub_categories` = `"Fx"`) and the controller via
  `Vst3ClassRegistration`/`Vst3PluginFactory`, then
  `lindelion_plugin_shell::export_vst3_entrypoints!(cenedril_vst3_factory())` — the macro already
  emits the Windows `InitDll`/`ExitDll` (`crates/lindelion-plugin-shell/src/vst3.rs:105`).
- **Change:** add the two files; controller `CID` from `VST3_BUNDLE_METADATA.controller_cid`.
- **Verify:** mirror Linnod's `registers_processor_and_controller_with_shared_factory` test:
  `class_count() == 2`, `getClassInfo2(0)` name == `"Cenedril"` and `subCategories == "Fx"`,
  `getClassInfo(1)` is the controller. **Red:** factory absent (greenfield); **green:** passes.
  (`cargo test -p cenedril`; runs in `make ci`.)

## Step 6 — Windows `.vst3` bundle path in `xtask` (cross-built with cargo-xwin)  [depends on #2]
- **File(s):** `xtask/src/bundle.rs`, `xtask/src/tests.rs`.
- **Build toolchain (resolved):** **cargo-xwin**, target **`x86_64-pc-windows-msvc`**,
  cross-compiled from Linux — the ship-correct MSVC ABI that Galad/Windows DAWs load, producible in
  this environment (and CI). Plain `cargo build --target x86_64-pc-windows-msvc` can't link from
  Linux (no MSVC linker); **`cargo xwin build`** supplies the CRT/SDK + `lld`. Prereqs:
  `cargo install cargo-xwin`, `rustup target add x86_64-pc-windows-msvc`, and
  `XWIN_ACCEPT_LICENSE=1` (Microsoft CRT/SDK, downloaded once).
- **Reference behavior:** Today `run_bundle` hard-refuses non-macOS targets, `build_release` shells
  `cargo build`, and `create_macos_vst3_bundle` writes `Contents/MacOS/<exe>` + `Info.plist` +
  `PkgInfo` + `Resources/moduleinfo.json`. Add a **Windows** path: (a) in `build_release`, when the
  target is `*-pc-windows-msvc`, invoke **`cargo xwin build --release -p cenedril --target
  x86_64-pc-windows-msvc`** instead of `cargo build`; (b) `create_windows_vst3_bundle` writes the
  layout `<BundleName>.vst3/Contents/x86_64-win/<BundleName>.vst3` (the DLL renamed to `.vst3`) plus
  `Contents/Resources/moduleinfo.json` (reuse `module_info(spec)` verbatim — platform-neutral);
  **no `Info.plist`/`PkgInfo`/codesign**. Source artifact for a cdylib on Windows is
  `<library_stem>.dll` (no `lib` prefix) in the target dir — add a `windows`-target arm to
  `source_library_path`. Gate selection on the target triple (`*-pc-windows-*`) the way
  `target_is_macos` gates today; keep the macOS path unchanged.
- **Change:** add the `cargo xwin` arm to `build_release`; add `create_windows_vst3_bundle` + the
  target dispatch in `run_bundle`; extend `source_library_path`. Keep functions small (≤600-line
  file lint).
- **Verify:** add a **pure string/layout** unit test alongside the existing
  `glirdir_module_info_uses_effect_metadata` style in `xtask/src/tests.rs` — assert the Windows
  layout descriptor (the relative paths `Contents/x86_64-win/Cenedril.vst3` and
  `Contents/Resources/moduleinfo.json`) and that `module_info(&spec)` for the Cenedril spec
  contains the Cenedril CIDs and `"Fx"`. **Red:** the layout helper/spec arm doesn't exist;
  **green:** passes. Keep `make ci` fs-free and cargo-xwin-free: test the path/string builder, not
  real bundle creation — the actual cross-build + fs write runs via Step 7's `make build-windows`
  (which *can* run on this Linux box once cargo-xwin is installed).

## Step 7 — `make build-windows` (cargo-xwin) + verify path  [depends on #5, #6]
- **File(s):** `Makefile` (a `build-windows` target), optionally `xtask/src/main.rs` help text.
- **Reference behavior:** `make build` is macOS-only (refuses non-Darwin) and iterates
  `BUILD_PLUGINS` calling `xtask bundle <plugin> --target <macos>`. Add a parallel **`build-windows`**
  target that runs `xtask bundle cenedril --target x86_64-pc-windows-msvc` (which now drives
  `cargo xwin build` per Step 6) and stages the resulting `.vst3` bundle. It must check the
  prereqs (cargo-xwin installed, target added) with a clear error, mirroring how `build` checks for
  Darwin/`rustup target`. Cenedril is Windows-only, so it must **not** join the macOS
  `BUILD_PLUGINS`/`PLUGINS` list.
- **Verify (resolved):** **build half is now Linux-runnable** — `make build-windows` produces the
  MSVC-ABI `Cenedril.vst3` on this box via cargo-xwin (the artifact, not a host load). **Load half:**
  the verify *target is Galad* (the VST3 host being built in parallel); the first end-to-end
  "loads + passes audio bit-exact at 0 latency" check happens once Galad-M1 (host-side VST3 load)
  is up. **Exit gate:** `make ci` green on Linux (Steps 1–6) **and** `make build-windows` produces a
  loadable `Cenedril.vst3` that, in Galad, passes audio through bit-exact at 0 latency.
- **Change:** add the `build-windows` Makefile target + prereq checks; document it in the reserved
  `plugins/cenedril/README.md`.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| 1, 2 | Product name + crate layout. | **Resolved:** Cenedril; dir `plugins/cenedril/`, package `cenedril`. |
| 6, 7 | Windows build toolchain + verify target. | **Resolved:** **cargo-xwin**, `x86_64-pc-windows-msvc`, cross-built from Linux; load-verified in the **Galad** host (its VST3-load path is Galad-M1). |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. No `[DECISION]` stops remain.
Steps 1–6 are fully verifiable in `make ci` on Linux; the Windows build half is runnable here via
`make build-windows` (cargo-xwin); the load half is verified in Galad once Galad-M1 is up.
