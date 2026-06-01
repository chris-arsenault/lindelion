# Galad M2 — Step Plan

Expansion of **M2** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: the **native WASAPI audio engine** — enumerate input/output devices, negotiate
format/sample-rate/buffer, exclusive-mode primary with shared-mode fallback, a **lock-free realtime
duplex callback** (capture → ring → render), glitch-free start/stop and device selection.
**Passthrough first: no plugins** (the VST3 chain joins in M3). This depends only on M0; it does not
touch `vst3_host/` (M1).

**Source-of-truth & reference (re-derive from these, not memory):**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) "Context / reuse map" (*build new:* WASAPI engine — device
  enumeration, format negotiation, exclusive/shared, lock-free RT duplex callback, ring buffers) +
  "Cross-cutting constraints" (target-gated Windows-only; WASAPI deps never leak into shared crates;
  **single channel:** one input → one chain → one output).
- [ADR-0022](docs/adr/0022-windows-vst3-host.md): **native WASAPI**, exclusive-mode primary +
  shared-mode fallback, lock-free callback — **not `cpal`**.
- [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md): **now applies** to the host's audio
  callback — the per-callback transport allocates nothing and takes no locks. Use
  `lindelion-test-allocator` (`install_test_allocator!()` + `assert_no_allocations`, mirrored from
  `crates/lindelion-dsp-utils/src/lib.rs:19` and its `envelope_follower.rs:103` usage).
- **Reuse:** `host/src/session.rs::DeviceRef { id, name }` is the serializable device identity that
  WASAPI enumeration produces (so a selected device round-trips into the session model).
- **WASAPI binding:** the `windows` crate 0.62 (already a unified transitive dep; `Win32::Media::Audio`
  is present at `…/windows-0.62.2/src/Windows/Win32/Media/Audio/mod.rs`).

## Resolved decisions (mine to make)

- **WASAPI binding = the raw `windows` crate** (not the `wasapi` wrapper, not `cpal`). Rationale:
  exclusive-mode + event-driven duplex needs full `IAudioClient` control (`Initialize` with
  `AUDCLNT_STREAMFLAGS_EVENTCALLBACK`, `IsFormatSupported`, `GetDevicePeriod`); the `windows` crate is
  the ADR-0022 "native WASAPI" path and is already version-unified in the lockfile. Added
  **target-gated**: `[target.'cfg(windows)'.dependencies] windows = { version = "0.62", features = […] }`
  so it never enters the Linux/macOS build. The WASAPI code lives under `#[cfg(windows)] mod wasapi;`.

**Verification strategy (no WASAPI / no wine on Linux):** the engine splits into a **platform-neutral
core** — lock-free ring, sample-format conversion, channel adaptation, mode/format **negotiation
logic**, and the per-callback **transport** — all compiled and **tested automatically on Linux**
(`cargo test -p galad`, incl. `assert_no_allocations`); and a thin **Windows COM shell**
(`audio/wasapi/`) — device enumeration, `IAudioClient` setup, the event-driven RT thread — which is
**cross-compile-verified** by `make host-windows-check`. The live "mic → output glitch-free + measured
round-trip latency" is a **Windows-runtime field check** recorded when Galad runs on hardware (the
same code the Linux transport test exercises), **not** a manual gate here (no human-audition exit).

`galad` stays excluded from `make ci`; run its tests with **`cargo test -p galad`** and the Windows
cross-compile with **`make host-windows-check`**.

---

## Step 1 — Lock-free SPSC ring buffer (the capture→render hand-off)
- **File(s):** `host/src/audio/mod.rs` (new), `host/src/audio/ring.rs` (new); `host/src/main.rs`
  (add `mod audio;` and, gated, the counting allocator: `#[cfg(test)]
  lindelion_test_allocator::install_test_allocator!();`); `host/Cargo.toml`
  (`[dev-dependencies] lindelion-test-allocator.workspace = true`).
- **Reference behavior:** A fixed-capacity single-producer/single-consumer `f32` ring with atomic
  read/write indices (`AtomicUsize`, `Ordering::Acquire`/`Release`), preallocated backing
  (`Box<[f32]>` built once at construction), and **allocation-free, lock-free** `push`/`pop` of slices
  that return the count moved and never block (ADR-0001). Power-of-two capacity for cheap masking.
  This is the "ring buffers" the plan lists as build-new; no prior art in the repo (the `worker.rs`
  files are job queues, not audio rings).
