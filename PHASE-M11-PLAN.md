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
level is last.** P1 unblocks all (measurement). P2 (decay) precedes everything that voices the tail.
**P1–P4 are done; P5–P7 are deferred to the backlog (realism polish, not correct-function — see below).**
The remaining critical path is the *tuning trio*, gated on real-time cost: **RT (CPU budget) → P8 (re-tune
the dynamic effects against the living tail) → P9 (gain staging / family balance / no-clip) → P10 (control
ranges/defaults + docs).** That trio is the original "tune the stages" intent.

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

### P5–P7 — DEFERRED to the backlog (realism polish, not correct-function)
**Dual-polarization beating/two-rate decay (P5), micro-imperfection/aliveness (P6), and register-aware
voicing (P7) are moved to the Lamath backlog** ([docs/plugins/lamath-backlog.md](docs/plugins/lamath-backlog.md)).
After P1–P4 the resonators *function correctly* — every family rings, sustains, holds tune, and the four
are timbrally distinct — so these three add aliveness, not correct function. They are also a real-time
cost the instrument may not be able to spare (P5 roughly doubles the String waveguide). Revisit as opt-in
polish after the instrument is level-staged, control-calibrated, and **confirmed real-time**; gate any
reinstatement on the CPU budget.

### RT — Real-time / CPU budget gate  [depends on P4]  — ✅ PASSED
Benched per-resonator and per-voice cost vs the 20.8 µs/sample (48 kHz) budget. **Real-time at sensible
polyphony** (1 voice ≈ 6.8 % of budget; ≤ 8 voices comfortable; ~14–15 dual-resonator voices at the limit;
16-voice dual-resonator chords ~7 % over). **The dispersion cascade was exonerated** — ~5–7 % of a String
voice, not the suspected hot spot. The real costs are the **Mesh grid** (2.16 µs/sample, the heaviest) and
the **2× oversampling**; high-polyphony headroom (if ever wanted) lives there, not in M11 tuning. Numbers
were taken on a slow server CPU, **not** the Apple-Silicon target — the real workstation will be faster, so
this is a conservative pass. (A pinned macOS run is the eventual real contract per `docs/performance.md`.)
- Result: passed; no trimming warranted. Per-resonator (oversampled): Tube 0.78 µs · String 1.13 µs ·
  Mesh 2.16 µs/sample.

### P8 — Dynamic response (M4–M10) re-tuned in the living tail  [depends on P2–P4, RT]  — ✅ DONE
Re-confirm tension, steepening, mesh geometric, body coupling, contact, balance, surrounding, and
sympathetic — now that a real sustaining tail exists and levels have moved — so each effect is audible
and correctly sized in the tail and stable. The energy references (tuned to the old anemic levels) move.
- Exit: `make test-integration` — combined-stages effect-size + stability battery green; isolated M4–M10
  tests still pass.

**Outcome (what was actually wrong + done):**
- **Energy references were ~15–60× too high (the core bug).** The per-voice energy bus actually peaks near
  RMS **0.010** (String/Mesh) / **0.004** (Tube) at full velocity — measured via a new `Voice::measured_energy()`
  test accessor — but every reference assumed ~0.15–0.3. So *every* dynamic effect ran at <1–7 % of its
  range — effectively inaudible on real notes. The effects were always *designed* to reach drive 1.0 (the
  "+40 cents", "cuivré bloom", etc.); only the references kept them from getting there. Recalibrated all six:
  tension 0.15→**0.012**, balance 0.3→**0.012**, steepen 0.15→**0.005**, geometric 0.15→**0.013**, radiation
  0.2→**0.012**, sympathetic 0.15→**0.004**. New gated calibration battery
  (`dynamic_effect_energy_references_track_real_playing`) pins the real bus level to the references' drive band.
- **Balance polarity was inverted + measured in the wrong window.** The body radiation (presence formant) is
  actually the *brighter* sustain voice and the loop-damped pickup the *warm* one — so the energy→position
  sign was flipped (now **soft→pickup/warm, loud→body/bright**: harder = brighter), and the test now measures
  the sustain window (the attack is a shared bright onset). The balance now has real audible authority
  (loud ≈30 % brighter centroid).
- **Midrange evenness via a body loading/radiation decouple.** Added `BODY_LOADING_SCALE` (0.22): the modal
  body now *loads* the loop with only a fraction of its admittance (midrange notes sustain seconds instead of
  being choked on plate modes) while still *radiating* its colour at full gain. Lengthened the in-gap default
  String pluck to ~5.8 s as a side effect (longer = better).
- **Tube steepening has positive energy feedback** (it adds harmonics → pumps the loop): at the new ref a
  full-velocity Tube *blooms* from RMS 0.004 → 0.019 (the brass cuivré dynamic), saturating the drive — the
  intended maximum. The battery asserts on computed *drive*, not raw energy, to allow this.

### P9 — Whole-path gain staging + family level balance + no-clip  [depends on RT, P8]  — ✅ DONE

