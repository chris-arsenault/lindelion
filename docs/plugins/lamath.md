# Lamath — Breath-Excited Resonator v0.3 Design Spec

**Name:** Lamath
**Name etymology:** Sindarin, "echo" or "ringing of voices." Six letters, pronounced LAH-math; paired phonetically with Glirdir.
**Target:** macOS (Apple Silicon primary, Intel best-effort), VST3
**Status:** Implemented VST3 instrument. This document preserves current behavior plus the v2 sidechain note/excitation plan.

---

## 1. Concept & Goals

A polyphonic physical-modeling synth where the **excitation source** is a user-loaded sample (rather than a synthesized impulse/noise burst), feeding configurable **resonator models** (modal bank, 1D waveguide). The thesis: existing physical-model synths sound like physical-model synths because their excitations are synthetic; feeding real breath transients, key clicks, and articulation noise into resonators produces hybrid timbres that no sampler library covers and no synthetic-excitation model can fake.

### Design principles

- **Sound generation only.** No FX, no reverb, no time-based or spectral processing beyond what's structurally part of the physical model. Effects belong in the Ableton chain downstream.
- **Sample as DSP component, not as playback.** The excitation sample is treated as an input signal into a resonator — not as a pitched/timestretched sound source. Tonality comes from the resonator.
- **Shared v2 audio path.** Sidechain audio can create notes, produce expression streams, and act as live excitation on the existing instrument through an optional input bus. Host-neutral detection and expression mapping live in shared crates; Lamath owns voice allocation, bus policy, and excitation routing.
- **Tight scope.** Fixed modulation routings, two resonator models, one output stage. No mod matrix, no per-effect chains, no built-in factory library.

### Non-goals

- Not a sampler. Excitations are short transients (typically <500ms), not melodic/looped content.
- Not a granular synth.
- Not an FX plugin. v2 adds an optional sidechain input bus to the existing instrument; it does not create a separate effect variant or host/emulate reverb, delay, distortion-as-effect, etc.
- Not cross-DAW polished — Ableton on Mac is the only test target for v1.

---

## 2. Signal Path

```mermaid
flowchart LR
    MIDI[MIDI In] --> VM[Voice Manager]
    SIDE[Optional Sidechain In] --> ANALYSIS[Audio Note & Expression Analysis]
    ANALYSIS -->|audio note on/off| VM
    EXP[Expression Stream] --> VM
    ANALYSIS --> EXP

    VM --> V1[Voice 1]
    VM --> V2[Voice 2]
    VM --> VN[Voice N]

    subgraph Voice
        EE[Excitation Engine<br/>multi-slot mixer]
        RA[Resonator A]
        RB[Resonator B]
        FILT[SVF Filter]
        SAT[Soft Saturator]

        SIDE -.->|v2 continuous or latch| EE
        EE -->|excitation buffer| RA
        EE -.->|parallel mode only| RB
        RA -->|series mode: A.out → B.exc| RB
        RA -.->|parallel mode: direct| MIX2
        RB --> MIX2[Resonator Mix]
        MIX2 --> FILT
        FILT --> SAT
    end

    V1 --> MIX[Voice Mixer]
    V2 --> MIX
    VN --> MIX
    MIX --> OUT[Stereo Out]
```

Each voice is independent state. The voice manager handles allocation, stealing, and routing MIDI/Expression to active voices. The `Expression Stream` is a per-voice struct (see §8) that v1 fills from MIDI and v2 can fill from shared audio analysis. In v2, sidechain audio can also create and release notes; audio-created voices use the same voice manager and expression contracts as MIDI-created voices.

---

## 3. Excitation Engine

### 3.1 Slot architecture

A patch declares up to **4 excitation slots**. Each slot has:

- **Sample reference** — by blake3 hash + last-known library-relative path
- **Gain** — pre-mix gain into the excitation buffer (dB)
- **Velocity zone** — lo/hi velocity bounds (0–127); slot only fires inside the zone
- **Sample-start offset** — fixed offset, plus optional velocity modulation depth
- **One-shot or loop** — one-shot is default; loop only for steady-state excitations (rare)
- **Pitch-track switch** — default off (excitation stays at original pitch); when on, sample is pitch-shifted to follow MIDI note via linear-interpolation resampling
- **Round-robin group** — slots tagged into the same RR group cycle on consecutive note-ons

