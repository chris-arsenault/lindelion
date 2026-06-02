# Lúmedir — M0 — Execution Steps

Step-level expansion of **M0** from [`LUMEDIR-VST-PLAN.md`](LUMEDIR-VST-PLAN.md): *Coach plugin
scaffold on the shared Windows platform — a working (silent) Windows plugin that feeds the analysis
worker.* Depends on **Cenedril M0/M1** (the Windows VST3 build path + the `IPlugView`→`HWND` Vizia
attach), both already built.

Run these in order. Each step names its file(s), the reference behavior to re-derive from a
source-of-truth (not memory), the minimal change, and a red→green test. The exit gate for the phase
is **`make ci` green** *and* the plugin builds as a Windows `.vst3` that passes audio bit-exact at 0
latency with the worker yielding `SignalSnapshot`s.

The template this phase mirrors throughout is **Cenedril** (`plugins/cenedril/`,
`crates/lindelion-ui/src/cenedril_vizia*`, `crates/lindelion-plugin-metadata`, `xtask/src/windows_bundle.rs`,
the `Makefile` Windows path). The one genuinely new piece is **feeding `AnalysisWorker`** from a
passthrough `process()` — no existing plugin does this yet.

---

## 1. Confirm the product name and the metadata string forms  [DECISION]

- **File(s):** none yet — this resolves the names that steps 2–8 hard-code.
- **Reference behavior:** the plan's M0 `[DECISION]` and `AGENTS.md` Product Names table call the
  product **Lúmedir** with crate/dir **`coach`**. Existing bundle metadata
  (`crates/lindelion-plugin-metadata/src/lib.rs`) is all ASCII; `library_stem` must equal the cdylib
  name and `PluginDescriptor::effect(name, id)` / `PClassInfo` names are written into C-string
  buffers.
- **Change:** confirm with the user:
  1. **Name = Lúmedir** (vs an alternative), crate/dir stays **`coach`**.
  2. The **display/bundle name** string: `"Lúmedir"` (with the `ú` diacritic, valid UTF-8 in the
     C-string buffers) **or** an ASCII `"Lumedir"`. This fixes `bundle_name`, `executable_name`,
     `controller_name`, and the `DESCRIPTOR` name used in steps 2–3/6. (`library_stem` = `"coach"`,
     `bundle_identifier` = `"com.ahara.coach"` regardless.)
  3. **Advisory only, does not gate any M0 step:** where the M1–M3 delivery estimators will live —
     promoted into shared `lindelion-speech-signals` (reusable) vs plugin-local in `plugins/coach`.
     M0 only *feeds* the existing worker, so record the choice but defer its effect to M1.
- **Verify:** no test — the executor stops here for the user's call, then carries the chosen strings
  forward verbatim. (If the user defers (1)/(2), do not proceed; (3) may be left open.)

> **RESOLVED (2026-05-31):** the user directed that the **filesystem match the product name, not a
> codename** ("just like every other plugin"). Therefore, **throughout steps 2–9, read `coach` →
> `lumedir`** (crate/dir/`package`/`library_stem` = `lumedir`; `bundle_identifier` =
> `com.ahara.lumedir`; module path `crates/lindelion-ui/src/coach_vizia*` → `lumedir_vizia*`; types
> `Coach*` → `Lumedir*`; cargo selector `-p coach` → `-p lumedir`). Display strings
> (`bundle_name`/`executable_name`/`controller_name`/`DESCRIPTOR` name) are **ASCII `"Lumedir"`**
> (accented "Lúmedir" stays prose-only, matching the all-ASCII metadata of every existing plugin).
> The 16-byte `PluginDescriptor` id is `*b"lindelion_lumedr"` (16 bytes). Estimator location =
> **plugin-local** (advisory; applied at M1).

---

## 2. Add `COACH_VST3_BUNDLE_METADATA` and register it in `metadata_for_package`  [depends on #1]

- **File(s):** `crates/lindelion-plugin-metadata/src/lib.rs`.
- **Reference behavior:** re-derive the `Vst3BundleMetadata` field shape from the existing
  `CENEDRIL_VST3_BUNDLE_METADATA` (the closest analogue — a passthrough **`Fx`**): `package`,
  `bundle_name`, `executable_name`, `bundle_identifier`, `library_stem`, `vst3_sub_categories: "Fx"`,
  `module_sub_categories: &["Fx"]`, two CIDs, `controller_name`. `from_plugin` in `xtask/src/bundle.rs`
  resolves entirely through `metadata_for_package`, so adding the arm is all xtask needs.
- **Change:** add a `COACH_VST3_BUNDLE_METADATA` const using the step-1 strings (`package: "coach"`,
  `library_stem: "coach"`, `bundle_identifier: "com.ahara.coach"`, `vst3_sub_categories: "Fx"`) with
  **two freshly generated 128-bit CIDs distinct from all four existing plugins'** (cenedril used the
  `0xCE9E…` mnemonic prefix; pick a distinct one). Add a `"coach" => Some(COACH_VST3_BUNDLE_METADATA)`
  arm to `metadata_for_package`.
