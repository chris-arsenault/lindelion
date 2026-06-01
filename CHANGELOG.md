# Changelog

All notable user-visible changes to Lindelion are recorded here.

## Unreleased

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

### Calóma

- Built Calóma, the Windows-only speech-clarity VST3: a serial chain of the 20 ported speech effects with a 3-valued signal-order parameter (Clarity / Broadcast / Light topologies), running on a mono downmix with compute-once shared analysis and a fixed-max latency reported to the host.
- Made it a self-contained single-component VST3 (one COM object is processor and controller) with **no host parameters** — a Vizia editor (order, per-effect enable + dry/wet intensity, input/output level) on the shared `lindelion-ui` stack is the sole control surface, writing lock-free shared state the audio thread reads. All three orders' chains are pre-built so an order switch is an atomic index flip.
- Refactored the SwiftF0-consuming speech effects to take an injected `SignalSnapshot`, so one shared analysis worker replaces the per-effect workers (retiring the test-only `sync-analysis` feature).
- Chose each order's committed default tuning with an offline full-chain tuning harness (`make tune-defaults`): tonal/dynamics parameters optimized against objective metrics (matched-pair SNR, dereverb, clarity, coloration) on a spoken-word battery, committed at unity gain (loudness is the user's to set; the limiter provides peak safety).
- Added a Windows VST3 build path (`make build-windows`, cargo-xwin) and per-order full-chain fidelity gates (`make test-models`): finite, non-clipping, noise not worsened, dereverb reduces the late tail, and reported latency matches the measured group delay.

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
