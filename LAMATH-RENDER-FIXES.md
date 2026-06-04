# Lamath Render-Catalog Fix Tracking

Working tracker for the issues surfaced by the human review of the Lamath render
catalog (`review/lamath-render-catalog-comments.json`, reviewed 2026-06-02). Not
durable docs — delete or migrate to `CHANGELOG`/ADR when the work lands.

Source analysis: every comment was cross-referenced against the case→recipe
mapping in `plugins/lamath/src/bin/lamath-render-catalog/` and the DSP defaults in
`plugins/lamath/src/patch.rs` + `plugins/lamath/src/dsp/constants.rs`.

## Two parallel goals

1. **Fix the sounds** — items P1–P9 below.
2. **Fix the test suite** — item T1. The in-depth audio-quality suite passed every
   one of these. Important correction to an earlier wrong read: the suite is *not*
   missing A/B or decay tests — it has a substantial T60 decay battery, frequency-
   darkening checks, per-family centroid-distinctness, mesh ring-out duration,
   pick/reed/bow sustain, velocity dynamics, and pitch tracking. The regressions
   slipped through on **threshold and gating semantics**, not absence of coverage.
   See "T1 — test-suite gap investigation" for the evidenced root causes. T1 is the
   reason the regressions shipped, and each P-item's red test feeds the T1 fix.

## Status

- **P1 + P9 (Tube): ✅ DONE — shipped v0.14.0.** Tube is a playable monophonic driven wind voice
  (beating-reed flow + mouth-termination + implicit junction → pitch-locks C2–C6; pressure-window
  dynamics; per-driver output trim; tongued/slur articulation via timing + breath ramp + bore glide;
  silent variants removed + scale demonstrators added). Durable record:
  [ADR-0032](docs/adr/0032-lamath-tube-driven-wind-voice.md), the [Lamath spec](docs/plugins/lamath.md)
  §4.2, CHANGELOG v0.14.0, and the [Lamath backlog](docs/plugins/lamath-backlog.md) (deferred:
  stronger brassiness, formant spectrum, UI-locked mono/driver, bound articulation, better default
  excitation). The completed working plan has been removed; this tracker now covers only **P3–P8 + T1**.

## Status legend

- `⬜` not started   `🔶` in progress   `✅` done   `—` n/a
- **Scope** decides process: `param` = tuning/calibration only (fix directly);
  `DSP` = algorithm/structure change → run **feature-start** for a design session
  before touching code.

## Matrix