- **Verify:** extend `metadata_lookup_covers_bundleable_plugins` to assert
  `metadata_for_package("coach").unwrap() == COACH_VST3_BUNDLE_METADATA`, plus an assertion that the
  two new CIDs differ from every existing CID. **Red:** the const/arm don't exist → won't compile.
  **Green:** `cargo test -p lindelion-plugin-metadata`.

---

## 3. Create the `plugins/coach` crate: workspace member + passthrough processor  [depends on #2]

- **File(s):** `Cargo.toml` (workspace `members`), `plugins/coach/Cargo.toml`,
  `plugins/coach/src/lib.rs`, `plugins/coach/src/plugin.rs`, `plugins/coach/README.md`.
- **Reference behavior:** mirror `plugins/cenedril/`: `Cargo.toml` (`crate-type = ["cdylib", "rlib"]`,
  deps `lindelion-dsp-utils`/`lindelion-plugin-metadata`/`lindelion-plugin-shell`/`lindelion-ui`/`vst3`,
  dev-dep `lindelion-test-allocator`, an empty `integration-tests` feature); `lib.rs` re-exporting
  metadata + installing the test allocator; `plugin.rs` implementing `AudioPlugin` with the bit-exact
  `passthrough_channel` (copy `input.left/right` to `buffer.left/right`, silence missing channels,
  no allocation), empty `parameters()`, `PluginState::empty(STATE_FORMAT_VERSION)`. The
  **worker** dep and feed are deliberately deferred to step 4 to keep this step a clean passthrough.
- **Change:** add `"plugins/coach"` to the workspace `members`; create the crate with a `Coach`
  `AudioPlugin` that is a pure stereo passthrough at 0 latency (no worker field yet);
  `DESCRIPTOR = PluginDescriptor::effect(<name from #1>, *b"lindelion_coach_")` (16 bytes); a `README.md`
  mirroring Cenedril's (Windows-only, build via `make build-windows`, links to ADR-0023 and this plan).
- **Verify:** port Cenedril's `process_is_bit_exact_stereo_passthrough_without_allocating` test
  (wrap `process()` in `assert_no_allocations`, assert outputs equal inputs). **Red:** the `coach`
  crate/`Coach` type don't exist → won't compile. **Green:** `cargo test -p coach`.

---

## 4. Feed `AnalysisWorker` from `process()`  [depends on #3]

- **File(s):** `plugins/coach/Cargo.toml` (add `lindelion-speech-signals` dep + a
  `test-sync-analysis` feature), `plugins/coach/src/plugin.rs`.
