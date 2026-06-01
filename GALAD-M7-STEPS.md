# Galad M7 — Robustness, validation, real-world soak — Steps

Expands **M7** of [GALAD-HOST-PLAN.md](GALAD-HOST-PLAN.md) (`### M7 — Robustness, validation,
real-world soak [depends on M4, M6]`) into red→green steps. Milestone scope (verbatim): *device
hot-swap/disconnect recovery; plugin scan + incompatible/misbehaving-plugin handling that cannot
crash the host; error surfaces; validation across a matrix of real third-party VST3s plus the
Lindelion VST3s (Calóma once it ships); stability/leak soak.*

Source-of-truth re-derived this pass: `host/src/audio/wasapi/engine.rs` (`run_loop` swallows runtime
faults — breaks the loop and the thread dies silently; only the *setup* `Result` is ever sent to the
control thread), `host/src/audio/wasapi/devices.rs` (`AudioError::{Com,UnsupportedFormat,ThreadSetup}`;
device invalidation arrives as a `Com` error through the pump `?`), `host/src/vst3_host/chain.rs`
(`process_in_place` writes plugin output straight to the device — NaN/Inf propagates),
`host/src/vst3_host/instance.rs` (`HostError::{NoAudioClass,MissingAudioProcessor,…}` already contain
incompatible-plugin load), `host/src/ui/vizia_app.rs` (controller failure arms only `eprintln!` today),
`host/src/ui/state.rs` (`scan_dir` lists `*.vst3`; no error/notice field), `host/src/session.rs`
(`AppSettings.plugin_scan_dirs` already persisted). ADRs: [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md)
(audio-thread fault signalling must be a non-blocking atomic, never a lock/channel the audio thread
waits on), [ADR-0022](docs/adr/0022-windows-vst3-host.md) (target-gated; live behaviour is a Windows
field check).

## Verification strategy (same split as M2–M6)

- **Neutral core, Linux-tested** (`cargo test -p galad`): the engine fault-status enum, the hostile
  test fixtures, the load-time validation probe, the NaN/Inf output guard, the error/notice state +
  the controller's fault reaction, and the plugin-catalog assembly — all in-memory, no fs/threads/clock.
- **Windows, cross-compile-verified** (`make host-windows-check`): the audio-thread fault store, the
  controller wiring of recovery + error display, the scan UI.
- **Windows-runtime field checks (NOT `make ci` gates)** — consistent with M2–M6 and the "no human
  verification by default except the accepted galad field checks" discipline: live device unplug/replug
  survival, the third-party plugin matrix, and the leak/soak run. These produce **documented
  artifacts** (`host/PLUGIN-MATRIX.md`, a soak note), not automated assertions.
- **Filesystem-touching tests** (the real folder scan) go behind the per-crate `integration-tests`
  feature with `#[cfg_attr(not(feature = "integration-tests"), ignore = "…")]` (AGENTS: `make ci` units
  stay in-memory); the pure catalog logic stays a fast unit test.

Exit gate for the phase: **`make ci` green on Linux (host excluded)** *and* **`make host-windows-check`
green**; the field artifacts (matrix, soak note) exist and are field-populated on Windows.

## Decisions to make (lifted to their owning steps)
- **Step 5 — containment depth** `[DECISION]`: in-process best-effort + documented residual limits,
  vs an out-of-process plugin sandbox (re-plans M7).
- **Step 7 — device-disconnect recovery policy** `[DECISION]`: stop+notify vs auto-restart-on-return
  vs auto-fallback-to-default.

Everything else is decided internally (durable-but-simple data shapes; type-safe status enums over
stringly state) per the "decide internal impl yourself" discipline.

---

## Step 1 — Engine runtime fault status (audio thread → control thread)  [windows + neutral]
- **File(s):** `host/src/audio/engine_status.rs` (new, neutral — the `EngineStatus` enum + `u8`
  mapping), `host/src/audio/mod.rs` (`mod engine_status; pub use …`), `host/src/audio/wasapi/engine.rs`
  (an `Arc<AtomicU8>` set by the audio thread on loop exit; `AudioEngine::status()` reader).
- **Reference behavior:** `run_loop` (engine.rs:230–278) `break`s on any pump error and the thread
  returns — the control thread is never told (only the setup `Result` is sent, engine.rs:170). ADR-0001:
  the audio thread must publish status via a non-blocking atomic, never block. Distinguish a *user* stop
  (the `stop` flag is set) from a *fault* (loop exited with `stop` still clear).
- **Change:** neutral `EngineStatus { Running, StoppedByUser, Faulted }` with `as_u8`/`from_u8`; the
  engine holds `Arc<AtomicU8>`, stores `Faulted` (or `StoppedByUser`) at `run_loop` exit, and exposes
  `AudioEngine::status() -> EngineStatus`. **Observability only — no recovery yet.**
- **Verify:** neutral unit test for the `EngineStatus ⇄ u8` round-trip and the default (`Running`).
  **Red:** the enum/`status()` don't exist (greenfield — fails to resolve). The audio-thread store is
  cross-compile-verified; the live fault is exercised in Step 7's field check.