### 3.2 Mixing rules

On each note-on, the engine:

1. Filters slots by velocity zone.
2. Within each RR group present, advances the RR cursor and selects one slot.
3. Sums all selected slots' sample playback into a per-voice excitation buffer.
4. Streams the excitation buffer into the resonator(s) per the routing config (§4.3).

This gives you, in one patch: a single excitation; or a velocity-split (soft taps vs hard strikes); or layered exciters (breath + key click); or RR variation across note repetitions; or any combination.

### 3.3 Playback mechanics

- Samples loaded into RAM at patch load (typical excitations are ≤1s, ≤96kB at 48kHz/16-bit mono).
- Per-voice playback cursor (f32 sample index) advances at `dest_sr / source_sr * pitch_ratio` per output sample.
- Linear interpolation between adjacent samples. Sufficient for excitation use; not trying to be Acoustica.
- Cursor terminates at sample end (one-shot) or wraps (loop).

### 3.4 v2 live audio excitation

v2 keeps Lamath as the existing instrument and adds an optional sidechain input bus. The sidechain stream can be used as excitation in four patch-configurable modes:

- `Off` — v1 sample-slot behavior only.
- `Continuous` — sanitized sidechain audio is mixed into active voices every block, with level control and hard realtime bounds.
- `NoteLatched` — each audio-created or MIDI-created note copies a bounded sidechain window into a per-voice latch buffer and plays it through the existing excitation path.
- `ContinuousAndNoteLatched` — a latched onset transient plus continuous sidechain drive while the voice remains active.

The continuous path is for breath/voice-driven energy. The latched path is for preserving onset identity: consonants, key clicks, chiff, plucks, or other transients around the note start. Both modes are product behavior local to Lamath, but input projection uses shared `ProcessContext::input` and audio analysis uses shared detector/expression crates.

Realtime constraints:

- All sidechain scratch buffers, pre-roll rings, detector state, and per-voice latch buffers are allocated during setup or structural patch application.
- Changing latch window size, pre-roll, or max latency is structural and must use an explicit apply policy.
- Empty or inactive input must behave exactly like v1.

---

## 4. Resonators

Two resonator slots per voice: **Resonator A** and **Resonator B**. Each slot is independently configurable as either a modal bank or a 1D waveguide. Both slots can be the same model type (e.g., A and B both modal banks, tuned to different fundamentals).

### 4.1 Modal bank

Bank of N second-order resonant filters in parallel, each modeling a single vibrational mode of a struck/blown idiophone.

**Parameters (per-resonator):**

- `mode_count` — user-configurable, **default 64**, soft cap 128, hard cap 256. Engine exposes an "offline render" override that allows unlimited (see §13).
- `model_preset` — frequency-ratio + decay-envelope template. Built-in templates: kalimba, marimba, bell, glass-bowl, metal-bar, woodblock, generic-strike. Templates ship hardcoded in v1 (no user editing).
- `fundamental_tune` — MIDI-tracked, plus fine offset (semitones + cents)
- `inharmonicity` — stretches/compresses the ratio template away from the preset's defaults
- `brightness` — per-mode gain envelope (tilts high modes up or down)
- `decay_global` — multiplier on all per-mode decays
- `decay_tilt` — biases high modes to decay faster (natural) or slower (unnatural, useful)
- `position_of_strike` — 0–1; modulates per-mode gains via `gain[n] *= |sin(n * π * position)|` so the excitation excites different modes more or less strongly depending on where on the resonator it lands

**DSP:** N parallel second-order resonator biquads. SIMD-batch in groups of 4 per AVX2/NEON lane (see §13).

### 4.2 1D waveguide

Karplus-Strong-extended single-delay-line waveguide for plucked-string and tube-like timbres.

**Parameters (per-resonator):**

