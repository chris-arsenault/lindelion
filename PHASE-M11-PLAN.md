# M11 — Instrument voicing program (milestone plan)

Make every resonator family (**Modal, String, Tube, Mesh**) sound like a real instrument across its
full **time-varying spectrum** — not a single parameter. The current state (measured): the waveguide/
mesh families have **no living tail** (Tube ~30 ms blip, Mesh ~100 ms to silence, String ~0.4 s),
strings are **perfectly harmonic** (zero stiffness), there is **no aliveness** (no pitch drift, beating,
or sustained noise bed), and Mesh/Tube/Modal lack body coloration. M11 fixes the tail first, then voices
the steady tone, body, dynamics, and level — building the *real physics* per the confirmed decisions.

This is the front-half (feature-start) plan: milestone-altitude phases with exit gates; step-level
file/test detail is filled by `plan-phase` just before each phase runs. **Out of scope:** M12 audition
(deferred — this pass is meter + spectrum only); any change to `ModalBank`'s *sound* (its level may
change; it is the reference); the plugin framework ([ADR-0002](docs/adr/0002-no-plugin-framework.md)).

## Confirmed decisions (this session)

- **Sustain = true indefinite-while-held via a continuous-excitation driver.** Build a **bow driver**
  for the String (extends the M8 driver layer, [ADR-0017](docs/adr/0017-additive-physical-driver-layer.md));
  the **reed** already self-oscillates for the Tube. Gate held → continuous excitation → no decay;
  note-off → excitation stops → natural ring-out.
- **Dual-polarization string** — two slightly-detuned coupled traveling-wave pairs (horizontal/vertical
  polarizations) for natural **beating** and the **two-rate decay** (fast initial + long aftersound) of
  real strings/pianos. A waveguide architecture change.
- **Mesh, full realism** — lower the damping, **add frequency-dependent decay** (high plate modes die
  first → metallic shimmer), and **add body/radiation coloration**.
- **Register-aware voicing** — per-note scaling of decay/brightness/inharmonicity across the keyboard
  (bass longer & more inharmonic & darker; treble shorter, purer, brighter).
- **Meter + spectrum only** this pass; the genuinely ear-dependent calls (aliveness "alive vs seasick")
  wait for M12.

## Per-family voicing targets

Objective calibration goals (meter/spectrum; auditioned in M12).

| Family | Sustain (held) | Decay structure | Inharmonicity | Body / radiation | Aliveness |
| --- | --- | --- | --- | --- | --- |
| **Modal** (reference) | per-preset (mallet 1–3 s, bell longer) | per-mode poles + `decay_tilt` (already frequency-dependent) | per-preset ratios (realistic) | none (idiophone, radiates direct) | sympathetic |
| **String** | **indefinite via bow** (gate held); free pluck rings long | frequency-dependent (loop filter) + **two-rate** (dual-polarization) | **realistic stiffness default** (currently 0!) | Guitar/Violin body (exists) | **beating** (dual-pol) + **pitch drift** + sustained bow noise + sympathetic |
| **Tube** | **indefinite via reed**; struck = short by physics | loop filter + mouth loss | harmonic (bore) | bell radiation + **add bore body** | breath noise + sympathetic |
| **Mesh** | long metallic shimmer | **add frequency-dependent decay** | natural plate (2D physics) | **add body coloration** | shimmer + sympathetic |

## Context / reuse map (verified against current code — re-derive, don't trust this summary)

- **Decay.** Waveguide T60 is **capped at `WAVEGUIDE_DECAY_MAX_SECONDS = 2.5` s** (`core.rs`), default
  loop_gain 0.92 → ~0.5 s; frequency-dependent decay already works via the loop lowpass
  (`loop_damping`, 8 kHz default cutoff). Tube `MOUTH_REFLECTION = −0.36` drains a struck bore (needs
  the reed). Mesh boundary damping `fixed(0.16)` → ±0.84 reflection, **uniform (no T60(f))** —
  `mesh_2d.rs`. Modal: per-mode pole radius + `decay_tilt` + per-preset `decay_seconds` (Bell 3.5 s) —
  the reference, *not* capped at 2.5 s.
- **Inharmonicity.** Waveguide `WAVEGUIDE_DISPERSION` default **0.0** (`constants.rs`) — pure harmonic,
  no stiffness; dispersion via two allpass stages (`dispersion.rs`), String-only. Modal: per-preset
  `special_ratios` + `inharmonicity` param. Mesh: inherent 2D plate inharmonicity.
