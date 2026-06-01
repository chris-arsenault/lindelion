# Galad M1 — Step Plan

Expansion of **M1** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: the **host-side VST3 protocol spike** — prove the `vst3` 0.3.0 crate can drive the
*host* side (scan/load a module, `GetPluginFactory`, enumerate classes, instantiate
`IComponent`/`IAudioProcessor`, set up buses + `ProcessSetup`, call `process()`), with the minimal
`IHostApplication`/`IComponentHandler` plugins require. This is the load-bearing risk; de-risk it
before live audio (M2). **No WASAPI, no UI, no chain** — those are M2/M3/M6.

**Source-of-truth & reference (re-derive from these, not memory):**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) "Context / reuse map" (*build new:* the entire host-side
  protocol; *reference:* `ProcessSetup`/`ProcessContext`/`AudioBuffer` shapes) + "Cross-cutting
  constraints"; [ADR-0002](docs/adr/0002-no-plugin-framework.md) (raw `vst3`, no host framework);
  [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (applies to the M3 audio callback, **not**
  this offline spike — the spike may allocate freely).
- **`vst3` 0.3.0 host API** (`/usr/local/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vst3-0.3.0/src/bindings.rs`,
  `com-scrape-types-0.1.1/src/{ptr,class}.rs`): `ComPtr::{from_raw, as_ptr, cast}`, `ComWrapper::{new,
  to_com_ptr}`, `Class { type Interfaces }`, `Interface::IID` (a `Guid` with `.as_ptr()`).
- **Host-call patterns already in-repo** (the proof the host path runs in-process on Linux):
  `crates/lindelion-plugin-shell/src/vst3_tests.rs:369` `factory_dispatches_class_creation_by_cid` —
  `factory.createInstance(CID.as_ptr(), IPluginBase::IID.as_ptr().cast(), &mut obj)` then
  `ComPtr::from_raw(obj.cast::<I>())`; and `factory_enumerates_registered_classes_through_ipluginfactory`.
- **Guest mirror for the fixture + buffer layout:** `plugins/cenedril/src/vst3_entry/processor.rs`
  (a passthrough `IComponent`+`IAudioProcessor`); `crates/lindelion-plugin-shell/src/vst3_process.rs`
  (how `ProcessData.inputs[0].__field0.channelBuffers32` is laid out — the host builds the mirror it
  reads); `vst3_component.rs:189` `process_setup_from_vst` (`ProcessSetup` fields).
- `TUID = [c_char; 16]`, declared via `vst3::Steinberg::uid(0x..,0x..,0x..,0x..)`; pass a CID/IID to
  `createInstance` as `cid.as_ptr() as FIDString` / `IComponent::IID.as_ptr().cast()`.

## Resolved decisions (the plan's M1 `[DECISION]`)

The plan's M1 `[DECISION]` is *"if the crate cannot host cleanly: fork the binding / drop to raw
`windows`-COM / another path."* **Findings resolve it: `vst3` 0.3.0 hosts cleanly, no fork.** Every
host-side method is present and the COM model (`com-scrape-types`) is **platform-neutral** — no
`#[cfg(windows)]`/winapi in the smart-pointers; the repo already issues host-side `IPluginFactory`
calls in `make ci` on Linux. So M1 proceeds on `vst3` 0.3.0 with **no upfront stop**. The contingency
only re-triggers if Step 3 or 4 hits an actual wall (a method that cannot be called / miscompiles);
if so, **stop and surface findings** before choosing fork vs. raw-COM (per the plan).

**Verification strategy (key consequence of no wine in this environment):**
- The whole host-side protocol — host context, factory enumeration, instantiation, the
  activate/setup/`process()` drive — is **platform-neutral** and is driven **in-process** against a
  small in-crate **fixture plugin**, run automatically by `cargo test -p galad` on Linux. This is the
  automated de-risk and M1's real exit gate.
- The plan's "*loads a real `.vst3` module … first a Lindelion VST3 we control, then a third-party*"
  maps to the **Windows module-loader path** (Step 5–6): it loads the actual `Cenedril.vst3` (built by
  `make build-windows`) then a third-party `.vst3`. A real Windows `.vst3` is a DLL that only loads at
  runtime on Windows; here it is **cross-compile-verified** (`make host-windows-check`) and reuses the
  *same* driver the Linux fixture test exercises (only the factory's source differs). Per the
  no-manual-verification rule, the on-Windows load is **not** a manual exit gate — the automated
  in-process driver test (Step 4) is.
- `host` stays a **general** host: it gains **no dependency on any plugin crate**. The fixture is a
  test-only harness built directly on `vst3`.

All host-side code in M1 is platform-neutral (compiles + the in-process parts test on Linux). `galad`
remains excluded from `make ci`; run its tests with **`cargo test -p galad`** and the Windows
cross-compile with **`make host-windows-check`**.

---

## Step 1 — Host context: `IHostApplication` + `IComponentHandler`
- **File(s):** `host/Cargo.toml` (add `vst3.workspace = true`), `host/src/vst3_host/mod.rs` (new),
  `host/src/vst3_host/host_context.rs` (new), `host/src/main.rs` (add `mod vst3_host;`).
- **Reference behavior:** A `Class` with `type Interfaces = (IHostApplication, IComponentHandler)`
  (gain.rs pattern; `vst3` lib.rs docs). `IHostApplicationTrait::getName(name: *mut String128)` fills
  the host name `"Galad"` as UTF-16 into the `[i16; 128]` (`String128 = [TChar;128]`, `TChar = char16`)
  and returns `kResultOk`; `createInstance(..)` returns `kNotImplemented` (sufficient for the spike —
  plugins that need host-created `IMessage`/`IAttributeList` are out of M1 scope).
  `IComponentHandlerTrait::{beginEdit, performEdit, endEdit, restartComponent}` return `kResultOk`.
  Expose a constructor returning `ComWrapper<HostContext>` and a helper to hand its
  `ComPtr<IHostApplication>` (as `*mut FUnknown` via `.as_ptr().cast()`) to a plugin's `initialize`.
  Use the crate's `kResultOk`/`kNotImplemented` constants (they encode the Win/non-Win values).
- **Change:** add `host_context.rs` (struct `HostContext`, `Class`, the two trait impls, a
  `pub fn new() -> ComWrapper<HostContext>` and a UTF-16 fill for `String128`); `mod.rs` re-exports it;
  register `mod vst3_host;` in `main.rs`.
- **Verify:** `cargo test -p galad` unit test in `host_context.rs`: build the context, get its
  `ComPtr<IHostApplication>`, call `getName` into a zeroed `String128`, decode the UTF-16 up to the NUL
  → assert `"Galad"`; assert `createInstance(..)` returns `kNotImplemented`. **Red:** greenfield — the
  module/types don't exist (fails to resolve). **Green:** name decodes to `"Galad"`.

## Step 2 — In-crate fixture plugin (test harness): passthrough `IComponent`+`IAudioProcessor` behind an `IPluginFactory`  [depends on #1]
- **File(s):** `host/src/vst3_host/fixture.rs` (new, `#[cfg(test)]`), `host/src/vst3_host/mod.rs`
  (register `#[cfg(test)] mod fixture;`).
- **Reference behavior:** Mirror `plugins/cenedril/src/vst3_entry/processor.rs` (the existing passthrough)
  **stripped to a test fixture**, built directly on `vst3` (not via `lindelion-plugin-shell`):
  - `FixtureProcessor` — `Class { Interfaces = (IComponent, IAudioProcessor) }` implementing
    `IPluginBaseTrait` (`initialize`/`terminate` → `kResultOk`), `IComponentTrait`
    (`getBusCount` → 1 for `kAudio`/`kInput` and `kAudio`/`kOutput` else 0; `getBusInfo` → a stereo
    `BusInfo`; `activateBus`/`setActive`/`setIoMode` → `kResultOk`; `getState`/`setState` → `kResultOk`;
    `getControllerClassId`/`getRoutingInfo` → `kNotImplemented`), and `IAudioProcessorTrait`
    (`setBusArrangements` → `kResultOk` only when `numIns==1 && numOuts==1`; `canProcessSampleSize` →
    `kResultOk` for `kSample32` else `kResultFalse`; `getLatencySamples`/`getTailSamples` → 0;
    `setupProcessing`/`setProcessing` → `kResultOk`; `process` → a **bit-exact** per-channel copy
    `inputs[0].channelBuffers32[c][i] → outputs[0].channelBuffers32[c][i]`, mirroring `vst3_process.rs`'s
    layout, writing silence to a channel when its input pointer is absent).
  - `FixtureFactory` — `Class { Interfaces = (IPluginFactory, IPluginFactory2) }` exposing exactly one
    class (a fixed `FIXTURE_CID` via `uid(..)`, category `"Audio Module Class"`); `countClasses` → 1;
    `getClassInfo`/`getClassInfo2` → fill cid/category/name; `createInstance(cid, iid, obj)` → if
    `*(cid as *const TUID) == FIXTURE_CID`, `ComWrapper::new(FixtureProcessor::new())` then
    `queryInterface(iid, obj)` (mirror `vst3_factory.rs:120` `createInstance`).
- **Change:** add `fixture.rs` with both types + a `pub(crate) fn fixture_factory() -> ComPtr<IPluginFactory>`
  (via `ComWrapper::new(FixtureFactory).to_com_ptr()`); register the cfg-gated module.
- **Verify:** `cargo test -p galad` in `fixture.rs`: `fixture_factory().countClasses() == 1`, and
  `createInstance(FIXTURE_CID.as_ptr(), IComponent::IID.as_ptr().cast(), &mut obj)` returns `kResultOk`
  with a non-null `obj` that `ComPtr::from_raw(obj.cast::<IComponent>())` then `.cast::<IAudioProcessor>()`
  both succeed. **Red:** greenfield — `fixture_factory`/types absent. **Green:** the interfaces resolve.

## Step 3 — Factory driver: enumerate classes, instantiate, query, initialize → `PluginInstance`  [depends on #1, #2]
- **File(s):** `host/src/vst3_host/instance.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** Given a `ComPtr<IPluginFactory>` (from the fixture in tests, or a loaded
  module in Step 5), the host: casts to `IPluginFactory2` (if present) and walks `0..countClasses()`,
  reading `PClassInfo2`/`PClassInfo`, to find the class whose `category` C-string is
  `"Audio Module Class"` (the audio-processor class — see `vst3_factory.rs:36`); calls
  `createInstance(class.cid.as_ptr(), IComponent::IID.as_ptr().cast(), &mut obj)` and
  `ComPtr::from_raw(obj.cast::<IComponent>())`; `component.initialize(host_ctx_funknown_ptr)` (Step 1
  context, as `*mut FUnknown`); `component.cast::<IAudioProcessor>()` for the processor. Hold both
  `ComPtr`s in `PluginInstance { component, processor }`; implement `Drop` to call
  `component.setActive(0)` then `component.terminate()` (idempotent teardown). All reference-correct
  per the `vst3_tests.rs:369` host-call pattern.
- **Change:** add `instance.rs` with `PluginInstance`, a constructor
  `PluginInstance::from_factory(factory: &ComPtr<IPluginFactory>, host: &ComPtr<IHostApplication>) ->
  Result<Self, HostError>`, a small `HostError` enum (no audio-class found / `createInstance` failed /
  `IAudioProcessor` missing), and `Drop`.
- **Verify:** `cargo test -p galad`: drive `fixture_factory()` (Step 2) + host context (Step 1) through
  `PluginInstance::from_factory`; assert it returns `Ok` and that the instance exposes a non-null
  `IAudioProcessor` (`getLatencySamples()` is callable and returns 0). Add a negative test: a stub
  factory reporting 0 classes → `Err(no audio class)`. **Red:** greenfield — `PluginInstance` absent.
  **Green:** `Ok` for the fixture, `Err` for the empty factory.

## Step 4 — Process driver: activate → `setupProcessing` → `process()`; host `ProcessData` builder + known-signal assertions  [depends on #3]
- **File(s):** `host/src/vst3_host/processing.rs` (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** This is the core de-risk. Drive the canonical offline host sequence against a
  `PluginInstance`: `processor.setBusArrangements(&[kStereo], 1, &[kStereo], 1)`;
  `processor.canProcessSampleSize(kSample32) == kResultOk`; `processor.setupProcessing(&mut ProcessSetup
  { processMode: kRealtime, symbolicSampleSize: kSample32, maxSamplesPerBlock, sampleRate })`;
  `component.activateBus(kAudio, kInput/kOutput, 0, 1)`; `component.setActive(1)`;
  `processor.setProcessing(1)`; build `ProcessData` and call `processor.process(&mut data)`; then unwind
  (`setProcessing(0)`, `setActive(0)`). Build `ProcessData` exactly mirroring the layout
  `vst3_process.rs` reads: host-owned `Vec<Vec<f32>>` per channel for **input** and **output** (distinct
  backing, no aliasing), a `Vec<*mut f32>` of channel pointers per bus, one `AudioBusBuffers` per bus
  (`numChannels`, `silenceFlags: 0`, `__field0.channelBuffers32 = ptr`), and
  `ProcessData { processMode: kRealtime, symbolicSampleSize: kSample32, numSamples, numInputs: 1,
  numOutputs: 1, inputs, outputs, inputParameterChanges/outputParameterChanges/inputEvents/outputEvents/
  processContext: null }`. Provide a helper that runs one block: input channels → process → returns the
  output channels.
- **Change:** add `processing.rs` with a `ProcessBlock` owner (holds the backing buffers + bus structs so
  pointers stay valid across `process()`), `fn prepare(processor, setup)`, and
  `fn process_block(&mut self, instance, input: &[&[f32]]) -> Vec<Vec<f32>>`. May allocate freely (offline
  spike — ADR-0001 does not apply here; that's M3).
- **Verify:** `cargo test -p galad` against the fixture: (a) **silence stays silent** — all-zero stereo
  input → all-zero output; (b) **sine passes with unity gain / 0 latency** — a 1 kHz stereo sine (e.g.
  480 samples @ 48 kHz) → output **bit-exact equals** input (the fixture is a verbatim passthrough),
  and `processor.getLatencySamples() == 0`. **Red:** greenfield — `process_block` absent (and a stub
  that leaves output zeroed fails the sine equality). **Green:** silence→silence and sine→sine hold.
  *(This automated test is M1's exit gate: a real plugin object is driven end-to-end and processes a
  known signal correctly.)*

## Step 5 — Windows module loader: `.vst3` → `GetPluginFactory` → `ComPtr<IPluginFactory>`  [depends on #3]
- **File(s):** `host/Cargo.toml` (add `libloading = "0.8"`, host-only), `host/src/vst3_host/module.rs`
  (new), `host/src/vst3_host/mod.rs` (register).
- **Reference behavior:** A VST3 module is a dynamic library; the host opens it and resolves the C entry
  points. Use `libloading` (cross-platform: compiles + runs on Linux, loads the OS library at runtime).
  Resolve the optional `InitDll: extern "system" fn() -> bool` (call if present; Windows convention),
  then the required `GetPluginFactory: extern "system" fn() -> *mut IPluginFactory`; wrap its result with
  `ComPtr::from_raw`. Keep the `libloading::Library` alive alongside the factory (define
  `LoadedModule { _library: Library, factory: ComPtr<IPluginFactory> }`) so the DLL is not unloaded while
  the factory lives; on drop, resolve+call `ExitDll` if present, then drop the library. The real `.vst3`
  on Windows lives at `<Bundle>.vst3/Contents/x86_64-win/<Bundle>.vst3` (the DLL renamed `.vst3` — see
  `xtask/src/windows_bundle.rs`); accept a path to that file.
- **Change:** add `module.rs` with `fn load_module(path: &Path) -> Result<LoadedModule, HostError>` and a
  `ModuleLoad` variant on `HostError`; declare the `libloading` dep.
- **Verify:** `cargo test -p galad` (Linux-runnable error path): `load_module(Path::new("/nonexistent.vst3"))`
  returns `Err(HostError::ModuleLoad(..))` (libloading fails to open). **Red:** greenfield — `load_module`
  absent. **Green:** the missing-file load returns the typed error. The **success path** (opening a real
  Windows `.vst3` and getting a live factory) is **cross-compile-verified** by `make host-windows-check`
  and exercised at runtime on Windows in Step 6 — it is not a manual gate here.

## Step 6 — Headless spike runner (load real `Cenedril.vst3` + a third-party, run the driver) + document the host-side API shape  [depends on #4, #5]
- **File(s):** `host/src/vst3_host/spike.rs` (new), `host/src/main.rs` (wire a `spike <plugin.vst3>`
  subcommand that calls it), `host/README.md` (a "Host-side VST3 protocol" section).
- **Reference behavior:** Compose the pieces end-to-end on a real module: `load_module(path)` (Step 5) →
  `PluginInstance::from_factory` with a `HostContext` (Steps 1, 3) → `process_block` a known signal
  (Step 4) → print the audio class name, latency, and a pass/fail on silence→silence and sine→sine. This
  is the plan's "headless run loads a real VST3 and processes a known signal correctly," run on Windows
  against `Cenedril.vst3` first (the Lindelion VST3 we control) then a third-party `.vst3`. The
  **documentation** exit: add a concise "Host-side VST3 protocol" section to `host/README.md` describing
  the verified flow (module → `GetPluginFactory` → enumerate `Audio Module Class` → `createInstance`
  `IComponent` → `initialize(IHostApplication)` → `cast IAudioProcessor` → `setBusArrangements` →
  `setupProcessing` → `activateBus`/`setActive`/`setProcessing` → build `ProcessData` → `process` →
  unwind), naming the `vst3` types at each step, plus thorough module docs on `vst3_host`.
- **Change:** add `spike.rs` (`fn run_spike(path: &Path) -> Result<SpikeReport, HostError>` reusing
  Steps 1/3/4/5; a `SpikeReport { class_name, latency_samples, silence_ok, sine_ok }`); a `spike`
  subcommand in `main.rs`; the README section + `vst3_host` module docs.
- **Verify:** `make host-windows-check` cross-compiles the spike for `x86_64-pc-windows-msvc` (the runner
  links cleanly). The **automated behavioral proof** is Step 4's in-process driver test (same driver code);
  the on-Windows run of `run_spike` against real `.vst3`s is the field check performed when Galad runs on
  Windows (M3+), **not** a manual M1 gate (no human-audition exit). **Red:** greenfield — `run_spike`/the
  subcommand don't exist (cross-compile/`-p galad` fails to resolve). **Green:** cross-compiles and
  `cargo test -p galad` stays green; the README documents the host-side API shape.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| (phase) | Can `vst3` 0.3.0 host cleanly, or fork / raw-COM? | **Resolved by findings:** hosts cleanly (all host methods present; `com-scrape-types` COM is platform-neutral; repo already makes host-side factory calls on Linux). Proceed on `vst3` 0.3.0; contingency re-triggers only if Step 3/4 hits a real wall. |
| (phase) | How to verify with no wine in this env. | In-process fixture driver = automated exit (`cargo test -p galad`, Linux); real `.vst3` DLL load = cross-compile-verified (`make host-windows-check`) + runtime on Windows. No manual-audition gate. |
| 4 | Does the host stay plugin-agnostic? | **Yes:** fixture is a test-only harness on `vst3`; `host` gains no plugin-crate dependency. Real Cenedril/third-party are loaded as modules (Step 6). |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. **No upfront `[DECISION]` stop** — the
phase decision is resolved above; only re-engage it if Step 3 or 4 hits an unhostable wall, in which
case stop and surface findings. Steps 1–5 are verified automatically by `cargo test -p galad` on Linux
(host excluded from `make ci`); Step 5's success path and Step 6 are cross-compile-verified by
`make host-windows-check`, with the real-`.vst3` runtime check happening when Galad runs on Windows.
M1 done de-risks the whole approach; M2 (WASAPI engine) and M3 (drive the chain from the RT callback)
follow — one milestone at a time.