## Step 2 — Hostile test fixtures (incompatible + misbehaving plugins)  [neutral test infra]
- **File(s):** `host/src/vst3_host/fixture.rs` (add variants + constructors).
- **Reference behavior:** the existing `FixtureProcessor` (gain/latency, fixture.rs:23–227) and
  `FixtureController`; the factory exposes classes via `countClasses`/`getClassInfo`/`createInstance`.
- **Change:** add fixtures for (a) **no audio class** (a factory exposing only a controller / no
  `"Audio Module Class"`), (b) **process-returns-error** (`process` → `kResultFalse`), (c) **NaN-emitter**
  (`process` writes `f32::NAN`/`INFINITY` into the output). A panicking/UB plugin is **out of in-process
  reach** — note it in the doc comment as the boundary case (it informs Step 5), do not add an aborting
  fixture to the unit suite.
- **Verify:** a test that instantiates each new fixture compiles and constructs. **Red:** the variants
  don't exist (greenfield). Consumed by Steps 3–4.

## Step 3 — Load-time validation probe that contains incompatible plugins  [neutral, vs fixtures]  [depends on #2]
- **File(s):** `host/src/vst3_host/validate.rs` (new), `host/src/vst3_host/mod.rs` (export),
  `host/src/ui/vizia_app.rs` (`add_plugin` probes before adding).
- **Reference behavior:** `load_module`/`PluginInstance::from_factory`/`ChainProcessor::new` already
  return `HostError` for incompatible plugins, but `add_plugin` (vizia_app.rs) pushes the module *before*
  any validation, so a broken plugin becomes a broken slot. The M1/M3 path (`processing::ProcessDriver`)
  already drives one block.
- **Change:** `validate_plugin(path) -> Result<(), HostError>` = load → instantiate → setup → process
  one **silent** block → teardown, returning the first error without panicking; `add_plugin` calls it
  first and **rejects** on `Err` (surfaces the error — Step 6 — and does not add the slot).
- **Verify:** neutral test — `validate_plugin` (driven against the fixture factory in-process) returns
  `Err` for the no-audio-class and error-returning fixtures and `Ok` for the gain fixture. **Red:**
  `validate_plugin` doesn't exist (greenfield).

## Step 4 — Audio-thread output sanitation (no NaN/Inf reaches the device)  [neutral DSP, vs NaN fixture]  [depends on #2]
- **File(s):** `host/src/vst3_host/chain.rs` (finite-guard the chain output).
- **Reference behavior:** `ChainProcessor::process_in_place` (chain.rs:61–90) copies plugin output
  straight back to the interleaved device block; a misbehaving plugin's NaN/Inf propagates to the output
  device. ADR-0001: the guard must be allocation-free (a per-sample branch, no buffers).
- **Change:** after the chain runs, replace any non-finite output sample with `0.0` (`if
  !s.is_finite() { 0.0 }`), in place.
- **Verify:** neutral test — push a block through a `ChainProcessor` built from the NaN-emitter fixture
  and assert every output sample `is_finite()`; wrap the process call in `assert_no_allocations!`.
  **Red:** today NaN passes straight through.

## Step 5 — Containment depth  [DECISION]
- **File(s):** none until decided; then either `host/README.md` (document residual limits) or a re-plan.
- **Reference behavior:** Steps 3–4 contain *incompatible* and *NaN-emitting* plugins in-process. The
  milestone's "cannot crash the host" is only **fully** achievable out-of-process — an in-process plugin
  that aborts, corrupts memory, or hangs in its own (C++) code can still take Galad down; `catch_unwind`
  on the host side does **not** catch a foreign-code crash or a panic raised inside the plugin's own FFI
  boundary.
- **Decision:** **(recommended) in-process best-effort** — Steps 3–4 plus a documented residual-limits
  note in `host/README.md` (matches the lightweight-host scope and ADR-0001); **or** commit to an
  **out-of-process sandbox** (subprocess-per-plugin with shared-memory audio + IPC), which is a large
  architecture change and **re-plans M7**. The executor stops here.
- **Verify:** n/a (decision). If in-process: the deliverable is the documented-limits note; if sandbox:
  M7 is re-expanded around process isolation.

## Step 6 — Error surfaces + engine-fault reaction in the UI  [neutral state + cross-compile]  [depends on #1, #3]
- **File(s):** `host/src/ui/state.rs` (`notice: Option<String>` + `set_notice`/`clear_notice`; excluded
  from `to_session`), `host/src/ui/vizia_app.rs` (controller sets a notice on every failure path instead
  of `eprintln!`-only; the meter tick reads `engine.status()` and, on `Faulted`, stops + sets a notice;
  the existing `status` signal renders the notice).
- **Reference behavior:** the controller's failure arms (start/add/save/load) currently only `eprintln!`
  (vizia_app.rs); `HostUiState` has no error field; the Vizia `status` signal shows only running/stopped.
  Step 1 added `engine.status()`.