| ID | Problem | Affected samples | Est. root cause (file:line) | Scope | Identified | Red test | Fixed | Human-validated |
|----|---------|------------------|------------------------------|-------|:----------:|:--------:|:-----:|:---------------:|
| **P1** | Tube has no audible struck/plucked voice — total silence | `baseline_tube_*` (all vel), `register_tube_*` (all reg), `contact_tube_*` (all), `driver_tube_sample` | Closed-tube waveguide (`boundary_reflection=-0.75`, `constants.rs:154`) given only an impulse strike with no sustained driver; no internal self-excitation path. **No tube test renders the default −0.75/0.97 operating point** — every tube test overrides to +0.8/0.85 & loop_gain 0.985+ (`tube_1d/tests.rs`); calibration battery passes it on `rms>0.0`. | DSP | ✅ | ⬜ | ⬜ | ⬜ |
| **P3** | String driver/excitation does not color the sound | `driver_string_pick_soft/hard`, `driver_string_sample` | Pick hardness/contact-time (`render.rs:189–195`) reach the patch but are inaudible — "just slightly louder." Excitation shaping washed out by the dominant Karplus loop; same class as the M11-P8 energy-reference miscalibration (15–60× off). | DSP? (confirm) | ✅ | ⬜ | ⬜ | ⬜ |
| **P4** | Contact model inert | `contact_string_tight/wide × short/long` | `ContactConfig` spread/contact-time (`render.rs:220–239`) produce no audible difference. | DSP? (confirm) | ✅ | ⬜ | ⬜ | ⬜ |
| **P5** | Source-body balance inert (flagship M9/M11 feature) | `source_body_string_depth000/050/100` (v020 & v127) | `source_body_balance` depth 0.0→1.0 (`render.rs:241–255`, default `0.5` `patch.rs:300`) produces no audible change. **Test gap:** the A/B test (`balance_tests.rs:47`) sweeps *energy* at fixed depth=0.85 via direct `set_balance_drive`, never sweeps *depth* through the energy follower at a played velocity — the catalog's actual axis is untested. | DSP? (confirm) | ✅ | ⬜ | ⬜ | ⬜ |
| **P6** | Sympathetic resonance does nothing | `surrounding_modal_sympathetic`, `*_mechanical_sympathetic`, `*_radiation_sympathetic`, `*_all`; `chord_modal_*_sympathetic_on` | Shared sympathetic bank at depth `0.90` (`render.rs:268`) inaudible while its siblings (mechanical, radiation) work. Likely not fed the mix or output gain negligible. | DSP? (confirm) | ✅ | ⬜ | ⬜ | ⬜ |
| **P7** | Bow driver broken — runaway crescendo / silent scratch | `driver_string_bow_smooth` ("crescendos, no decay, too much feedback"), `driver_string_bow_scratch` ("no audible sound") | Stick-slip friction model (`render.rs:197–206`) unstable/uncalibrated; behaves as feedback loop, not a bow. | DSP | ✅ | ⬜ | ⬜ | ⬜ |
| **P8** | Low-register string dies at default loop gain | `register_string_c2_v100` ("no sound, glitched") vs `edge_string_source_body_low_c2` ("made a sound this time") | Default `WAVEGUIDE_LOOP_GAIN=0.97` (`constants.rs:141`) doesn't sustain the long C2 delay line; edge case rings only because it bumps to `0.985`. Needs frequency-compensated loop gain. | param→DSP (confirm) | ✅ | ⬜ | ⬜ | ⬜ |
| **P9** | Reed driver is fuzz/scream, not a reed | `driver_tube_reed_soft` ("high-pitched scream, no reed, just fuzz"), `driver_tube_reed_hard` ("super bit-smashed") | Self-oscillating reed driver (`render.rs:207–216`) uncalibrated; the only tube excitation that makes sound, and it's distortion. Related to but distinct from P1. | DSP | ✅ | ⬜ | ⬜ | ⬜ |
| **T1** | **Audio-quality suite let all of the above through** | n/a (meta) | **Investigated (not absence of A/B-decay tests — those exist).** Root causes: "audible"=`rms>0.0`; ring=`Option`-gated (vacuous on no-ring); A/B="not bit-identical" (1e-6); strict perceptual tests sample non-default operating points; perceptual targets deferred & never backfilled; thin register coverage. Strict tests run on isolated cores, full-synth tests carry loose asserts — bugs live in the integration neither covers. See notes below. | process | ✅ | — | ⬜ | — |

## Confirmed-good (regression guards to lock in, not bugs)

These the human liked — worth pinning so fixes elsewhere don't break them:
Modal (all vel + all registers), basic String pluck (C4/C6), all chords,
`surrounding_*_mechanical`, `surrounding_*_radiation`, and the liked edge cases
(`edge_string_dense_hard_chord`, `edge_mesh_low_damping_high_material`,
`edge_tube_closed_nonlinear`, `edge_tube_open_nonlinear`).

## T1 — test-suite gap investigation (the "why did this pass?" question)

**Investigated** — these are confirmed root causes from reading the suite, not
hypotheses. The tests exist and are rigorous; they pass bad sound because of *how*
they assert, *where* they sample the parameter space, and *what* they excite.

### Confirmed defects in the existing assertions

1. **"Audible" is asserted as nonzero, not as audible.**
   `calibration_tests.rs:206` gates audibility on `clip.rms > 0.0`; sub-component
   tests use `> 1.0e-8` / `> 1.0e-9`. A tube that is silent to the ear renders
   ~1e-5 RMS and passes every one of these. The comment says "should render audible
   output" — the predicate says "not exactly zero." → **Catches P1, P8.**
   Fix: a real dBFS sustain floor (e.g. post-attack window RMS > −60 dBFS, and a
   *minimum-sustain* gate for struck families).

2. **Decay/ring assertions are `Option`-gated and pass vacuously on failure.**
   `calibration_tests.rs:225`: `t60_seconds.is_none_or(|t| ... t > 0.0)`. When a
   resonator dies instantly, `partial_t60_seconds` returns
   `None`, so the one assertion that should catch "doesn't ring" is *skipped*
   exactly in the failure case. Same pattern on `centroid_endpoints` and
   `attack_sustain_ratio`.
   Fix: for struck/plucked families, T60 MUST be `Some` and inside a target band.