- `fundamental_tune` — MIDI-tracked, semitone + cent offset
- `waveguide_style` — `String` or `Tube`; string is the default single-delay-line plucked/struck behavior, tube adds boundary reflection behavior for bore-like resonances
- `loop_filter_cutoff` — controls high-frequency damping in the feedback loop (brightness)
- `loop_filter_resonance` — controls Q of the loop filter
- `loop_gain` — feedback gain (sustain length)
- `loop_nonlinearity` — soft-clip strength inside the loop (adds bow-like character)
- `position_of_strike` — 0–1; selects the tap point along the delay line where excitation is injected
- `boundary_reflection` — -1–1; tube style reflection coefficient, where negative values invert the boundary reflection and positive values preserve polarity

**DSP:** delay line of length `sr / freq` samples (with first-order all-pass for sub-sample fractional tuning), one-pole or biquad lowpass in the loop, soft-clip stage. `String` style uses ordinary same-polarity feedback; `Tube` style applies the boundary reflection coefficient inside the loop so polarity and reflection amount become part of the resonator response.

**Deferred to v2:** bidirectional/two-port waveguide for proper tube modeling (closed-end reflection, clarinet-like behavior). v1 single-line is sufficient for the plucked/struck character.

### 4.3 Resonator routing

Two routing modes, switchable per-patch:

- **Parallel:** excitation buffer feeds both A and B independently. Audio outputs sum (with per-resonator mix slider).
- **Series (cascade):** excitation buffer feeds A only. A's audio output becomes B's excitation input.