- **Change:** add the neutral `notice` field + transitions; the controller records failures into it and
  clears it on success; on a `Faulted` engine status the tick reaction sets `running = false` + a notice
  (the recovery *policy* beyond stop-and-notify is Step 7); `sync` maps the notice into `status`.
- **Verify:** neutral test — `set_notice`/`clear_notice` round-trip and that `to_session` ignores the
  notice (transient, not persisted). **Red:** no `notice` field (greenfield). The Vizia rendering + the
  live `status()` read are cross-compile-verified.

## Step 7 — Device-disconnect recovery policy  [DECISION]  [depends on #1, #6]
- **File(s):** `host/src/ui/vizia_app.rs` (the tick reaction implements the chosen policy); a neutral
  helper in `host/src/ui/state.rs` if the policy needs a pure state transition.
- **Reference behavior:** Step 1 surfaces `Faulted`; Step 6 already does the minimal stop+notify. Device
  invalidation (unplug) reaches the audio thread as a `Com` error → `Faulted`. The control thread can
  re-`enumerate`/`default_device` (devices.rs) to find a replacement.
- **Decision:** pick the recovery behaviour — **(a) stop + notify** (user re-selects + re-starts;
  smallest, deterministic), **(b) auto-restart on the same device** when it reappears, or **(c)
  auto-fallback to the new default device**. UX the user owns. The executor stops here.
- **Verify:** neutral test of the chosen reaction (e.g. for (a): `Faulted` ⇒ `running == false` + a
  notice; for (b)/(c): the controller's device-reselection helper picks the expected replacement from a
  given device list). **Red:** the reaction/helper doesn't exist. **Live unplug/replug survival is the
  Windows field check** (not a `make ci` gate).

## Step 8 — Plugin scan into a validated catalog  [neutral catalog + integration fs test]  [depends on #3]
- **File(s):** `host/src/ui/state.rs` (a `PluginCatalog` model — discovered paths + per-path validation
  status), `host/src/ui/vizia_app.rs` ("scan folder" → populate an add-from list using Step 3's probe),
  `host/src/session.rs` (`AppSettings.plugin_scan_dirs` already exists — persist the chosen dirs).
- **Reference behavior:** `HostUiState::scan_dir` (state.rs:106) already lists `*.vst3` under a dir;
  `AppSettings.plugin_scan_dirs` is already serialized. The milestone names "plugin scan."
- **Change:** assemble a `PluginCatalog` from `(path, validation-status)` pairs (validation via Step 3),
  surfaced as the add-plugin source; the actual fs walk stays `scan_dir`. Keep it lean (no background
  rescan, no persisted catalog cache — just dirs + on-demand scan).
- **Verify:** **unit** — `PluginCatalog` assembly from a supplied list of `(path, Result)` pairs
  (in-memory, no fs). **Integration** (`integration-tests`, `#[cfg_attr(not(…), ignore)]`) — `scan_dir`
  over a temp dir finds the `.vst3` entries. **Red:** the catalog type doesn't exist.

## Step 9 — Plugin compatibility matrix (field-populated doc)  [Windows field check + artifact]
- **File(s):** `host/PLUGIN-MATRIX.md` (new).
- **Reference behavior:** milestone — "validation across a matrix … a documented plugin matrix
  loads/processes/edits correctly."
- **Change:** create the matrix skeleton — columns *plugin, vendor, version, load, process, editor,
  notes* — seeded with the Lindelion Windows VST3s (Cenedril; Calóma once it ships) and rows for the
  third-party set; field-populated on Windows.
- **Verify:** **not a `make ci` gate.** The doc + structure is the deliverable; load/process/edit results
  are recorded by running real plugins in Galad on Windows (field check).

## Step 10 — Stability/leak soak (procedure + result)  [Windows field check + artifact]
- **File(s):** `host/README.md` (a "Soak" subsection) or an appendix in `host/PLUGIN-MATRIX.md`.
- **Reference behavior:** milestone — "no leaks over a soak run."
- **Change:** document the soak procedure (multi-hour run with periodic chain edits + device
  hot-swaps; watch RSS / GDI+USER handle counts) and record the observed result.
- **Verify:** **not a `make ci` gate** — the soak is a Windows-runtime field activity; the recorded
  result is the deliverable.

---

## Decisions table
| Step | Decision | Status |
| ---- | -------- | ------ |
| 5 | Misbehaving-plugin containment depth (in-process best-effort + documented limits **vs** out-of-process sandbox). | **Open — `[DECISION]`, recommend in-process best-effort.** |
| 7 | Device-disconnect recovery policy (stop+notify **vs** auto-restart-on-return **vs** auto-fallback-to-default). | **Open — `[DECISION]`.** |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. **Two `[DECISION]` stops** (Steps 5 and 7) —
the executor stops and asks before each; everything else is decided. Steps 1–4, 6, 8 are verified
automatically (`cargo test -p galad` on Linux + `make host-windows-check`); Steps 9–10 and the live
halves of Steps 1/7 (unplug/replug survival, the third-party matrix, the soak run) are **Windows-runtime
field checks with documented artifacts, not `make ci` gates**. M7 completes Galad's robustness pass; it
is the last planned milestone. One step at a time, in order.
