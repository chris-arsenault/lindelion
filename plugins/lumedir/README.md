# Lúmedir — Windows-only Speech-Coach VST3

**Lúmedir** (Quenya `lúme` "time/rhythm" + `-dir` "keeper") is a Windows-only passthrough
Speech-Coach VST3: audio passes through unchanged at zero latency, and the value is feedback on
*delivery* — speaking **rate/cadence**, **pitch dynamism**, **pause structure**, and **clarity** —
as a live running readout and an end-of-session summary, scored against configurable target bands. It
targets **Windows only** — a Windows VST3 build + a Windows `IPlugView`→`HWND` Vizia/baseview editor
attach — per [ADR-0023](../../docs/adr/0023-new-vsts-windows-only.md), reusing Cenedril's M0/M1
platform foundation; it runs in the Galad host and Windows DAWs. (The crate/dir is `lumedir`,
matching the product name; an earlier codename was `coach`.)

**Current state (M0–M5):** a bit-exact, zero-latency passthrough processor that **feeds the
off-thread delivery worker** (`DeliveryWorker` over `lindelion-speech-signals`), the Windows `.vst3`
bundle path, the M1–M3 delivery-metric estimators (speaking rate, pitch dynamism, pause structure,
clarity) assembled into a `DeliverySnapshot`, the M4 **live running-readout** Vizia editor (delivery
gauges through the shared `lindelion-ui::vizia_meter::meter_row`), and M5 **session summary +
target-band scoring**. It is a **single-component VST3** (one COM object is processor + controller),
so the editor reads the worker's snapshots directly through a lock-free `DeliveryReader` — mirroring
Cenedril and Calóma, which also expose no host parameters.

M5 adds a configurable coaching config — the syllables-per-word factor plus per-metric target bands
(`config.rs`), with defaults grounded in public-speaking norms (rate 120–160 WPM / 3.0–4.0 syl/s,
pitch dynamism ≥ 3.0 st, pause fraction 0.10–0.30, clarity ≥ 0.6). The config persists in plugin
state via the shared versioned-TOML patch format (`config_io.rs`) and is edited live through a
lock-free `SharedConfig`; the worker reads the factor each publish so WPM tracks factor edits.
Each metric is scored against its band (`BandStatus`), and a manual-start/stop `SessionAccumulator`
aggregates a session summary (per-metric mean + in-band fraction). The editor adds an in/out-of-band
status strip, Start/Stop session controls + summary, and band/factor steppers.

M6 hardens and validates: the plugin's behavior is exercised on Linux through the **actual VST3 COM
boundary** (factory → `IComponent`/`IAudioProcessor` → `process` bit-exact passthrough), a
**bounded-allocation soak** over the delivery estimators (the repo's counting-allocator
`count_allocations`, warmed past the dynamism window — no unbounded growth) and an **off-thread
worker stability soak** (finite/in-range over a long run, clean shutdown), both run via
`make test-integration`; and the Windows `.vst3` is produced with a full MSVC link. Lúmedir is
**feature-complete (M0–M6)**.

The cross-platform DSP/processor, the delivery source/config-surface wiring, the metric→gauge
mappings, the band scoring, config persistence, session accumulation, the VST3-boundary load/run,
and the soak/stability checks are all validated on Linux (`make ci` + `make test-integration`). The
one irreducibly-Windows item is the editor's *visual* rendering (the Vizia view is gated to the
Windows target per ADR-0023); it is compile-checked on the Windows build and confirmed by a Windows
host screenshot — see the checklist below.

## Build (Windows VST3)

Lúmedir is cross-built from Linux as an MSVC-ABI `.vst3` with **cargo-xwin** (ADR-0023):

```sh
cargo install cargo-xwin                 # one-time; downloads the MS CRT/SDK on first build
rustup target add x86_64-pc-windows-msvc # one-time
make build-windows                       # stages Lumedir.vst3 in the bundle staging dir
```

Then copy the staged `Lumedir.vst3` to a Windows host, or load it in the Galad host, to verify it
passes audio through bit-exact at zero latency.

## Windows host validation (user-performed)

Everything *functional* is already proven on Linux (`make ci` + `make test-integration`). The only
checks that require a Windows host are the ones a Linux box physically cannot do — rendering the
Vizia editor window and loading into a third-party DAW. Load the staged `Lumedir.vst3` in the Galad
host and in a Windows DAW, and confirm:

- [ ] Audio passes through **bit-exact at 0 latency** (already proven on Linux via the COM-boundary
  test; re-confirm in a real host).
- [ ] The editor **window renders**: the delivery gauges, the in/out-of-band status strip, the
  Start/Stop session controls + end-of-session summary, and the factor/band steppers all display.
- [ ] Live gauges + status strip **update** as audio plays; Start/Stop drives the session summary.
- [ ] The band/factor steppers edit, and the scoring/WPM **re-reflect** the change.
- [ ] Settings **persist across reload** (save the project / close+reopen; bands + factor return).
- [ ] A **leak soak**: run for an extended period and watch process memory stay flat (the Linux
  bounded-allocation soak already covers the delivery estimators; this catches anything host-side).

- Spec: [docs/plugins/lumedir.md](../../docs/plugins/lumedir.md)
- Decisions: [ADR-0023 — New VSTs target Windows](../../docs/adr/0023-new-vsts-windows-only.md),
  [ADR-0048 — Lúmedir is a single-component VST3](../../docs/adr/0048-lumedir-single-component.md)
- DSP: [docs/dsp/delivery-metrics.md](../../docs/dsp/delivery-metrics.md)