- **Body.** String `ReducedBody` (`body.rs`: Guitar 11 modes / Violin 10, two-way bridge,
  `BODY_GAIN_SCALE = 0.08`). Tube bell radiation (`RADIATION_*`) but no discrete body modes. Mesh/Modal:
  none. Output stage SVF is the only post-resonator filter (`output_stage.rs`).
- **Aliveness (absent).** No LFO→pitch route (`modulation_state.rs` — pitch is frozen per note); single
  `TravelingWavePair` (no detune/polarization, `traveling.rs`); M10 surrounding noise is **attack-only**
  (5/30 ms envelopes, `surrounding.rs`). Sympathetic chamber exists (`sympathetic_chamber.rs`).
- **Drivers / energy.** M8 driver layer (`resonator_stack/driver.rs`: pick/reed) inside the 2x loop,
  effort-driven; the reed self-oscillates. The bow is a new archetype here.
- **Measurement.** Have: `audio_window_metrics` (rms/peak/centroid), `harmonic_decay_profile`
  (early/late ratio), `estimate_f0_autocorrelation_refined`, `spectral_centroid_hz`, `band_energy`,
  `render_metric_profile`, `render_response`, the in-session gated diagnostics + `Voice::stage_peaks`.
  **Gaps to add:** per-partial **T60**, **inharmonicity ratio** (partial freq vs n·f0),
  **centroid-over-time**, **attack/sustain ratio**, a **held-note (note-on, no note-off) render helper**.

## Cross-cutting constraints

- **Allocation-free audio thread** ([ADR-0001](docs/adr/0001-allocation-free-audio-thread.md)); every new
  stage sizes buffers at construction + `assert_no_allocations`. The bow driver, dual-polarization pair,
  mesh decay filters, and noise/drift generators all obey this.
- **Heavy measurement/calibration tests are gated** behind the `integration-tests` feature +
  `#[ignore]`, run via `make test-integration`; never in the fast in-memory `make ci` (AGENTS.md). Keep
  cheap unit guards for wiring.
- **`ModalBank` sound is untouched** — only its level may change. It stays the decay/partial reference.
- **Stability.** Near-unity loop gain (long sustain), the self-oscillating bow, and the coupled
  dual-polarization pair are all stability edges — every phase asserts bounded/finite under hard drive.
- **Meter-only verification.** Objective fixtures (T60(f), inharmonicity ratios, centroid-over-time,
  beat-rate, noise floor, level). Aliveness "feel" is M12.
- **Doc surface.** Each phase's lasting architectural decision (bow driver, dual-polarization, mesh
  realism, register-aware voicing, gain-staging/limiter) lands an ADR when the phase lands (reserve
  **ADR-0026+**, via repo-docs), with the calibrated targets recorded; one `CHANGELOG.md` line per
  user-visible change; trade-offs in ADRs.

## Phases

Ordered by perceptual impact + dependency: **the tail must exist before its character can be voiced;
level is last.** P1 unblocks all (measurement). P2 (decay) precedes P3–P8 (everything voices the tail).
P9 (dynamics) and P10 (level) come once the voice is whole.

### P1 — Voicing measurement battery
Extend the gated battery with the missing objective metrics: per-partial **T60**, **inharmonicity ratio**,
**centroid-over-time** (spectral darkening), **attack/sustain ratio**, and a **held-note render** helper;
fold in the in-session per-stage taps. The objective-fixture foundation every later phase asserts against.
- Exit: `make test-integration` reports finite, sane metrics for every dimension/family; `make ci` fast/green.

### P2 — Decay structure & energy retention — make the tail exist  [depends on P1]
Raise the waveguide T60 cap (well past 2.5 s); re-tune the String decay + loop-filter so the tail rings
with the right frequency-dependent darkening. Mesh: lower damping and **add frequency-dependent decay**
(high modes die first). Verify near-unity-loop stability (no growth).
- **[DECISION]** String default T60 / loop-filter cutoff; mesh decay model (per-mode filter vs
  frequency-shaped boundary) and target ring-out.
- Exit: `make test-integration` — held/struck tails ring to target with frequency-dependent darkening;
  bounded/stable; the Tube/Mesh dead-tail is gone; isolated decay tests re-targeted and green.

### P3 — Sustain drivers: true indefinite-while-held  [depends on P2]
Build the **bow driver** (continuous, effort-driven excitation) extending the M8 driver layer; confirm
the **reed** sustains the Tube. Gate held → no decay; note-off → excitation stops → ring-out.
- **[DECISION]** Bow archetype (friction-curve / velocity), its parameter surface, and the note-off
  ring-out coupling.
- Exit: `make test-integration` — a held note sustains without decay (bow/reed); note-off rings out;
  bounded/stable; sample/pluck excitation unaffected; no-alloc.