**Outcome (what was found + done):**
- **Per-family levels were ~38 dB apart.** A single full-velocity voice peaks at Modal ≈ −1.7 dBFS but
  String/Tube/Mesh at ≈ −36/−28/−40 dBFS, and the waveguides ran ~30 dB under a usable level. Added a
  **per-family output makeup** (Modal 0.6× / String 32× / Tube 12× / Mesh 49×) bringing every family's
  single voice to ≈ −6 dBFS peak, matched on **peak** (crest factors differ ~8×, so peak-matching avoids
  clipping the plucky families). `crates`-clean: the makeup lives in `resonator_stack/makeup.rs`.
- **Makeup is applied per-resonator, *before* the A/B mix, but the energy tap stays on the raw mix.** A
  single post-mix makeup over-amplified a loud+quiet mix (Modal+waveguide parallel → rms 2.6); applying
  each slot's makeup before the mix fixes it, and tapping energy from the raw mix keeps P8's bus untouched
  (the pinned decoupling). `process_sample` now returns the raw mix and stores a `staged_output` the voice
  reads for the audio path.
- **Bow driver tamed + driver-trimmed.** `BOW_INJECTION_GAIN`/`BOW_OUTPUT_LIMIT` 4.0 → 0.12/0.5: a held bow
  no longer runs to energy-bus RMS ~8.7 but locks at a sane forte ~0.3 (a self-oscillator's locked cycle
  can't go arbitrarily low without un-locking). Because a self-oscillating bow is intrinsically ~8× a pluck
  at the output, a **per-driver trim** (Bow 0.2×) folds into the makeup so a bowed note lands at a forte
  level instead of slamming the limiter.
- **Master soft-clip safety stage** (`dsp/master_stage.rs`, wired in `runtime` after the sympathetic
  chamber): a stateless, lookahead-free, allocation-free per-sample soft clipper — identity below a −6 dBFS
  knee (single notes untouched, bit-exact), soft knee to a −1 dBFS ceiling. **No loudness normalization** —
  the P8 dynamic range is preserved; only the static makeup + safety clip.
- **Sympathetic reference re-confirmed at the post-makeup mix.** `SYMPATHETIC_SEND_ENERGY_REF` 0.004 → 0.2
  (the makeup raised the observed mix from ~0.0025 to ~0.1–0.34) so a forte note reaches meaningful send
  drive and chords saturate.
- Exit: `make ci` + `make test-integration` green.

**Decided (P8 coupling):** gain staging is applied **output-side — after the M2 energy tap**
(`observe_energy(resonator_output)`), so it never moves the resonator's physical vibration level that
the dynamic effects key off. The energy bus stays the physical-amplitude tracker; the input/excitation
drive is set physically (velocity/effort), not re-staged here. This decouples P8 from P9 by construction.
Stage every stage (excitation → driver → contact → resonator → body → surrounding → output → sympathetic)
to a healthy level (high SNR, target peak ≈ −18…−6 dBFS); balance family loudness at matched dynamics on
the *real tails*; add a soft limiter + master makeup on top of `INTERNAL_HEADROOM_DB`.
- **The core is ~30 dB too quiet.** A single full-velocity voice outputs RMS **0.0025 ≈ −52 dBFS** (P8
  `max_out_energy`); P9 must add output-side makeup to reach the healthy target, *after* the M2 energy tap so
  P8's calibration is untouched.
- **From P8 — two things to fix here:** (1) the **bow driver self-oscillates absurdly hot** — a sustained
  limit cycle reaches energy-bus RMS **2.6–8.7** (vs ~0.01 for a pluck), far above full-scale; the driver
  gain (`BOW_INJECTION_GAIN`/`BOW_OUTPUT_LIMIT = 4.0`) needs taming so a held bow sits at a sane level.
  (2) the **sympathetic send reference (0.004) observes the post-output mix**, the one energy reference
  *downstream* of this gain staging — re-confirm it once the final output level is set.
- **Decided (P9 plan-phase, research-grounded — not blocking):**
  - *Healthy target* — single full-velocity voice peaks ≈ **−12…−6 dBFS**, RMS ≈ **−18 dBFS** (the standard
    instrument-bus staging level); the existing `INTERNAL_HEADROOM_DB = −12` stays as the polyphony headroom
    (16 incoherent voices ≈ +12 dB → ≈ 0 dBFS), so the makeup goes *before* it (but after the energy tap).
  - *Family tolerance* — Modal/String/Tube/Mesh within **±3 dB** at matched dynamics; **Modal is the loudness
    reference** (sound untouched, level may move — cross-cutting constraint).
  - *Limiter/normalization* — a per-sample, allocation-free, lookahead-free **soft clipper** (identity below
    a ≈ −6 dBFS knee, soft-knee to a ≈ **−1 dBFS** ceiling) as the master safety stage. **No auto-loudness
    normalization** — it would squash the P8 dynamic range we just calibrated; only the static per-family
    makeup + the safety clip. Impact: a busy chord rides into the soft knee (gentle, transparent) rather than
    being level-flattened.
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
