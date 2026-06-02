# Changelog

All notable user-visible changes to Lindelion are recorded here.

## v0.13.1 - 2026-06-02

### Tooling

- Added a Markdown relative-link check to `make ci` (xtask `link-check`): it walks every `.md` in the repo and fails the build if an inline `[text](path)` or `![alt](path)` link points at a missing file — catching the recurring doc defect (a plugin spec linking `adr/…` instead of `../adr/…`, or a retired plan leaving dangling references). External, `#anchor`-only, and in-code links are skipped; run it alone with `cargo run -p xtask -- link-check`.

## v0.13.0 - 2026-06-02

### Lamath

- Added an opt-in **shared-body idiophone mode** (`shared_body` patch toggle, default off). When on, an idiophone patch's resonator becomes a single persistent body that note-ons re-strike instead of allocating a per-note voice: a strike injects the velocity-selected excitation at force into the live body, each strike retunes it ring-preserving so it stays melodically playable, and the body's own decay is the envelope — the per-note amp envelope steps aside, the output filter and saturation apply as a static post-body coloration, and each strike arms the attack-noise burst. A configurable key-switch MIDI range damps the body to silence over a short ramp; note-off does nothing to the ring. The body runs its own energy follower and stages like a same-family voice, summed engine → shared body → sympathetic chamber → master. Off is bit-identical to the per-voice behavior, and waveguide (string/tube) slots stay polyphonic-per-voice. Exposed in the editor as a Body Mode toggle and damp-range controls, and covered by objective stability and fidelity sweeps. See [ADR-0031](docs/adr/0031-shared-body-idiophone-mode.md) and the [Lamath spec](docs/plugins/lamath.md).

## v0.12.0 - 2026-06-02

### Cenedril

- Cenedril is feature-complete (M0–M6): the Windows-only passthrough Visualizer VST3 ships its full editor — a selectable **magnitude or time-frequency reassigned** spectrogram, peak/RMS/crest + BS.1770-4 LUFS meters, an analysis-signal panel (voicing, speech presence, onset/spectral flux, HNR, pitch), and controls for the view, frequency scale (log/linear), color map (magma/viridis/grayscale), and dB display range — with all editor settings persisted in plugin state across reload. The audio path stays a bit-exact, zero-latency, allocation-free parallel tap; the editor reads the analysis from lock-free cells of a single-component VST3 ([ADR-0040](docs/adr/0040-cenedril-analysis-and-editor-delivery.md), [spec](docs/plugins/cenedril.md)). On-target Windows load-and-run remains to be verified.

### Shared DSP

- Added `lindelion-dsp-utils::reassign::ReassignStft`, a forward-only method-of-reassignment STFT analyzer: per bin it emits the magnitude plus time/frequency reassignment offsets, allocation-free on the audio thread. It is Cenedril's single spectral tap (the magnitude spectrogram reads it as a byproduct of the reassigned analysis). See [docs/dsp/reassignment.md](docs/dsp/reassignment.md).

### Build

- `make build-windows` now accepts `PLUGIN=<name>` to cross-compile and stage a single Windows-only plugin (e.g. `make build-windows PLUGIN=cenedril`), mirroring macOS `make build PLUGIN=`. Without it, all `$(WINDOWS_PLUGINS)` build as before.

## v0.11.0 - 2026-06-02

### Lúmedir