**Series mode stability:** feeding A's continuous tonal output into B's excitation input risks runaway resonance (B excited by every cycle of A's ring-down). Mitigation: B's excitation input passes through a **high-pass at ~80Hz + transient-bias gate** that emphasizes onset content and de-emphasizes steady-state. This is part of the series-mode path, not user-exposed.

---

## 5. Output Stage

Per-voice, after resonator mix:

### 5.1 Filter — State Variable Filter (SVF)

- 12 dB/oct, modes: LP (default), BP, HP, switchable per-patch
- Parameters: cutoff (Hz, MIDI-key-trackable), resonance (0–1, self-oscillating at max)
- SVF chosen over ladder for: lower CPU, cleaner sound, less character of its own (resonators already supply character)
- Cutoff is a primary modulation destination (see §7.3)

### 5.2 Saturation — soft analog-modeled

- Sits **post-filter** as a master color stage
- `tanh`-based wave-shaper with mild asymmetry (slight even-harmonic content)
- Single `drive` parameter (0–1); output gain compensation built-in
- Post-filter rather than pre-filter because the resonators already provide harmonic complexity; saturation is for output coloration, not sound design

### 5.3 Master

- Per-voice gain (governed by amp envelope)
- Voice-mix sum to stereo
- Master gain control, master pan
- No per-voice stereo placement in v1 (deferred to v2)

---

## 6. Voice Management

- **Polyphony:** 8 voices baseline. User-configurable up to 16 (CPU-permitting).
- **Voice stealing:** oldest-released → quietest-released → oldest-active (suppress only if all slots are sustaining).
- **Per-voice state:** excitation playback cursors, resonator state (delay lines, biquad states), envelope/LFO states, filter state. All allocated up-front at instantiation; zero allocations in the audio thread.
- **Note-on cost:** sample-cursor reset, envelope reset, optional resonator state reset (configurable per-patch — "retrigger resonator" toggle; off by default for ringing carryover between notes).

---

## 7. Modulation

### 7.1 Sources

- **Amp Envelope** (ADSR) — always routed to output gain
- **Secondary Envelope** (ADSR) — user-assignable destination
- **LFO** — sine/triangle/saw/square/random S&H; rate in Hz or tempo-synced
- **MIDI Velocity** — note attack value
- **MIDI Aftertouch** (channel) — user-assignable destination
- **MIDI Mod Wheel** (CC1) — user-assignable destination
- **MIDI Pitch Bend** — always routed to resonator pitch (range configurable, default ±2 semitones)

### 7.2 Destinations (assignable)

- Filter cutoff
- Resonator A damping (modal decay_global / waveguide loop_gain)
- Resonator B damping
- Resonator A position-of-strike
- Resonator B position-of-strike
- Excitation gain
- LFO rate

### 7.3 Fixed routings (always-on)

- Amp envelope → output gain
- Pitch bend → resonator pitch (both A and B)
- Velocity → excitation gain (linear; depth controllable per-patch)
- MIDI note → resonator fundamental (equal temperament)

### 7.4 User-assignable routings

Four slots, each: `{source} → {destination} × amount`. Source picked from §7.1, destination from §7.2. No curves in v1 (linear only).

Justification for fixed-routing-with-4-flex-slots over full matrix: keeps UI small and the "playable instrument" feel intact, while leaving room for one experiment per patch (e.g., aftertouch → position-of-strike for breath-controlled timbre shift).

---

## 8. MIDI & Expression

### 8.1 ExpressionStream abstraction

```rust
struct ExpressionStream {
    pitch_bend: f32,    // semitones, signed
    pressure: f32,      // 0..1, channel pressure equivalent
    brightness: f32,    // 0..1, timbre/CC74 equivalent
    velocity: f32,      // 0..1, note-on velocity (latched)
    gate: bool,
}
```

One stream per voice. The Voice Manager maintains stream state and passes it into the voice's processing each block.

### 8.2 v1 stream source — MIDI

- `pitch_bend` ← channel pitch bend × range
- `pressure` ← channel aftertouch (CC129 equivalent)
- `brightness` ← CC74 (default; user-mappable to other CCs)
- `velocity` ← note-on velocity
- `gate` ← note-on / note-off

### 8.3 v2 stream source — sidechain audio

The expression contract is shared in `lindelion-plugin-shell`:

```rust
trait ExpressionSource {
    fn voice_started(&mut self, voice_id: u32, channel: u8, note: u8, velocity: f32);
    fn voice_released(&mut self, voice_id: u32);
    fn next_block(&mut self, voice_id: u32) -> ExpressionStream;
}
```

`lindelion-audio-expression` provides the shared audio-analysis-to-expression layer. Lamath v2 composes that layer with Lamath-local voice allocation and excitation policy. The optional sidechain input can run in three source modes:

- `Off` — MIDI creates voices and fills `ExpressionStream` exactly as v1.
- `AudioCreatesNotes` — sidechain onsets create/release voices. MIDI note input does not allocate voices, though automation and transport still apply.
- `MidiPlusAudioCreatesNotes` — MIDI and sidechain audio can both create voices. Voice ownership is tracked so audio note-offs cannot release MIDI voices and MIDI note-offs cannot release audio voices.

Sidechain note creation uses shared pitch, onset, loudness, and brightness primitives:

- Streaming onset plus stable pitch chooses the MIDI note at voice start.
- RMS maps to note velocity and ongoing `pressure`.
- Pitch drift after voice start maps to `pitch_bend` relative to the chosen MIDI note; it should not churn repeated note-on events.
- Spectral centroid maps to `brightness`.
- Release hysteresis and minimum note length prevent breath/noise tails from chattering gate state.

Lamath owns the product policy above the shared detectors: source mode, voice ownership, note retrigger behavior, latch/continuous excitation routing, and UI/status payloads.

---

## 9. Sample Library

A persistent, plugin-managed sample library. Lives on disk independently of any Ableton set. Patches reference samples by content hash with last-known-path fallback.

### 9.1 Disk layout

```
~/Library/Application Support/Ahara/Lamath/
├── Samples/              # User's organized hierarchy; arbitrary subfolders
│   ├── breath/
│   ├── key-clicks/
│   ├── mallets/
│   └── ...
├── Patches/              # Patch files (.toml)
├── index.db              # SQLite index
└── config.toml           # User settings (library path overrides, etc.)
```

Library path configurable. Default as shown; user can relocate to e.g. a fast NVMe or a synced Dropbox folder.

### 9.2 SQLite schema (v1)

```sql
CREATE TABLE samples (
  id INTEGER PRIMARY KEY,
  blake3_hash TEXT UNIQUE NOT NULL,
  relative_path TEXT NOT NULL,        -- relative to Samples/ root
  filename TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  sample_rate INTEGER NOT NULL,
  channels INTEGER NOT NULL,
  rms_db REAL,
  peak_db REAL,
  waveform_preview BLOB,              -- precomputed RMS-per-pixel preview, ~1KB
  imported_at TEXT NOT NULL,
  user_notes TEXT
);

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE NOT NULL
);

CREATE TABLE sample_tags (
  sample_id INTEGER NOT NULL REFERENCES samples(id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (sample_id, tag_id)
);

CREATE INDEX idx_samples_hash ON samples(blake3_hash);
CREATE INDEX idx_samples_path ON samples(relative_path);
```

### 9.3 Drag-and-drop ingest

Drop audio file(s) onto the plugin UI → samples ingest pipeline:

1. Compute blake3 hash of the source file.
2. If hash already in `samples` table → no-op (deduplication).
3. Otherwise: copy file into `Samples/incoming/` (or user-selected subfolder), convert to flac if not already, write metadata to SQLite, generate waveform preview, insert row.
4. Default behavior is **copy** (sample lives in library). Optional setting: reference-only (sample stays at original path; library tracks absolute path; portability cost).

### 9.4 Patch ↔ sample resolution

Patch file stores each excitation slot's sample reference as:

```toml
[slot.1]
sample_hash = "blake3:abc123..."
last_known_path = "breath/clarinet-chiff-soft-01.flac"
```

Resolution order on patch load:

1. Look up by `sample_hash` in SQLite → if found, use that path.
2. If not found by hash, try `last_known_path` → if file exists, hash it and add to library, then use it.
3. If still not found, mark slot as "missing sample" — UI shows red indicator, slot is bypassed, patch otherwise loads.

### 9.5 Embed-on-export

A "Export patch with samples" action bakes referenced sample audio into the patch file as base64-encoded flac. Receiving instance auto-imports on load. Useful for sharing patches across machines.

---

## 10. State & Presets

- Patches stored as TOML on disk in `Patches/` (human-readable, diffable, easy to version-control).
- Plugin state for DAW project save/load uses the shared `PluginState` and versioned TOML patch helpers through VST3's `IComponent::getState` / `setState`. Reload reconstructs the active patch plus unsaved parameter overrides.
- Patch file references samples by hash; resolution at load time per §9.4.
- Current patch/state payloads include the v2 audio input, expression, note-detection, and live-excitation fields directly. Lamath has no deployed pre-v2 compatibility surface.
- "Default patch" ships hardcoded — a single excitation slot referencing a synthesized noise burst (generated at plugin init), feeding a modal bank with the `marimba` template. So the plugin makes sound out of the box with no user samples.

---

## 11. UI (Vizia)

Layout sketch (described — full mockup deferred):

- **Top bar:** patch name, browse/save/export, library button, MIDI activity LED, CPU meter.
- **Left column — Excitation Engine:** 4 slot rows (sample name, velocity zone, gain, RR group, pitch-track toggle). Drop zones above each slot. Mute/solo per slot.
- **Center — Resonator A and Resonator B:** stacked panels, each with model-type switch (Modal / Waveguide), preset selector (modal only), and the resonator-specific parameter cluster. Routing switch (Series / Parallel) between them.
- **Right column — Output & Modulation:** filter (cutoff, res, mode), saturation (drive), master (gain, pan). Below: envelopes (amp + secondary), LFO, 4 user mod slots.
- **Bottom drawer:** sample library browser (collapsed by default). When expanded, shows tree of `Samples/`, tag filter chips, waveform preview, audition button (plays excitation through current Resonator A config).

UI runs at the editor framerate (typically 30–60 fps), pulls audio-thread state via lock-free reads.

---

## 12. Technology Stack

**Architectural decision: framework-less integration on top of raw VST3 bindings + off-the-shelf windowing/UI.**

No plugin framework (no nih-plug, no JUCE, no iPlug2). The VST3 ABI grunt work (COM vtables, FUnknown plumbing) comes from the MIT-licensed `vst3` crate (coupler.rs). Window lifecycle and NSView embedding — the one piece that genuinely earns its keep — comes from `baseview`. UI from `vizia` direct (not the `vizia-plug` adapter, which is the nih-plug bridge we're not using). Everything between those layers — parameter system, MIDI normalization, voice management, state serialization, audio buffer iteration, threading discipline — is project code under our control.

Rationale for going framework-less:
- The new MIT-licensed `vst3` crate (Oct 2025) removed the GPL-contamination tax that previously made framework-less unattractive.
- Plugin's architectural needs (multi-source `ExpressionStream`, future live-input excitation seam, custom sample-slot routing) don't fit cleanly into nih-plug's idioms.
- Plumbing surface for this plugin is modest (one stereo out, MIDI in, ~30 parameters, one editor window). Once written, it's owned and never breaks on a framework upgrade.
- Matches the approach used successfully for the C# VST host project.

### 12.1 Crate stack

| Layer            | Choice                                                 | Notes                                                                          |
| ---------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------ |
| VST3 ABI         | **`vst3` crate** (coupler.rs)                          | MIT/Apache 2.0. Pre-generated bindings, no libclang at build time.             |
| Plugin shell     | **`lindelion-plugin-shell` + Lamath adapters**         | Shared descriptors, parameters, process context, state, VST3 helpers, messages, MIDI normalization, and voice allocation. Lamath declares product CIDs, buses, payloads, patch paths, and runtime policy. |
| Window lifecycle | **`baseview`**                                         | NSView embedding, dpi, focus, event loop. Used directly, not via nih-plug.    |
| UI framework     | **`vizia`** (direct)                                   | Declarative, retained-mode. Custom binding into project parameter system.      |
| Format           | **VST3** only                                          | Ableton on Mac loads VST3 natively. CLAP not yet supported by Ableton (as of Live 12.4, May 2026). |
| Build target     | Apple Silicon primary, Intel best-effort               | Custom `xtask` produces `.vst3` macOS bundle (Info.plist, PkgInfo, ad-hoc signature). |
| DSP              | `lindelion-dsp-utils` plus Lamath resonator DSP        | Shared math/analysis/smoothing helpers; product-local modal bank and waveguide. |
| Audio analysis   | `lindelion-audio-expression`, `lindelion-pitch-detect`, `lindelion-onset-detect` | Shared streaming pitch/onset/loudness/expression surfaces for v2 sidechain analysis. |
| Sample I/O       | `lindelion-sample-library`                             | Shared loaded-audio ownership, file-library ingest, hashing, indexing, and preview generation. |
| Database         | `rusqlite` (bundled libsqlite)                         | Single-file embedded DB behind the sample-library file feature.                |
| Hashing          | `blake3`                                               | Fast content addressing.                                                       |
| Serialization    | `serde` + `toml` through shared patch/state helpers    | Patch files diffable; DAW state carries the versioned plugin state payload.    |
| SIMD             | `std::simd` (portable) or `pulp`                       | For modal-bank batching; see §13.                                              |

### 12.2 Plugin shell boundary

The workspace owns framework-less VST3 integration through shared shell crates plus thin product adapters. v2 work should extend the shared shell when the behavior is host protocol mechanics, and keep Lamath-only policy in Lamath.

Shared shell responsibilities:

- **Factory and component helpers.** VST3 factory registration, bus metadata helpers, state stream helpers, fixed-size view behavior, and typed message wrappers.
- **Process context.** `ProcessContext` carries setup, output buffers, optional audio input, MIDI events, and transport. v2 sidechain audio must enter through this shared input field.
- **Parameter registry.** Stable IDs, normalized/plain conversion, formatting, smoothing metadata, editor metadata, and apply dispatch live behind the shared registry model.
- **MIDI and expression contracts.** Host MIDI normalizes to shared `MidiEvent`; MIDI and audio-driven control both feed `ExpressionSource` / `ExpressionStream`.
- **Patch/state helpers.** Versioned TOML patch envelopes and `PluginState` roundtrips use shared patch I/O helpers.

Lamath-local responsibilities:

- Product CIDs, bus table, parameter list, patch paths, apply-policy enums, runtime targets, UI slots, and product-specific VST3 message payloads.
- Resonator DSP, voice-trigger policy, sidechain source modes, audio-created voice ownership, live-excitation routing, and latch-buffer policy.

### 12.3 VST3 validator

Steinberg's `validator` tool runs ~300 conformance tests against the bundle. The plugin must pass these for reliable behavior across hosts. The validator runs from the SDK distribution and reports pass/fail per check. Plan: integrate validator runs into the build (xtask target that builds, then runs validator, fails build on regression).

---

## 13. Performance

### 13.1 Per-voice cost model (rough, 48kHz)

| Component                     | Cost (per sample)            | Notes                                              |
| ----------------------------- | ---------------------------- | -------------------------------------------------- |
| Excitation playback (4 slots) | ~20 ops                      | Linear interp, mix sum                             |
| Modal bank, 64 modes          | ~256 ops (with SIMD batching) | 4 modes per 4-lane SIMD vector                     |
| Waveguide                     | ~40 ops                      | Delay tap + loop filter + clip                     |
| Filter (SVF)                  | ~10 ops                      |                                                    |
| Saturation                    | ~10 ops                      | tanh approximation                                 |
| **Total per voice**           | **~340 ops/sample**          | Both resonators at 64 modes is worst-case          |

8 voices × 340 ops/sample × 48000 samples/sec ≈ 130 MOps/sec. Comfortable on M-series silicon with room to spare; should idle well under 5% single-core CPU at moderate polyphony.

### 13.2 Mode count tradeoffs

- 32 modes: kalimba/woodblock convincing, bells thin
- 64 modes: sweet spot for v1 default
- 128 modes: bells, glass, gongs reach their full character; 2× CPU
- 256+ modes: diminishing returns for real-time; useful for offline render

### 13.3 Offline render mode

Detect via VST3's `ProcessSetup::processMode` — VST3 reports `kOffline` for bounce/render contexts. When offline:

- Mode count cap is removed (or set to a configurable "offline max", default 512)
- Voice count cap can be raised
- Internal oversampling factor optionally bumped (1× → 2× or 4×)

Patches store both a "live" mode count and an "offline" mode count. Live for performance, offline for bouncing the final track.

### 13.4 SIMD plan

The modal bank is the main SIMD target. Each mode is a 2nd-order biquad with shared input/output structure. Batch modes in groups of 4 (NEON) or 8 (AVX2): vectorize the per-sample update across modes. Expect ~3× throughput vs scalar.

Worth doing from the start because the architecture (mode parameter layout in memory) is hard to retrofit. Use `std::simd` portable intrinsics; fall back to scalar on unsupported targets.

---

## 14. V1 Scope vs V2 Extensions

### V1 (ships)

- Polyphonic (8 voices, configurable to 16)
- Resonator A + B, each Modal or Waveguide
- Series and Parallel routing
- 4-slot excitation engine with velocity zones, round-robin, layering
- SVF filter + soft saturation output
- Fixed mod routings + 4 user-assignable slots
- Sample library: SQLite-indexed, drag-drop ingest, hash-based patch references
- VST3, Mac primary

### V2 committed scope

- **Optional sidechain input bus on the existing instrument** — Lamath remains the same VST3 instrument and adds an optional audio input bus. Empty, inactive, or unrouted input preserves v1 MIDI-only behavior.
- **Sidechain audio creates notes** — audio onsets create voices through the existing voice manager. Stable pitch at onset chooses the MIDI note; later pitch drift becomes expression pitch bend.
- **Audio-derived ExpressionStream** — shared `lindelion-audio-expression` maps pitch, RMS, and spectral centroid to pitch bend, pressure, and brightness. Lamath owns voice ownership, retrigger policy, and source-mode behavior.
- **Live audio excitation** — sidechain audio can be `Off`, `Continuous`, `NoteLatched`, or `ContinuousAndNoteLatched` per patch. Continuous mode mixes bounded sidechain energy into active voices; note-latched mode captures a bounded onset window into a per-voice buffer.
- **MIDI/audio interaction policy** — v2 exposes MIDI-only, audio-created, and MIDI-plus-audio-created modes. Audio-created note-offs release only audio-owned voices; MIDI note-offs release only MIDI-owned voices.
- **Registry-backed v2 parameter surface** — v2 parameters cover source mode, expression enable/mapping, note detection thresholds, release hysteresis, velocity amount, live excitation mode/gain, latch window, latch pre-roll, and latch fade. Patch paths, apply policies, and runtime targets remain Lamath-local.
- **Patch/state payloads** — current patches and DAW states roundtrip the registry-backed v2 fields directly. No pre-v2 migration layer is maintained for Lamath.
- **Realtime contract** — sidechain scratch buffers, detector state, pre-roll rings, and latch buffers are allocated during setup or structural patch application. Audio-thread processing must not allocate, block, log, perform file/database I/O, or call UI/host services.

### Later extensions

- **Plate / membrane resonator** — third model type plugged into the existing resonator slot interface.
- **Banded waveguide** — fourth model type for bowed/glass timbres.
- **Cross-coupling / sympathetic resonance routing** — third routing mode where B's output partially feeds back into A's excitation.
- **Per-voice stereo placement** — pan and stereo width per voice for natural ensemble spread.
- **Microtuning** — Scala / .tun file support if a worldbuilding/game project needs it.

---

## 15. Current Implementation Status

Lamath is the current bundleable Lindelion VST3 instrument. The core audio path, parameter surface, patch state, sample-library loading, VST3 processor/controller boundary, native Vizia editor surface, and macOS bundle automation exist in the workspace.

### Implemented

- Stable host parameter surface with component/controller tests.
- Parameter updates mutate patch state and update active voices when the parameter is live.
- Smoothed live output, loop-gain, filter, pitch-bend, saturation, and routing controls.
- Structural resonator and modulation changes update future voices without killing active notes.
- DSP render, automation stress, sample-rate/buffer-size, offline, and no-allocation tests for the audio path.
- TOML patch save/load and DAW state roundtrip.
- File-backed sample library with ingest, hashing, indexing, preview generation, moved-file recovery, and missing-sample reporting.
- Native editor command services for patch save/load/export, sample ingest/assignment/clear, and telemetry requests.
- macOS VST3 bundle layout, moduleinfo generation, ad-hoc signing, staging, and install automation.
- Shared audio-expression types are available through `lindelion-audio-expression`; v2 still needs product integration for the optional sidechain bus, audio-created voice allocation, and live excitation routing.

### External Validation

- Steinberg validator and Ableton scan checks still need to be run on macOS after bundle-affecting changes. Linux target checks do not replace this.

## 16. Future Design Decisions

1. **Default mode count per template.** Each modal preset can ship with a recommended mode count while still allowing user override.

2. **Sample format on ingest.** The current file library preserves usable source references; a future ingest policy may re-encode excitations to mono 48 kHz FLAC for storage consistency.

3. **Offline render mode.** Patches already distinguish live performance constraints from higher-quality bounce targets in the design; product UI still needs an explicit workflow for that distinction.

---

## Appendix A — Glossary

- **Excitation:** the input signal that drives a resonator (struck/blown/plucked). In this synth: a sample.
- **Modal bank:** a parallel array of resonant filters, each modeling one vibrational mode of a physical object.
- **Waveguide:** a delay-line-based model of a 1D vibrating medium (string, tube). Sound emerges from the feedback loop's standing-wave behavior.
- **Mode:** a single vibrational frequency of a physical object. A kalimba tine has a few strong modes; a cymbal has hundreds.
- **Position of strike:** where on a resonating object the excitation is applied. Affects which modes are excited (a string struck at its midpoint excites mostly odd harmonics).
- **ExpressionStream:** internal abstraction for per-voice continuous control (pitch bend, pressure, brightness, gate). v1 driven by MIDI; v2 optionally driven by audio analysis.