- **Change:** add `ring.rs` (`struct AudioRing` + `fn with_capacity`, `fn push(&self, &[f32]) -> usize`,
  `fn pop(&self, &mut [f32]) -> usize`, `fn available`/`fn free`); `audio/mod.rs` declares `mod ring;`;
  wire the dev-dep + `install_test_allocator!` + `mod audio;`.
- **Verify:** `cargo test -p galad`: (a) push N then pop N returns the same samples in order, including a
  wraparound that crosses the capacity boundary; (b) `assert_no_allocations("ring push/pop", …)` around
  a fill+drain cycle on a pre-built ring. **Red:** greenfield — `AudioRing` absent. **Green:** ordering
  + wraparound hold and zero allocations.

## Step 2 — Sample-format conversion and the `StreamFormat` descriptor  [depends on #1]
- **File(s):** `host/src/audio/format.rs` (new), `host/src/audio/mod.rs` (register).
- **Reference behavior:** WASAPI presents audio as interleaved frames in a concrete sample type —
  shared mode is typically 32-bit float; exclusive mode is often 16-bit PCM. Model a platform-neutral
  `SampleFormat { F32, I16 }` and `StreamFormat { sample_rate: u32, channels: u16, sample: SampleFormat }`,
  with conversion **to/from the engine's interleaved `f32`**: `i16` ⇄ `f32` via `/ 32768.0` and
  `(x * 32767.0).clamp(-32768,32767)`, `f32` identity. (i24/i32 are a documented later extension — start
  with the two common cases.)
- **Change:** add `format.rs` with the enums + `fn bytes_to_f32(format, &[u8], &mut [f32])` and
  `fn f32_to_bytes(format, &[f32], &mut [u8])` (or `&[i16]`/`&[f32]` typed variants); no allocation.
- **Verify:** `cargo test -p galad`: `i16` → `f32` → `i16` round-trips within ±1 LSB across the range
  (`i16::MIN/MAX/0`); `f32` is identity. **Red:** greenfield — the converters don't exist. **Green:**
  round-trip tolerance holds.

## Step 3 — Channel adaptation for the single chain (mono ⇄ stereo)  [depends on #1] [DECISION]
- **File(s):** `host/src/audio/channels.rs` (new), `host/src/audio/mod.rs` (register).
- **Reference behavior:** The chain runs **stereo** (VST3 plugins are stereo); a mic is often **mono**;
  output devices may be mono or stereo. Adapt interleaved frames between an N-channel device stream and
  the 2-channel chain. **Recommended semantics (the [DECISION]):** capture **mono → stereo** by
  duplicating the sample to L and R (centered mono); **stereo → stereo** verbatim. Render **stereo →
  mono** by averaging `(L+R)/2`; **stereo → stereo** verbatim. Equal channel counts are bit-exact
  passthrough. (Alternative the user may pick: −3 dB mono fold-down, or L-only mono capture.)
- **Change:** add `channels.rs` with `fn adapt(src_channels, dst_channels, &[f32], &mut [f32])`
  (interleaved), allocation-free.
- **Verify:** `cargo test -p galad`: mono→stereo duplicates each sample to both outputs; stereo→mono
  averages; equal counts are bit-exact. **Red:** greenfield. **Green:** the three mappings hold.
- **[DECISION]:** confirm the mono⇄stereo mapping above (duplicate up / average down) for the single
  chain, or specify a different fold-down. The executor stops here.

## Step 4 — Stream mode + format negotiation policy (exclusive vs shared, buffer size)  [DECISION]
- **File(s):** `host/src/audio/negotiation.rs` (new), `host/src/audio/mod.rs` (register).
- **Reference behavior:** Pure decision logic over descriptors (the Windows `IsFormatSupported`/
  `GetDevicePeriod` calls live in Step 7; this is the **rule** they feed). Given a requested policy and
  what a device reports, choose **share mode** and **buffer frame count**: try **exclusive** first
  (lowest latency); fall back to **shared** when exclusive is unsupported. Compute buffer frames from a
  device period (minimum period in exclusive, default in shared) and the sample rate. **Recommended
  policy (the [DECISION]):** exclusive event-driven primary, shared event-driven fallback; buffer = the
  device minimum period (exclusive) / default period (shared); **round-trip latency target ≈ ≤10 ms**,
  with the *measured* value recorded at the exit (the target is a goal, not a hard gate).
- **Change:** add `negotiation.rs`: a `SharePolicy`/`StreamConfig { share_mode, format, buffer_frames }`
  and a pure `fn choose(requested, exclusive_supported: bool, periods…) -> StreamConfig`; a
  `LATENCY_TARGET` constant.