- Validated Lúmedir M6 on Linux — the plugin is **feature-complete (M0–M6)**. Settings persistence (target bands + syllables-per-word factor) already round-trips from M5; M6 adds the validation surface: a **full VST3-boundary load/run test** (factory → `IComponent`/`IAudioProcessor` → `process`, asserting bit-exact 0-latency passthrough through the actual COM entry path a host uses, on Linux), a **bounded-allocation soak** over the delivery estimators (the repo's counting allocator via `count_allocations`, warmed past the 2000-frame pitch-dynamism window so a real leak would show as batch-over-batch growth), and an **off-thread worker stability soak** (finite/in-range snapshots over a long run, no deadlock, clean thread join). The soaks run via `make test-integration`; the VST3-boundary test runs in `make ci`.
- The Windows `.vst3` is produced with a full MSVC link (`make build-windows`). The only validation that requires a Windows host — the editor window's *visual* rendering (Vizia is gated to the Windows target, ADR-0023) and loading into a third-party DAW — is a documented checklist in the plugin README; everything functional is proven on Linux.

### Documentation

- Moved Lúmedir's documentation to its durable home now that the feature is complete: removed the temporary root tracking docs (`LUMEDIR-VST-PLAN.md` + the `M0`–`M4` step files), added the spec [`docs/plugins/lumedir.md`](docs/plugins/lumedir.md), the delivery-metric DSP reference [`docs/dsp/delivery-metrics.md`](docs/dsp/delivery-metrics.md), and [ADR-0048](docs/adr/0048-lumedir-single-component.md) (single-component VST3); refreshed the architecture, docs index, AGENTS code map, and backlog.

## v0.10.0 - 2026-06-02

### Lúmedir

- Built Lúmedir M5 — session summary + target-band scoring. The editor now scores each live metric against a configurable target band (`BandStatus`: below / in / above), shows an in/out-of-band status strip, and — under manual Start/Stop session control — accumulates an end-of-session summary (per-metric mean + fraction of samples in band). Session accumulation and the metric→status scoring are platform-neutral and `make ci`-tested; the summary/scoring/edit view is compile-checked on the Windows build.
- Added configurable coaching config — the syllables-per-word factor plus per-metric target bands — with defaults grounded in public-speaking norms (speaking rate 120–160 WPM / 3.0–4.0 syl/s; pitch dynamism ≥ 3.0 semitones; pause fraction 0.10–0.30; clarity ≥ 0.6). The config persists in plugin state via the shared versioned-TOML patch format and is edited live through a lock-free `SharedConfig`; the delivery worker reads the factor each publish, so WPM tracks factor edits without a respawn (no audio-thread locks, ADR-0001). The editor exposes factor + band steppers.

## v0.9.0 - 2026-06-02

### Lúmedir

- Built Lúmedir M4 — the **live running-readout** Vizia editor. Six delivery gauges (speaking rate, words/min, pitch dynamism, pause fraction, pause count, clarity) render through the shared `lindelion-ui::vizia_meter::meter_row`, updating ~15 fps from the off-thread delivery worker's snapshots — read on the UI thread through a lock-free `DeliveryReader`, never the audio thread. The metric→bar-fill mappings and value formatters are platform-neutral and `make ci`-tested (display spans anchored on the `FIXTURES.md` delivery targets); the Vizia view itself is compile-checked on the Windows build and visually verified on a Windows host.
- Converted Lúmedir to a **single-component VST3** (one COM object implements `IComponent` + `IAudioProcessor` + `IEditController`), replacing the two-class processor/controller split. Lúmedir exposes no host parameters, so the separate controller existed only to forward `createView`; folding it onto the processor lets the editor read the worker's delivery snapshots directly — no `IConnectionPoint`/`IMessage` marshaling — matching Cenedril and Calóma.

## v0.8.3 - 2026-06-02

### Tests

- Cut the `make ci` unit suite from ~119s to ~15s by moving 38 heavy DSP tests that had slipped the integration gate into the `integration-tests` feature (run via `make test-integration`), where the convention already puts multi-second fidelity/stability/tuning sweeps. Lamath alone was 95s of the 119s — its M9–M11 waveguide stability/tuning/timbre renders (extreme-drive bounds, decay-across-range, tuning matrices, the 96s source↔body balance timbre render) were never gated; likewise dsp-utils' spectral-centroid/inharmonicity measurement sweeps, the pitch-shift Resample Pro fidelity battery, and linnod's pitch-fidelity/analysis renders. No tests were deleted — every expensive test is a real assertion, so all moved (none were low-value). The allocation-free (ADR-0001) guards stay in the fast `make ci` path deliberately.

### Shared infrastructure

- Added a shared meter/gauge row widget (`lindelion-ui::vizia_meter::meter_row`) for the Windows VST editors: a labelled horizontal bar with a proportional `Signal<f32>` fill and a formatted value, plus its `METER_STYLE`. Cenedril's level/LUFS meters and analysis readouts and Lúmedir's delivery gauges all render through it, so the two milestones (Cenedril M5, Lúmedir M4) don't each build a meter — landing it now, ahead of both, removes the one real duplication risk in parallelizing those streams.
- Reserved per-workstream ADR number ranges in the ADR index (Lamath resonator 0032–0039, Cenedril 0040–0047, Lúmedir 0048–0055) so concurrent branches draw from disjoint blocks and don't collide on a number at merge time.

### Shared infrastructure

- Deconflicted duplication left by the independently-forked plugin branches (a semantic pass over the recent merges). Lúmedir, forked earliest, had rebuilt two things Calóma and Cenedril already had.
- Extracted the lock-free audio→worker hand-off into `lindelion-dsp-utils::handoff` — an `AtomicF32` cell and an SPSC `SampleRing`. The speech-signals `AnalysisWorker` and Lúmedir's `DeliveryWorker` had byte-identical copies of the ring; Cenedril's meter snapshot re-rolled the same per-field atomic encoding. All three now share the primitives (Cenedril's frame-granular `FrameRing` stays distinct but builds on `AtomicF32`).
- Extracted the Windows `IPlugView`→`HWND` Vizia attach into `lindelion-ui::vizia_window::ViziaWindowEditor`. Calóma, Cenedril, and Lúmedir each carried their own copy of the `ParentWindow` + `open_parented` + close-on-drop boilerplate (Lúmedir's was a literal copy of Cenedril's, down to an unused `parent_view` parameter); their editors are now thin newtypes over the shared helper. The DSP analysis primitives (STFT, LUFS, pitch/voicing, flux, envelope) were already shared and needed no change.

## v0.8.0 - 2026-06-02

### Lúmedir

- Built Lúmedir (M0–M3), the Windows-only passthrough Speech-Coach VST3 (crate `plugins/lumedir`): audio passes through bit-exact at zero latency while a mono mix feeds an off-thread delivery worker that computes live delivery metrics. Two-class VST3 with the Windows `.vst3` bundle path (`WINDOWS_PLUGINS += lumedir`) and a placeholder Vizia editor (`lindelion-ui::lumedir_vizia`); the live readout view is a later milestone.
- Added the delivery-metric estimators, each plugin-local, pure, and fixture-validated: **speaking rate/cadence** (an envelope-peak syllable-nuclei detector over the intensity contour → syllables/min over a sliding window, with WPM derived via a configurable syllables-per-word factor); **pitch dynamism** (windowed voiced-f0 from SwiftF0 → semitone standard deviation, so flat reads far below animated); **pause structure** (energy-frame silence runs → pause fraction/count/length); and **clarity** (voicing ratio plus onset sharpness).
- Assembled the metrics into a complete delivery snapshot published by a plugin-local off-thread `DeliveryWorker` (transport modeled on the existing analysis worker, reusing the shared `SignalAnalyzer`/`lindelion-speech-signals`), exposed to the editor via `latest_delivery()`. The estimators' heavy/ONNX tests are gated to `make test-integration` / `make test-models`, out of the `make ci` unit path.

### Lamath

- Started the shared-body idiophone mode ([ADR-0031](docs/adr/0031-shared-body-idiophone-mode.md)): an allocation-free, persistent shared-body skeleton wired into the voice/resonator stack. It is silent groundwork — the re-strikable persistent-resonator behavior arrives in later milestones — so existing patches and the default patch are unchanged.

### Code organization

- Split four over-limit Lamath source files into focused submodules to satisfy the 600-line file-size lint: driver/contact patch configs out of `patch.rs`, the modulation-slot path policy out of `parameters/paths.rs`, and the driver/contact-range and expanded-parameter test groups out of their test files.

## v0.7.0 - 2026-06-02

### Cenedril

- Gave Cenedril its realtime analysis. An allocation-free audio-thread tap, parallel to the bit-exact passthrough, computes an STFT magnitude spectrum, peak/RMS/crest levels, BS.1770-4 **LUFS** (momentary/short-term/integrated — a new K-weighted meter in `lindelion-dsp-utils`, gated through a bounded histogram so it never allocates), and an inline speech-presence signal, and hands them to the editor over a lock-free SPSC frame ring plus an atomic meter snapshot. The allocating SwiftF0 signal analysis runs on an off-thread worker.
- Added a scrolling STFT **spectrogram** to the Cenedril editor: a log-frequency, dB-scaled, magma-colored view rendered with Skia from the analysis ring (a ~66 ms drain-and-repaint), with the platform-neutral model `make ci`-validated (steady sine → a horizontal line, a sweep → a diagonal, silence → the floor).
- Made Cenedril a **single-component** VST3 — one COM object is processor and controller — so the editor reads the audio thread's lock-free ring directly with no message marshaling; `make build-windows` stages a single-component `Cenedril.vst3`.

## v0.6.0 - 2026-06-01

### Lamath

- Added a coupling/contact stage and an energy-dependent source↔body balance to the waveguide instruments, so picked-vs-strummed and soft-vs-loud read as distinct timbres rather than levels. A new **contact** control spreads the strike across the String/Tube — a tight pick versus a wide strum, widened further by playing effort — and a **contact time** mellows the onset; the spread averages out the strike-position comb (a flatter, different harmonic balance) while the contact time low-passes the attack. On the String, a defeatable **source↔body balance** leans the output to the warm body at low dynamics and the direct pickup at high dynamics through an equal-power crossfade that holds output level, so soft and loud differ in character, not just gain. Both controls default to the pre-M9 behavior, so existing patches and the default patch are unchanged; Modal and Mesh are unaffected.

- Added effort/energy-scaled surrounding effects, the "surrounding" link of the dynamic-response chain. A per-voice **mechanical noise** burst (a bright pick click plus a band-limited breath rush) fires at note-on and scales with playing effort; an energy-scaled **radiation brightening** high-shelf makes a more energetically-sounding note radiate brighter; and a **cross-voice sympathetic chamber** — a pool of lightly-damped strings tuned to the notes you actually play, excited by the mix with an energy-scaled send — lets every note ring the others' (and its own) sympathetic strings, ringing on after the note. The chamber is a send/return at the runtime, so the voice engine is unchanged. Each effect is a `0..1` depth defaulting to 0 (defeated), so existing patches and the default patch are unchanged.

- Voiced the resonators into real instruments. The String and Mesh now ring to multi-second tails instead of dying in well under a second; the String carries audible inharmonicity (string stiffness) and a two-voicing body, and the four resonator families (Modal, String, Tube, Mesh) are now timbrally distinct rather than variations on one tone.

- Calibrated the energy-driven dynamic response to real playing levels. The tension bloom, bore steepening, mesh shimmer, source↔body balance, radiation brightening, and sympathetic send were all referenced to energy levels far above what playing actually produces, so they barely engaged; they now act across the real dynamic range — soft, medium, and hard playing read as genuinely different. The source↔body balance also reads in the natural direction (harder playing blooms brighter), and the String midrange no longer over-damps on body resonances.

- Staged the whole instrument for level and headroom. The resonator families, which emerged up to ~38 dB apart and ~30 dB too quiet, are now loudness-balanced and brought to a usable output level; a transparent master safety clipper holds dense chords below −1 dBFS without touching the level or tone of a single note. The bow driver's self-oscillation, which could run far past full scale, is tamed to a sane forte.

- Added the **bow** friction driver: a continuous stick-slip excitation that sustains a bowed (Helmholtz) String tone for as long as a note is held, selectable as a fourth `Driver Type` with pressure, bow-speed, and friction controls. The default String now enables the source↔body balance (0.5), so the factory patch is dynamically alive out of the box; Modal, Tube, and Mesh defaults are unchanged.

### Galad

- Rebuilt the host UI around the signal chain. The window is now a single channel: a compact top bar (brand, run state, In/Out device pickers, status, session load/save), a **full-width signal chain** as the primary surface (insert rows with bypass, editor, reorder, and remove, ending in an "Add plugin" slot), and a master/transport bar (start/stop, input and output meters, master fader and mute). Devices select from the top bar; the chain owns the window.
- Moved the plugin browser into an **on-demand modal overlay** opened from the chain's "Add plugin" slot, instead of a permanent panel. The overlay holds the vendor-grouped catalog and the scan-folder management, dims the view behind it, and dismisses on add, on the close button, or on a click outside — so browsing and folder setup no longer consume the main view.
- Restyled to the shared Vizia design language: real Tabler icons, a cyan/green/amber/pink accent palette on graphite with section-header accent bars, correct rounded corners (`corner-radius`), and consistent label clipping. Pulses the top-level window size during startup so the native surface reliably delivers its first paint.

## v0.5.0 - 2026-06-01

### Calóma

- Built Calóma, the Windows-only speech-clarity VST3: a serial chain of the 20 ported speech effects with a 3-valued signal-order parameter (Clarity / Broadcast / Light topologies), running on a mono downmix with compute-once shared analysis and a fixed-max latency reported to the host.
- Made it a self-contained single-component VST3 (one COM object is processor and controller) with **no host parameters** — a Vizia editor (order, per-effect enable + dry/wet intensity, input/output level) on the shared `lindelion-ui` stack is the sole control surface, writing lock-free shared state the audio thread reads. All three orders' chains are pre-built so an order switch is an atomic index flip.
- Refactored the SwiftF0-consuming speech effects to take an injected `SignalSnapshot`, so one shared analysis worker replaces the per-effect workers (retiring the test-only `sync-analysis` feature).
- Chose each order's committed default tuning with an offline full-chain tuning harness (`make tune-defaults`): tonal/dynamics parameters optimized against objective metrics (matched-pair SNR, dereverb, clarity, coloration) on a spoken-word battery, committed at unity gain (loudness is the user's to set; the limiter provides peak safety).
- Added a Windows VST3 build path (`make build-windows`, cargo-xwin) and per-order full-chain fidelity gates (`make test-models`): finite, non-clipping, noise not worsened, dereverb reduces the late tail, and reported latency matches the measured group delay.

## v0.4.0 - 2026-06-01

### Lamath

- Implemented the Lamath v2 sidechain workflow: optional audio input bus, audio-created notes from sidechain onsets, continuous and note-latched live excitation modes, and per-patch audio/MIDI interaction policy.
- Added typed parameter bindings for audio input mode, audio expression mapping, note detection thresholds, live excitation mode, latch window, and latch fade.
- Added preallocated sidechain scratch, pre-roll, and per-voice latch buffers so the v2 audio path remains allocation-free on the audio thread.
- Corrected Tube waveguide tuning to the bore's quarter-wave termination relationship and compensated both boundary filters' phase delay, so `frequency_hz` is the played pitch (within 3 cents through the low–mid range; accuracy tapers in the top octave, where the quarter-wave loop is only a few samples long). Existing Tube patches now sound an octave higher, since the Tube previously resonated roughly an octave flat above ~165 Hz.
- Added a Mesh resonator model: a rectangular 2D digital-waveguide mesh (plate/membrane), selectable per resonator slot alongside Modal and Waveguide, with material, size, damping, tension, strike-position, and pickup-spread controls. The model is opt-in, so existing patches and the default patch are unaffected.
- Calibrated waveguide loop damping to an explicit frequency-dependent T60(f): the played pitch now decays in the requested time and higher partials decay measurably faster, where they previously shared a single decay time that the loop filter only approximated.
- Cut waveguide (String and Tube) per-voice cost by ~10x by deriving each resonator's linear operators — loop damping, dispersion, bore profile, geometry, delay tuning, and body coloration — at control rate and caching them until an input moves, instead of recomputing every sample; the audible render is unchanged. Continuous controls (loop gain, filter cutoff/resonance, dispersion, bore reflection) now glide on parameter changes instead of zippering, while pitch and strike/pickup positions still track exactly. Mesh is unaffected.
- Ran the String, Tube, and Mesh resonator inner loop at 2x oversampling — the shared substrate for upcoming dynamic-response nonlinearity — keeping the sound unchanged within filter tolerance today. The plugin now reports a small fixed latency (8 samples at the host rate) for host delay compensation. Modal voices are untouched and add no latency.
- Gave the String waveguide energy-dependent tension modulation: a hard pluck now blooms — its pitch sharpens transiently on the attack (up to ~+40 cents at peak energy) and settles back to nominal as the note decays, driven by the measured-energy bus. Soft and medium dynamics stay in tune (the drive follows energy squared), and a string at rest is unchanged. Tube and Mesh are unaffected.
- Gave the Tube waveguide energy-dependent bore steepening: a loud bore now turns brassy — an amplitude-dependent dispersion stage steepens the wavefronts inside the bore and the bell radiates the resulting harmonics, so the spectrum brightens with playing energy and mellows when soft, while the fundamental tuning is preserved. Soft bores and bores at rest are unchanged (the effect follows energy squared). String, Mesh, and Modal are unaffected.
- Gave the 2D Mesh resonator energy-dependent geometric (von Kármán) coupling: a hard strike now blooms — an energy-conserving junction rotation steers energy upward into higher modes as playing energy rises, the gong/cymbal shimmer that builds with how hard you hit and recedes as the strike decays. Soft strikes stay linear and a mesh at rest is unchanged (the effect follows energy squared); the coupling only redistributes energy, never injects it. String, Tube, and Modal are unaffected.
- Replaced the String waveguide's heuristic output EQ with a physically reduced body coupled two-way at the bridge: a small bank of signature modes, an air/Helmholtz cavity mode, and a broad formant form the bridge admittance, so the body loads the string loop — partials near a body resonance lose energy and decay faster, and the body absorbs energy across its active range so a body-coupled note sustains less than a bare string would (what a real instrument does, not just a tone control). The coupling is a passive wave-digital termination, so the resonant body never destabilizes the string. The String output is the body's radiated motion summed with a string pickup tap, so the body colours the sound while `pickup_position` and the playing level stay intact (the String sits at the same loudness as before). Two body voicings (guitar, violin) give distinct character. Tube and Modal are unaffected.

- Added a force-dependent physical driver layer to the String/Tube waveguides: a selectable driver sits between the excitation and the resonator and runs inside the 2x oversampled loop, driven by playing effort. **Pick** is a force-shaped contact — a harder strike passes more high frequencies (brighter), a timbral change rather than just level. **Reed** is a self-oscillating wind valve — mouth pressure from effort, a nonlinear reed table coupled to the bore's returning wave, so the bore blows above a pressure threshold and stays quiet below it (a playing-force regime change). The existing sample/sidechain excitation is the default driver, so patches without a driver are unchanged; Modal and Mesh are unaffected.

### Glirdir

- Completed Glirdir's VST3 buildout: sing-to-MIDI scratchpad with capture-first analysis worker, editor surface, drag/export with fallback paths, sample-library scratchpad save, patch and DAW state persistence, and macOS bundle support.

### Galad

- Added Galad, a standalone Windows realtime VST3 *host* application (`galad/`, package `galad`): live microphone → an ordered chain of arbitrary VST3 plugins → output device, with native WASAPI device management. It hosts Lindelion and third-party VST3s through the same path, runs each plugin's own native editor window, and routes its output to any device (including a user-installed virtual cable). Windows-only and excluded from `make ci`; the platform-neutral host logic is tested on Linux and the Windows build is cross-compiled via cargo-xwin.
- Galad preserves each plugin's state across live chain edits (reorder, bypass, add, remove) by keeping plugin instances alive and rebuilding only the processing order over them, published gaplessly; sessions (selected devices + ordered chain + per-plugin opaque state + scanned folders) save and restore as versioned TOML.
- Galad runs the chain at the audio device's own sample rate and declares it to each plugin; it pre-selects the system default input/output on launch, validates a plugin before adding it, and guards the chain output against NaN/Inf so a misbehaving plugin cannot reach the device.

### Shared infrastructure

- Extracted shared Glirdir/Lamath surfaces into reusable crates: `lindelion-capture`, `lindelion-audio-expression`, `lindelion-phrase-analysis`, and `lindelion-midi`.
- Added host-agnostic Criterion benchmarks for `lindelion-dsp-utils`, Lamath modal/waveguide/engine paths, and Glirdir's offline analysis job.
- Added `make bench` and `make bench-smoke` targets and per-crate perf records under `docs/perf/`.
- Upgraded the shared `DelayLine` fractional read to 4-point Lagrange (cubic) interpolation, flattening fractional-delay group delay so waveguide loops stay in tune at high frequencies; String waveguide tuning is now within ~1.5 cents across 30 Hz–4 kHz.

### Code organization

- Split oversized Rust modules so every file fits the 600-line size cap enforced by `xtask lint-sizes`.

### Bug fixes

- Fixed the resonator parameter architecture so registry bindings drive both host automation and the editor surface.
- Fixed resonator architecture debt by aligning module boundaries with the workspace's shared-core principles.

### Documentation

- Reorganized plugin docs and backlog tracking under `docs/plugins/` and added per-crate perf records under `docs/perf/`. Added the repository documentation convention (`AGENTS.md`, ADRs, CHANGELOG, workspace backlog) following `../ahara/REPO-DOCS.md`.
- Established the speech-effect port of `hot-mic`: recorded the shared-workspace and host-agnostic-core decisions (ADR-0012, ADR-0013), scaffolded the `speech/` tree and the `lindelion-effect` / `lindelion-fidelity` foundation crates, and added the port milestones to the workspace backlog.

## v0.3.0 - 2026-05-23

### Plugin shell

- Implemented the resonator VST3 plugin shell for Lamath, including processor, controller, factory, editor, state, and message adapters.
- Established the parameter registry as the single source of truth for parameter metadata, normalized/plain conversion, formatting, patch get/set, apply policy, runtime target, smoothing metadata, and editor binding.