3. **A/B "materially changes" thresholds are "not bit-identical," not "audible."**
   `render_tests.rs` exposed-parameter checks use `rms_difference > 1.0e-6`;
   `tube_1d/tests.rs:198` "materially changes" uses `> 0.000_001`; mesh position
   `> 1.0e-5`. These pass on changes far below any perceptual threshold. →
   **Catches P3, P4, P5, P6** (knobs that "apply" but are inaudible).
   Fix: gain-normalized spectral-distance / centroid-delta floors set at a
   perceptual level, not a numeric-noise level.

4. **Strict perceptual sub-tests sample non-default operating points.**
   Every tube test overrides `boundary_reflection` to **+0.8/0.85** and `loop_gain`
   to **0.985–0.992** — never the default **−0.75 / 0.97** (`tube_1d/tests.rs`
   passim). The balance A/B (`balance_tests.rs:47`) sweeps **energy** at a fixed
   **depth=0.85** and drives `String1d` directly via `set_balance_drive`, never
   sweeps **depth** at a played velocity through the energy follower (the catalog's
   axis). So the guarded region is the well-behaved one; the default patch's actual
   operating point and the catalog's actual sweep axis are untested. →
   **Catches P1, P5.**
   Fix: every family must have a perceptual test *at its shipped default patch*; A/B
   tests must sweep the same axis the user/catalog sweeps.

5. **Perceptual targets were deliberately deferred and never backfilled.**
   `calibration_tests.rs` says so: *"P1 only guards that the fixtures are computable
   and well-formed… the loudness-MATCH target is M11 step 3… absolute target bands
   are left to the M12 audition"* (lines 198, 264, 300). The battery was built as a
   *computability* harness with real targets pushed to later phases + M12 human
   audition (this review). The targets never landed, so the only real perceptual
   gate was the human — which is why this is the first time the defects surfaced. →
   This is the *systemic* cause behind 1–4.
   Fix: backfill the deferred target bands as automated assertions now.

6. **Register/velocity coverage is thin.** Calibration battery runs only at note 60
   (C4); the decay battery at 165–392 Hz. C2 (≈65 Hz, P8) is below everything
   tested, and per-knob A/B is usually at one velocity. → **Catches P8.**
   Fix: sustain/ring assertions across C2–C6 and low velocities.

### Architecture note (not a defect, but the structural reason)

The *strict* perceptual tests run on **isolated DSP cores** via
`render_*_response` (impulse / shaped-pluck / sustained-sine fed straight into
`WaveguideResonator` / `ModalBank` / `Mesh2d`), while the **full-`ResonatorSynth`**
tests (which use defaults and MIDI note-on) carry the **loose** assertions (1–2
above). So the rigorous checks never see the integrated voice (excitation
generator → driver → contact → energy follower → ADSR → surrounding → master), and
the integrated-voice checks never assert anything perceptual. The bugs live in the
*integration*, where neither half looks. The catalog renders through
`set_patch_with_loaded_excitations(patch, Vec::new())` — **empty** excitations —
the exact integrated path no strict test covers.

### T1 deliverable

A new perceptual-assertion layer that closes 1–6: a per-family *shipped-default*
held-note render through the full synth asserting (a) sustained dBFS floor, (b)
mandatory in-band T60 for struck families, (c) attack→decay envelope shape (not
crescendo, catches P7), across (d) C2–C6 and low velocities; plus differential A/B
gates at perceptual thresholds on the catalog's own sweep axes (driver, contact,
source-body depth, sympathetic). Cheap in-memory guards land in `make ci` per ADR
rules; multi-second sweeps gate to `make test-integration`. Each P-item's red test
is one row of this layer.

## Test → sample-failure traceability

For each sound defect: the sample(s), the test that *nominally* covers that
behavior, the exact passing assertion, why it let the defect through, and the gap
class. Gap classes: **T1.1** audible=`>0` not audible; **T1.2** ring is
`Option`-gated (vacuous on no-ring); **T1.3** A/B="not bit-identical" threshold;
**T1.4** test exercises an isolated DSP core or a non-shipped operating point, not
the integrated full-synth voice at the shipped default; **T1.5** presence asserted,
not envelope-shape/timbre quality; **T1.6** register/velocity coverage hole.