- **Reference behavior:** re-derive the worker contract from `speech/signals/src/worker.rs`:
  `AnalysisWorker::new(source_sample_rate: u32)` spawns the off-thread worker (allocates — do it in
  `reset`, never `process`); `push(&[f32])` is **allocation-free and non-blocking** in the default
  (off-thread) build; `latest() -> SignalSnapshot` is allocation-free. The mono feed uses the
  existing allocation-free `AudioInputBuffer::write_mono_to(&mut [f32])`
  (`crates/lindelion-plugin-shell/src/process.rs`) into a buffer pre-sized in `reset` from
  `ProcessSetup::max_block_size`. The counting allocator is **thread-local**
  (`crates/lindelion-test-allocator/src/lib.rs`), so a live worker thread does not perturb the
  audio-thread no-alloc assertion (proven by `worker.rs`'s own `push_and_latest_are_allocation_free`).
- **Change:** add the `lindelion-speech-signals` dependency and a
  `test-sync-analysis = ["lindelion-speech-signals/sync-analysis"]` feature. Give `Coach` a
  `worker: Option<AnalysisWorker>` and a pre-sized `mono_scratch: Vec<f32>` (both `Default`able, so
  keep `#[derive(Default)]`). In `reset`, build the worker at `setup.sample_rate` and size
  `mono_scratch` to `max_block_size`. In `process`, after passthrough, mono-mix the **input** via
  `write_mono_to(&mut self.mono_scratch)` and `worker.push(&mono_scratch[..n])`. Add a
  `latest_snapshot(&self) -> SignalSnapshot` accessor (the read path the M4 editor and the exit
  criterion both need).
- **Verify:** in `make ci` (default features, off-thread worker), the ported no-alloc passthrough
  test still passes **with the worker now wired** — extend it so the asserted region also performs
  the mono-feed (proves the feed is allocation-free). **Red:** before wiring, `Coach` has no worker/
  feed so the test references a non-existent `latest_snapshot`/feed path → won't compile; **Green:**
  `cargo test -p coach`. (The deterministic *snapshot-value* proof is step 5, gated out of `make ci`.)

---

## 5. Deterministic "worker yields a `SignalSnapshot`" integration test  [depends on #4]

- **File(s):** `plugins/coach/tests/integration.rs` (new), `Makefile` (`test-integration` target).
- **Reference behavior:** this is the phase exit's *"the worker yields `SignalSnapshot`s"* clause,
  made deterministic and **pure for `make ci`** per the unit-test purity rule (no threads/sleep/
  wall-clock in the default run). Re-derive the pattern from the speech effect crates' integration
  suite: with `lindelion-speech-signals/sync-analysis`, `push` runs the analyzer **inline** and
  publishes before returning, so `latest()` immediately reflects the fed audio (no off-thread race,
  no sleep-poll). The `make ci` run leaves the feature off → the test is `#[ignore]`d; the worker's
  voiced/silence semantics are in `analyzer.rs` (`voicing_state == 2.0` for a voiced tone, `0.0` for
  silence).
- **Change:** add `plugins/coach/tests/integration.rs` with a test, annotated
  `#[cfg_attr(not(feature = "test-sync-analysis"), ignore = "needs sync-analysis for a deterministic worker snapshot")]`,
  that builds `Coach`, `reset`s at 48 kHz, drives a synthetic voiced tone block through `process()`
  (the `voiced_tone` helper used in `worker.rs`/`analyzer.rs`), and asserts `latest_snapshot()` reads
  `voicing_state == 2.0` with a plausible `pitch_hz` — and a silence block reads `voicing_state == 0.0`.
  Add to the `Makefile` `test-integration` target, matching the speech-crate invocation shape:
  `cargo test -p coach --test integration --features test-sync-analysis -- --include-ignored`
  (note `--test integration`, so the lib's off-thread no-alloc test from step 4 is **not** rebuilt
  under the allocating sync build).
- **Verify:** **Red:** the test file/`test-sync-analysis` feature don't exist. **Green:**
  `make test-integration` runs it (un-ignored, inline-sync, deterministic) and `make ci` skips it
  (ignored). Confirm both: `cargo test -p coach` shows it ignored; the new Makefile line passes.

---

## 6. Add the placeholder Vizia editor stack in `lindelion-ui`  [depends on #1]

- **File(s):** `crates/lindelion-ui/src/coach_vizia.rs` (new),
  `crates/lindelion-ui/src/coach_vizia/platform.rs` (new), `crates/lindelion-ui/src/lib.rs` (register
  `pub mod coach_vizia;`).
- **Reference behavior:** mirror `cenedril_vizia`: the size/host types
  (`CoachEditorSize`, `CoachEditorHost::new(controller: usize)`) and the `COACH_EDITOR_WIDTH/HEIGHT`
  consts are **platform-neutral and compile everywhere** (incl. Linux `make ci`); the `vizia`
  `Application` + `IPlugView`→`HWND` `open_parented` attach (`CoachViziaEditor::attach`) lives in the
  **`#[cfg(target_os = "windows")]`** `platform` submodule, because `vizia` is a Windows-target dep
  for the new VSTs (ADR-0023). The M0 view is a **static placeholder** (a titled `VStack`), reading
  no data — exactly Cenedril's `build_placeholder_view`.
- **Change:** create `coach_vizia.rs` (neutral `CoachEditorSize`/`CoachEditorHost` + width/height
  consts + `#[cfg(windows)] pub use platform::CoachViziaEditor;`) and `coach_vizia/platform.rs`
  (the windows-gated Vizia app + `attach`/`Drop`), titled with the step-1 name and a "Speech-Coach —
  editor embedding (M0)" subtitle. Register the module in `lib.rs`.
- **Verify:** add a small neutral-type unit test in `coach_vizia.rs` (e.g. `CoachEditorHost::new`
  round-trips the controller pointer; the width/height consts are the expected positive values) so
  the module is exercised cross-platform. **Red:** the module/types don't exist → won't compile.
  **Green:** `cargo test -p lindelion-ui`. (The Windows attach itself is `cfg(windows)` and validated
  by the Windows build in step 8, not by `make ci`.)

---

## 7. Add the `vst3_entry` COM scaffold (factory/processor/controller/editor)  [depends on #2, #3, #6]

- **File(s):** `plugins/coach/src/vst3_entry/mod.rs`, `.../controller.rs`, `.../factory.rs`,
  `.../processor.rs`, `.../editor.rs` (all new); `plugins/coach/src/lib.rs` (add `mod vst3_entry;`).
- **Reference behavior:** mirror `plugins/cenedril/src/vst3_entry/` verbatim in structure, swapping
  Cenedril→Coach and `cenedril_vizia`→`coach_vizia`: `mod.rs` (the `non_*`/`unsafe_op` allows +
  off-Windows `dead_code` allow + `SUBCATEGORY` from metadata); `processor.rs` (one stereo audio-in /
  one stereo audio-out `Vst3BusInfo` pair, `getLatencySamples → 0`, `IComponent`/`IAudioProcessor`/
  `IProcessContextRequirements`, state read/write via the shell helpers, CIDs from
  `COACH_VST3_BUNDLE_METADATA`); `controller.rs` (`IEditController`, zero params, `createView →
  editor::create_editor_view`); `editor.rs` (`FixedSizePlugView` over `COACH_EDITOR_WIDTH/HEIGHT`,
  `#[cfg(windows)]` `CoachViziaEditor::attach`, `kNotImplemented` off-Windows); `factory.rs`
  (register processor + controller, `export_vst3_entrypoints!`).
- **Change:** create the five files as the Coach analogue; add `mod vst3_entry;` to `lib.rs`.
- **Verify:** port Cenedril's `vst3_entry` unit tests — factory registers exactly the processor +
  controller with the Coach CIDs/`SUBCATEGORY` (`*_registers_processor_and_controller_with_shared_factory`),
  the processor reports one stereo in / one stereo out and **zero latency**, and
  `create_editor_view` returns a non-null `IPlugView` (reclaiming the leaked COM ref). **Red:** the
  modules/symbols don't exist → won't compile. **Green:** `cargo test -p coach`.

---

## 8. Register the Windows `.vst3` bundle path for `coach`  [depends on #2, #7]

- **File(s):** `Makefile` (`WINDOWS_PLUGINS`), `xtask/src/windows_bundle.rs` (tests).
- **Reference behavior:** the Windows bundle automation (`xtask/src/windows_bundle.rs`,
  `make build-windows`) is driven by `BundleSpec::from_plugin` → `metadata_for_package`, so once
  step 2's metadata exists, `xtask plugin-info coach` / `xtask bundle coach --target
  x86_64-pc-windows-msvc` resolve automatically; `make build-windows` iterates `WINDOWS_PLUGINS`.
  The `cenedril_windows_layout_places_dll_and_moduleinfo` / `cenedril_module_info_uses_effect_metadata`
  tests pin the `.vst3` folder layout (`<bundle>/Contents/x86_64-win/<stem>.vst3`) and moduleinfo.
- **Change:** add `coach` to `WINDOWS_PLUGINS` in the `Makefile` (`WINDOWS_PLUGINS ?= cenedril coach`).
  Add Coach analogues of the two `windows_bundle.rs` tests asserting the Coach bundle dir name (the
  step-1 display name `.vst3`), the DLL/moduleinfo relative paths from `library_stem = "coach"`, and
  that the moduleinfo carries the `Fx` effect metadata.
- **Verify:** **Red:** the Coach bundle tests don't exist. **Green:** `cargo test -p xtask`. The
  real cross-build (`make build-windows`) and Windows host load are the phase's Windows exit gate,
  performed on the Windows side (see Exit), not in `make ci`.

---

## 9. Update the documentation surface  [depends on #3]

- **File(s):** `AGENTS.md` (Code Map `plugins/coach` row), `CHANGELOG.md`.
- **Reference behavior:** repo-docs conventions — index-style current-state assertions, no future
  tense for shipped state. The Code Map row currently reads *"(Reserved, not yet a workspace
  member)"*; after step 3 it **is** a member with an M0 scaffold. Mirror how the `plugins/cenedril`
  row describes its M0 scaffold.
- **Change:** rewrite the `plugins/coach` Code Map row to assert the current M0 state (member;
  Windows-only passthrough Speech-Coach VST3; bit-exact 0-latency passthrough that **feeds the
  analysis worker**; Vizia editor placeholder; delivery metrics are later milestones). Add a
  `CHANGELOG.md` entry for the Lúmedir M0 scaffold.
- **Verify:** no automated test (doc-only). Gate: `make ci` stays green and the row no longer says
  "not yet a workspace member". (Optional sanity: `grep -n "plugins/coach" AGENTS.md`.)

---

## Phase exit checklist

- [ ] `make ci` green (passthrough bit-exact + allocation-free **with the worker fed**; metadata,
      `vst3_entry`, `coach_vizia` neutral types, and xtask Coach bundle tests all pass on Linux).
- [ ] `make test-integration` runs the gated `sync-analysis` test → the worker yields a correct
      `SignalSnapshot` (voiced/silence) from fed audio.
- [ ] `make build-windows` produces `<name>.vst3`; it loads in the Galad host / a Windows DAW and
      passes audio **bit-exact at 0 latency** with the placeholder editor rendering.
- [ ] Decisions recorded: name **Lúmedir** + metadata string forms (#1); estimator location noted
      (advisory, applied in M1).

Then expand **M1** (the syllable-nuclei speaking-rate estimator) with `plan-phase`.