### P4 — Partial structure: inharmonicity + harmonic content + body  [depends on P2]
Set a **realistic string stiffness** (dispersion) default; voice the steady harmonic content per family;
add **body/radiation coloration** to Mesh (and bore body to Tube); verify String body and Modal presets.
- **[DECISION]** String stiffness default; mesh/tube body mode sets; per-family timbre (centroid) targets.
- Exit: `make test-integration` — per-family inharmonicity ratio, harmonic profile, and body formants
  within target bands and pairwise distinct; Modal sound unchanged.

### P5 — Dual-polarization string: beating + two-rate decay  [depends on P2, P4]
Replace the single traveling-wave pair with two slightly-detuned **coupled** polarizations → natural
beating and the two-rate decay of real strings.
- **[DECISION]** Detune amount, inter-polarization coupling strength, decay split (fast/slow).
- Exit: `make test-integration` — measurable beat rate + a two-slope decay envelope; bounded/stable;
  tuning preserved; no-alloc. (ADR — waveguide polarization model.)

### P6 — Micro-imperfection / aliveness  [depends on P2]
Add **pitch micro-instability** (a slow drift/vibrato route, currently absent) and a **sustained noise
bed** (extend the M10 attack-only noise to a continuous bow/breath/air component); sympathetic exists,
beating from P5.
- **[DECISION]** Pitch-drift depth/rate and whether it's a fixed voice character or a control; sustained-
  noise spectrum/level per family.
- Exit: `make test-integration` — measurable pitch variance and a sustained noise floor during the held
  tail (vs the dead-stable / silent-tail baseline); defeatable; no-alloc.

### P7 — Register-aware voicing  [depends on P2, P4]
Scale decay/brightness/inharmonicity per played note across the keyboard (bass longer/darker/more
inharmonic; treble shorter/purer/brighter).
- **[DECISION]** The per-register scaling curves and their span.
- Exit: `make test-integration` — the voicing metrics vary monotonically with pitch to the target curves;
  extremes stable.

### P8 — Dynamic response (M4–M10) re-tuned in the living tail  [depends on P2–P7]
Re-confirm tension, steepening, mesh geometric, body coupling, contact, balance, surrounding, and
sympathetic — now that a real sustaining tail exists and levels have moved — so each effect is audible
and correctly sized in the tail and stable. The energy references (tuned to the old anemic levels) move.
- Exit: `make test-integration` — combined-stages effect-size + stability battery green; isolated M4–M10
  tests still pass.

### P9 — Whole-path gain staging + family level balance + no-clip  [depends on P2–P8]
Stage every stage (excitation → driver → contact → resonator → body → surrounding → output → sympathetic)
to a healthy level (high SNR, target peak ≈ −18…−6 dBFS); balance family loudness at matched dynamics on
the *real tails*; add a soft limiter (ceiling −0.3 dBFS, identity below threshold) + small final
normalization on top of `INTERNAL_HEADROOM_DB`.
- **[DECISION]** Per-stage healthy target, family loudness tolerance, limiter/normalization design.
- Exit: `make test-integration` — families within loudness tolerance across vel 20/100/127; no gesture
  clips. `make ci` guards: limiter identity for unity sine; staged gain applied; no-alloc.

### P10 — Control ranges/defaults from data + docs + exit gate  [depends on P2–P9]
Set min/max/default/taper of every voicing + M4–M10 control from the measured data (registry + patch in
sync); write the phase ADRs (reserve ADR-0026+) and `CHANGELOG.md`; delegate doc conventions to repo-docs.
- Exit: `make ci` green (shown); `make test-integration` green on the full voicing battery; `make bench`
  within the 1–4 voice budget; ranges documented; no-alloc.

## Decisions needing your input (collated)

| Phase | Decision you own |
| ----- | ---------------- |
| P2 | String default T60 / loop-filter cutoff; mesh decay model + target ring-out. |
| P3 | Bow archetype + parameter surface; note-off ring-out coupling. |
| P4 | String stiffness default; mesh/tube body mode sets; per-family timbre targets. |
| P5 | Dual-polarization detune / coupling / decay split. |
| P6 | Pitch-drift depth/rate (character vs control); sustained-noise spectrum/level. |
| P7 | Per-register scaling curves + span. |
| P9 | Per-stage healthy target; family loudness tolerance; limiter/normalization design. |

---

This plan is the single source of truth. To execute, run `plan-phase` on one phase to expand it into
ordered, file-level steps, then the companion execution prompt. M11's measurement/calibration tests run
via `make test-integration`, not `make ci`.