| P | Sample failure | Nominal covering test (file:line) | The assertion that passed | Why it passed the defect | Gap |
|---|----------------|-----------------------------------|---------------------------|--------------------------|-----|
| P1 | Tube silent (`baseline/register/contact_tube_*`, `driver_tube_sample`) | `calibration_tests.rs:261` battery (Tube default, held note); tube core suite `tube_1d/tests.rs` | `clip.rms > 0.0` (`:206`); `t60.is_none_or(..)` (`:225`); core tests assert decay but **only at `boundary_reflection`=+0.8/0.85, `loop_gain`≥0.985** | Near-silent tube is nonzero → passes "audible"; dead tail → T60 `None` → vacuous; **no core test ever renders the default −0.75 / 0.97** | T1.1, T1.2, T1.4 |
| P3 | String driver inert (`driver_string_pick_soft/hard`, `sample`) | `resonator_stack/tests.rs:~150` pick brightness | `hard_centroid > soft_centroid * 1.1` | Renders the isolated `ResonatorEngine` + impulse; passes in isolation, but the full voice's energy/effort path collapses the difference. No full-synth driver A/B exists | T1.4 |
| P4 | Contact inert (`contact_string_tight/wide × short/long`) | `resonator_stack/tests.rs:438` `strike_position_spread_changes_timbre…` | gain-invariant centroid shift `> 0.15` (picked 0.0 vs strummed 0.85) | Renders the isolated String core with `excitation_spread` **hard-set on `WaveguideParams`** (`:423`), bypassing `ContactStage`→effort→spread and the integrated voice | T1.4 |
| P5 | Source-body inert (`source_body_string_depth000/050/100`) | `balance_tests.rs:47` `source_body_balance_shifts_timbre_soft_vs_loud` | `loud_centroid > soft_centroid * 1.15` | Sweeps **energy** at fixed **depth=0.85** via direct `String1d::set_balance_drive`; the catalog sweeps **depth** at fixed velocity through the energy follower — that axis is never tested | T1.4 |
| P6 | Sympathetic inert (`surrounding_*_sympathetic*`, chord `sympathetic_on`) | `sympathetic_chamber/tests.rs:30,61,83,106` (ring, pitch-track, harmonic couple, super-linear) | tail `rms > 1.0e-3`; on/off ratios | Feeds a burst (energy 0.5) **directly into `SympatheticChamber::process_block`**; the full-synth mix→chamber send / chamber→output return gain (the actually-inaudible part) is never exercised | T1.4 |
| P7 | Bow crescendo / silent scratch (`driver_string_bow_smooth/scratch`) | `resonator_stack/tests.rs:244`; `energy_calibration.rs:195` bow sustain | `late > mid * 0.7` (and `late_rms > 0.005`) | It's a **lower bound** — a runaway crescendo (late ≫ mid) trivially satisfies "doesn't fall below 70% of mid." Guards against over-damping, not against growth. `BowScratch` params untested | T1.5 |
| P8 | C2 string silent (`register_string_c2_v100`) | `decay_tests.rs` string battery; `calibration_tests.rs:261` | T60 ∈ 4.5–6.5 s, etc. — **at 165 / 220 / 392 Hz**; calibration at note 60 (C4) only | Lowest tested string fundamental is 165 Hz; C2 ≈ 65 Hz is below everything. The default `loop_gain 0.97` failing to sustain a long delay line is never measured low enough | T1.6 |
| P9 | Reed fuzz, not a reed (`driver_tube_reed_soft/hard`) | `resonator_stack/tests.rs:193` reed self-oscillation | `late_blown > late_quiet * 5.0` | Asserts the reed gets *loud when blown* (it oscillates) — fuzz is loud, so it passes. Nothing asserts the **spectral/harmonic quality** of a reed vs broadband distortion. Isolated engine, not full voice | T1.5, T1.4 |

**The one-line story:** every broken sub-feature *has* a passing, rigorous
perceptual test — but **7 of the remaining 8 (all but P8) pass because the strict test runs on an
isolated DSP core or a non-shipped operating point, never the integrated full-synth
voice at the shipped default patch.** The only tests that *do* run the full synth at
defaults (`calibration_tests.rs`) carry vacuous gates (`rms>0`, `t60.is_none_or`).
So the rigor is real but aimed away from the integration, and the integration is
guarded only by "doesn't crash / isn't exactly zero." P8 is the lone pure
coverage-hole (register). This is why T1's fix is *one* new layer — full-synth,
shipped-default, perceptual, across register — not nine scattered patches.

## Process notes

- A row whose Scope is `DSP` gets a **feature-start** design session before code.
  `param` rows are fixed directly with a red test first.
- Several `DSP?` rows need a confirmation deep-dive to settle param-vs-DSP before
  committing to feature-start — that triage is the first action per item.
- Human-validation is via WAV-render audition (the granted exception for Lamath);
  every other exit stays automated per the no-human-verification default.