- **Verify:** `cargo test -p galad`: `choose(..)` picks exclusive when supported and falls back to shared
  when not; buffer-frame computation matches `period × sample_rate`. **Red:** greenfield. **Green:** the
  selection + arithmetic hold.
- **[DECISION]:** confirm the **latency target (~≤10 ms)** and the **exclusive-primary / shared-fallback
  buffer policy** (minimum vs default period), or set different numbers. The executor stops here.

## Step 5 — Passthrough transport (the per-callback step the RT thread runs)  [depends on #1, #3]
- **File(s):** `host/src/audio/transport.rs` (new), `host/src/audio/mod.rs` (register).
- **Reference behavior:** The platform-neutral body the realtime callback invokes, written once and
  shared by the Linux test and the Windows engine. Capture side: take a slice of interleaved device
  `f32` frames, channel-adapt (Step 3) to stereo, `push` to the ring (Step 1). Render side: `pop` stereo
  frames from the ring, channel-adapt to the output device's channels, write into the output slice;
  under-run (ring empty) writes silence. **Allocation-free and lock-free** (ADR-0001) — operates on
  caller-provided scratch slices, no per-call allocation.
- **Change:** add `transport.rs` with `struct Transport` (holds the ring + channel counts + preallocated
  scratch) and `fn capture(&self, device_frames: &[f32])` / `fn render(&self, out: &mut [f32])`.
- **Verify:** `cargo test -p galad` (synthetic loopback, no hardware): push a sine through
  `capture` then drain via `render`, assert the output equals the input after channel adaptation; wrap a
  `capture`+`render` cycle in `assert_no_allocations`. **Red:** greenfield — `Transport` absent (and a
  stub that drops samples fails the equality). **Green:** the signal flows through correctly with zero
  allocations. *(This is M2's automated de-risk: the realtime transport logic proven without a device.)*

## Step 6 — WASAPI device enumeration (Windows COM)  [windows]
- **File(s):** `host/Cargo.toml` (target-gated `windows` dep), `host/src/audio/wasapi/mod.rs` (new),
  `host/src/audio/wasapi/devices.rs` (new), `host/src/audio/mod.rs` (add `#[cfg(windows)] mod wasapi;`).
- **Reference behavior:** Standard WASAPI device discovery: `CoInitializeEx`; `CoCreateInstance` of
  `MMDeviceEnumerator` → `IMMDeviceEnumerator`; `EnumAudioEndpoints(eCapture | eRender,
  DEVICE_STATE_ACTIVE)` → for each `IMMDevice`, `GetId()` → the endpoint id and `OpenPropertyStore` →
  `PKEY_Device_FriendlyName` → the display name; `GetDefaultAudioEndpoint` for the default. Produce
  `Vec<DeviceRef>` reusing `crate::session::DeviceRef { id, name }`. Add the `windows` features the
  calls require (start with `Win32_Media_Audio`, `Win32_System_Com`, `Win32_System_Com_StructuredStorage`,
  `Win32_Foundation`, `Win32_UI_Shell_PropertiesSystem`, `Win32_Devices_FunctionDiscovery`,
  `Win32_System_Variant`; extend as the cross-compiler demands).
- **Change:** add the target-gated dep; `wasapi/mod.rs` (`#![allow(unsafe_op_in_unsafe_fn, …)]` like
  `vst3_host`); `devices.rs` with `fn enumerate(direction) -> Result<Vec<DeviceRef>, AudioError>` and a
  small `AudioError`; a PWSTR→`String` helper.
- **Verify:** **`make host-windows-check`** cross-compiles the enumeration for `x86_64-pc-windows-msvc`.
  If any pure helper is platform-neutral (e.g. a PWSTR-length/decoding helper extracted to take `&[u16]`),
  add a Linux `cargo test -p galad` for it. **Red:** greenfield — the module/symbols don't exist
  (cross-compile fails to resolve). **Green:** it cross-compiles; the live device list is a Windows-runtime
  check (Step 8).

## Step 7 — WASAPI duplex stream setup (`IAudioClient`, exclusive→shared)  [depends on #4, #6] [windows]
- **File(s):** `host/src/audio/wasapi/stream.rs` (new), `host/src/audio/wasapi/mod.rs` (register).
- **Reference behavior:** For a selected `DeviceRef`, resolve `IMMDeviceEnumerator::GetDevice(id)` →
  `IMMDevice::Activate(IID_IAudioClient)` → `IAudioClient`. `GetMixFormat` (shared) / build a
  `WAVEFORMATEX(TENSIBLE)` and `IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, …)`; pick mode+format+
  buffer via Step 4's `choose`. `GetDevicePeriod` for the period; `Initialize(share_mode,
  AUDCLNT_STREAMFLAGS_EVENTCALLBACK, hnsBufferDuration, …, &format)`; on
  `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` re-derive the aligned size and re-`Initialize` (the documented
  exclusive-mode retry). `GetBufferSize`; `CreateEventW` + `SetEventHandle`; `GetService` →
  `IAudioCaptureClient` (input) / `IAudioRenderClient` (output). Wrap capture and render clients each in a
  `WasapiStream` holding the client, service, event handle, and negotiated `StreamFormat`.
- **Change:** add `stream.rs` with `WasapiStream` + `fn open_capture(dev)` / `fn open_render(dev)`
  returning the configured clients (not yet started).
- **Verify:** **`make host-windows-check`** cross-compiles. **Red:** greenfield. **Green:** cross-compiles;
  real initialization is exercised at runtime in Step 8.

## Step 8 — Realtime duplex engine + passthrough run + latency report  [depends on #5, #7] [windows]
- **File(s):** `host/src/audio/wasapi/engine.rs` (new), `host/src/audio/mod.rs` (public `#[cfg(windows)]`
  engine API), `host/src/main.rs` (a `passthrough <in-id> <out-id>` subcommand, `#[cfg(windows)]`; a
  clear "Windows-only" message off-Windows).
- **Reference behavior:** Open capture + render streams (Step 7) sharing one `AudioRing`/`Transport`
  (Step 5); `CoInitializeEx` on a dedicated **realtime thread**; `IAudioClient::Start` both; loop on
  `WaitForMultipleObjects(capture_event, render_event)`: on capture-ready, `IAudioCaptureClient::
  GetBuffer` → `Transport::capture` → `ReleaseBuffer`; on render-ready, `GetCurrentPadding`,
  `IAudioRenderClient::GetBuffer` → `Transport::render` → `ReleaseBuffer`. Only WASAPI `GetBuffer`/
  `ReleaseBuffer` and the Step-5 transport run on the audio thread — **no allocation, no locks**
  (ADR-0001). Clean `Stop`/teardown on a stop flag (`AtomicBool`). Report **measured round-trip latency**
  from `GetStreamLatency` + buffer periods.
- **Change:** add `engine.rs` (`struct AudioEngine` with `fn start(in: DeviceRef, out: DeviceRef) ->
  Result<…>`, `fn stop`, `fn measured_latency`); expose a `#[cfg(windows)]` `audio::run_passthrough`; wire
  the `passthrough` subcommand.
- **Verify:** **`make host-windows-check`** cross-compiles the engine + subcommand. The **automated**
  proof is Step 5's transport test (the same `Transport` the engine drives). The **live** "mic → output
  glitch-free at the chosen buffer size, device enumerate/select works, measured round-trip latency
  recorded" is the Windows-runtime field check (run `galad passthrough …` on hardware; record the latency)
  — performed when Galad runs on Windows, **not** a manual `make ci` gate (no human-audition exit). **Red:**
  greenfield. **Green:** cross-compiles and `cargo test -p galad` stays green.

---

## Decisions table
| Step | Decision | Status / recommendation |
| ---- | -------- | ----------------------- |
| (phase) | WASAPI binding: raw `windows` vs `wasapi` wrapper vs `cpal`. | **Resolved (mine):** raw `windows` 0.62, target-gated — full exclusive/event-driven control, ADR-0022's native WASAPI, already lockfile-unified. |
| 3 | Mono⇄stereo handling for the single chain. | **Recommend:** mono→stereo duplicate (centered), stereo→mono average; equal = bit-exact. (Alt: −3 dB fold-down / L-only.) |
| 4 | Round-trip latency target + exclusive-vs-shared buffer policy. | **Recommend:** exclusive event-driven primary, shared fallback; buffer = min period (excl.) / default (shared); target ≤~10 ms, *measured* value recorded at exit. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. Two `[DECISION]` stops remain (Steps 3 and 4),
each with a recommendation. Steps 1–5 are verified automatically by `cargo test -p galad` on Linux
(including `assert_no_allocations` for the ADR-0001 path); Steps 6–8 are cross-compile-verified by
`make host-windows-check`, with the live passthrough + measured latency recorded when Galad runs on
Windows. M2 done gives a glitch-free native audio path; **M3** joins it to the M1 VST3 host (drive the
plugin chain from the RT callback). One milestone at a time.
