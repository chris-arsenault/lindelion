# Lamath Tube (driven wind voice + reed driver) — current deficiencies

> **THREAD COMPLETE (2026-06-10): the register-key voice is audition-approved.** Low E, the
> register-key high sustain (body color, breath balance, attack, release), and the fast low/high
> articulation all passed audition. The decision set is recorded in
> [ADR-0049](docs/adr/0049-lamath-tube-register-key-voice.md); the current model is documented in
> [docs/plugins/lamath-tube.md](docs/plugins/lamath-tube.md); remaining work (soft-velocity vented
> range, pre-break G4/G#4 sag, parked tone guards, shaped breath, altissimo) is in
> [docs/plugins/lamath-backlog.md](docs/plugins/lamath-backlog.md). The interim A/B comparator
> cases, patch switches, and their unit tests were removed after approval (shipped renders
> hash-verified bit-identical through the cleanup). The investigation log below is retained as
> history.

Assessment from ADR-0032, the Lamath spec, the backlog, the reed/bore code
(`tube_1d.rs`, `driver.rs`, `makeup.rs`), the wind-path wiring in
`resonator_stack.rs`, the full-synth guards in `tube_voice_tests.rs`, and memory of
this thread.

## Current state (2026-06-05) — extracted to its own crate + plugin

The tube has been extracted out of the monolithic Lamath: the DSP now lives in
`crates/lindelion-wind` (`ReedTube`/`ReedTubeParams`/`ReedTubeSwitches`, `ReedDriver`,
`TubeBody`) and the product in `plugins/lamath-tube` (`TubeProcessor`). The **plugin layer
is built out**, addressing several items: 8 key-switchable **articulation excitations**
(Tongue/Sforzando/Legato/Staccato/Marcato/Breath/Accent/Slur) replace the generic builtin
impulse (**F12** "fat tongue" + **F11** bound articulation), brightness/damping/pressure/
stiffness/embouchure/bell are patch params, and `ReedTubeSwitches` give per-component A/B
toggles (bell / bore-steepening / body). `TubeBody` already carries a formant pair (≈280 Hz
bore body + ≈1500 Hz bell flare) as A1 scaffolding.

The **headline DSP redesign is where work picks up**. The audition/render harness now exists.
Audition changes must be true A/B renders: each experiment needs paired current-vs-new cases in the
same render group, with the investigated mechanism as the only intentional variable. The old
`11_tube_dynamics` bell on/off cases are only bell A/Bs; they are not valid A/Bs for bore, body,
reed, or register-key work.

1. **Energy-conserving bell** — ✅ *done (2026-06-04), audition = marginal.* `output_sample` now
   radiates the bell-end pressure `incident + reflected` (= `(1+R)·incident`, bounded by the incident
   wave), shaped by the fixed far-field HF filter; removed the old non-conserving tap (`×2.5` gain,
   `effort²` gating, double-count). Guarded by `bell_radiation_does_not_depend_on_effort`. Objective
   wins: the ff balloon is gone (`tube_dyn_scale_v127_bell_on` peak −4.3 → −11.6 dBFS), bell is now
   fixed-efficiency. **Audition (user, 2026-06-04): "slightly better but not materially so."**
   *Why marginal:* the bell still radiates the bore's bright internal wave **through a high-pass**, so
   it re-emphasizes highs. bell-off sounds like a clarinet only because the pickup→`TubeBody` path
   low-passes the same bright wave warm; bell-on un-tames it. **The square is upstream — in the bore,
   not the bell tap.** So #1 was necessary (killed the amplification/balloon) but not where the
   material tone gain lives.

### Next levers, by likely impact (revised after the #1 audition)

**Audition discipline:** a weak/no-effect A/B does **not** invalidate the physical lever by itself.
First assume the attempted delta may have been too small, mixed too low, or placed where the signal
path masks it. Before marking a lever low-impact, make a stress render that should be obviously
audible if the path has leverage, then bracket back toward a nominal setting. Do not infer from
micro changes like a 1 Hz cutoff move, a same-cutoff order swap, or a low-mix formant tweak.

2. **Bore HF loss (the "bore" half) — still unresolved.** The loop (gain ~0.97 + a mild mouth
   low-pass) sustains a bright/buzzy wave that the pickup merely hides and the bell re-exposes. A real
   bore loses highs fast; make the loop properly lossy at HF so the *sustained wave itself* is a warm
   clarinet — then pickup and bell are both warm and the "square" goes away at its source. Failed
   audition (2026-06-05): an extra bore-wall high shelf inside both traveling directions produced
   note-dependent flat/sharp tuning damage and did not materially improve timbre, so it was removed.
   Next step is contribution audit, not stronger loop-filter tweaking.
3. **Body/formant warmth (A1) — still unresolved.** `TubeBody` has a fixed ≈280 Hz + ≈1180 Hz pair,
   but it has not yet been proven to carry enough of the audible signal to fix the missing h3 ring.
   Failed audition (2026-06-05): tracking the high body resonance to the third partial with more h3
   gain had no material audible effect at the first levels; that proves level/mix leverage was too
   low, not that formant body is invalid. `14_tube_output_paths` auditions the existing current/body-
   only/dry-pickup+bell/dry-pickup paths. Audition update: strong h3 body-only is slightly different
   and slightly better, but the effect is small and largely masked in the full mix.
   Decision direction: strong h3 body is useful, and the reduced-bell audition should become nominal
   50% bell mix per project convention. `16_tube_body_formant_mix` now auditions the body-dominant h3
   setting with bell off/nominal/full, with nominal as the intended default and full bell as the
   brighter upper comparison.
4. **Radiation shape — inconclusive at the first delta (2026-06-05).** Current bell radiation is a 2nd-order
   500 Hz highpass outside the oscillator loop. With strong h3 body + nominal 50% bell accepted as the
   current target, full bell remains useful but leans square-wave. `17_tube_radiation_shape` compared
   the current 2nd-order radiation against a gentler first-order radiation transfer at nominal and
   full bell, with the body/bell tuning otherwise held fixed. Audition result: barely audible/micro.
   That does **not** prove radiation shape is invalid; it proves this particular order swap at the
   same cutoff was not a meaningful enough delta. A real radiation-shape audition needs a wide,
   intentionally obvious bracket before deciding whether the lever matters.
5. **Bore steepening — active structural audition (2026-06-05).** The current Tube loop includes an
   amplitude-dependent allpass steepener inside the mouth/bore feedback path. Unlike bell radiation
   or body EQ, this can alter the sustained wave itself and is velocity-dependent. `18_tube_bore_steepening`
   compares current steepening against steepening disabled at nominal and full bell.
6. **Brightness from the source / cuivré** (`reed.rs`): the beating duty cycle should shift with
   blowing pressure so the bore generates more harmonics when blown harder. Only meaningful once the
   base tone is warm. **Reed-aperture status (updated 2026-06-05): inertia kept, pitch fixed.**
   Finite reed inertia is a real source-side de-harshening (it band-limits the reed's hard gating,
   stripping ~72% of the 1–3 kHz energy at C4, centroid 1383→943 Hz). The pitch flatness it introduced
   is **diagnosed and compensated** — see the 2026-06-05 investigation outcome below for the full
   lineage — and the earlier "treat it as a structural oscillator problem, do not compensate" guidance
   is **superseded**: the flatness was a *fixed loop group delay* (the inertial aperture is a
   minimum-phase 2nd-order low-pass inside the reed→bore feedback), now corrected in the bore-length
   tuning. Only `12_tube_reed_aperture` (instant vs inertial) remains as the A/B for this thread; the
   integrated-boundary and predicted-aperture experiments were both ruled out and removed (including
   `13_tube_aperture_prediction`).
7. **Shaped breath** (item 14, `reed.rs`): white-noise turbulence → band/formant-shaped air.

Plugin layer already addresses **F11/F12** (8 key-switchable articulation excitations replace the
generic impulse) and gives the `ReedTubeSwitches` A/B toggles used above.

### Reference comparison (2026-06-04) — real clarinet vs our tube, harmonic structure

Compared a real **Iowa clarinet G4** (`testdata/audio/iowa_clarinet_G4.wav`, 392 Hz) against the G4
note of `11_tube_dynamics/tube_dyn_scale_v100_bell_on.wav` (same pitch), magnitudes normalized to the
fundamental:

| harmonic | real clarinet | our tube | Δ (ours − real) |
| --- | --- | --- | --- |
| h2 (even) | −48 dB | −28 | +20 |
| **h3 (1176 Hz)** | **+5.6** | **−3.5** | **−9** |
| h4 | −12.5 | −24 | −12 |
| h5 (1960) | −18 | −5.6 | **+12** |
| h7 (2744) | −22 | −7.9 | **+14** |
| h9 | −24 | −11 | +13 |
| h11 | −35 | −15 | **+20** |
| h13 (5096) | −57 | −20 | **+37** |

centroid: real **1950 Hz** vs ours **2293 Hz**; odd/even 7.9 vs 9.3.

Quantified targets (these sharpen levers #2 and #3):

1. **HF rolloff is the dominant gap = the "square".** A real clarinet rolls off steeply above ~h5
   (h7 −22, h11 −35, h13 −57); ours barely rolls off (h7 −8, h11 −15, h13 −20) — its upper odd
   harmonics are **+14 to +37 dB too hot**, i.e. a square's `1/n` tail. **Target: ~15–35 dB more
   attenuation across h7–h13**, via bore HF loss (lever #2). This is the big win.
2. **Missing the clarinet's defining h3 peak.** Real clarinet's *strongest* harmonic is h3
   (**+5.6 dB over the fundamental**) — the "ring" lives in the h3–h5 band (≈1.2–2 kHz). Ours has h3
   weak (−3.5). **Target: a resonant emphasis around ≈1.2 kHz (h3)** — body/formant (lever #3).
   `TubeBody`'s current ≈280 Hz + ≈1500 Hz pair does not sit on h3; it is mistuned for this.
3. **Even-harmonic distribution differs (secondary).** Ours is slightly *more* odd-dominant overall;
   the shape differs (real has a notable h4, suppressed h2). Minor next to (1) and (2).

Method: `scipy` rfft on a steady window, peak-pick per harmonic; reference fixture is the existing
Iowa MIS clarinet sample (no sourcing needed).

## First, a scoping caveat

The driven-wind Tube **shipped and works** at the baseline bar: it self-oscillates
across C2–C6, locks to the fundamental, sustains audibly below the clipper, and plays
a scale in tune. The original "silent tube" defect (P1/P9) is genuinely fixed. So the
deficiencies below are *quality/expressivity/UX gaps*, not "it's broken."

One honest note: `review/lamath-render-catalog-comments.json` has **no post-fix tube
audition** — the old silent-tube cases were deleted and the new driven-tube/articulation
cases were never auditioned (the JSON still only carries the old string-driver and mesh
comments). So the list below is synthesized from ADR-0032's own "Accepted divergences /
deferred" section, the backlog, and reading the code — **not** from fresh ears. A WAV
audition of the new cases is the obvious gate before/with any of this.

## Deficiencies, grouped

### A. Timbre — the tone is thin and synthetic

1. **Odd-harmonic square, not a formant-shaped clarinet.** ADR-0032 and the spec both
   admit the output is "an odd-harmonic square rather than a fully formant-shaped
   clarinet." The bore's standing-wave resonances don't sculpt the spectrum into
   formants — `loop_filter_cutoff` just rolls off brightness globally (`tube_1d.rs:411`
   drops the effective mouth cutoff so ~8 kHz lands near ~2 kHz). Register and
   termination read as smaller timbral changes than they should.
2. **Brassiness-with-effort (cuivré) is weak and unguarded.** This is called out as
   deferred in the ADR *and* visible in code: the energy-driven steepening
   (`STEEPEN_ENERGY_REF=0.005`, `apply_steepening`, radiation path at `tube_1d.rs:264`)
   is the only loud-brightening mechanism, the static `loop_nonlinearity` saturation is
   *disabled* in the wind path (it destabilizes into period-doubling —
   `process_sample_wind` comment at line 247–250), and the tube_1d comment itself says
   steepening "does not yet produce a strong brightening." There is **no A/B test**
   asserting a louder note is brighter, precisely because it's too weak to assert.

### B. Dynamics — compressed and mostly timbral — ✅ DONE (v0.15.0, 2026-06-03)

> Resolved via effort-referenced bore steepening: blowing pressure (effort) now drives the
> steepening + bell radiation, so a louder note brightens into the cuivré (centroid ≈2460→3900 Hz
> at C4 mf→ff, odd-harmonic richness rising register-wide) plus a ~10 dB level dynamic. Investigation
> also found the reed's timbre was blowing-pressure-invariant (the real root cause) and that the
> "overblow" belief was an autocorrelation octave artifact — the reed holds its fundamental across
> the low/mid register; the altissimo squeak is kept as physically-accurate behaviour. Pitch tests
> moved to a DFT metric. See the [ADR-0032 item-B update](docs/adr/0032-lamath-tube-driven-wind-voice.md#update--item-b-dynamics-and-brightness-with-effort-2026-06-03).

3. **Velocity barely moves the voice.** To keep pitch stable, effort is mapped into a
   *deliberately narrow* reed pressure window (`REED_PRESSURE_FLOOR=0.63` →
   `REED_PRESSURE_CEIL=0.76`, `driver.rs:42`). The spec states the velocity dynamic is
   "largely timbral within that pressure window." So pp→ff is a small level/timbre
   change, not the dramatic dynamic a wind player has. The
   `driven_tube_sits_below_the_master_clipper` test only asserts `loud > soft * 1.1` — a
   10% level delta is the *bar*, which tells you how compressed it is.

### C. Tuning

4. **Top octave is flat — more than "a few cents"** (ADR + spec). The scale guard tolerance is 60
   cents (a full quartertone), which *masks* this rather than fixing it. Measured 2026-06-05 on the
   **instant** aperture (i.e. reed-comp-independent, so this is the bore residual, distinct from the
   now-fixed reed-inertia flatness): −6c at C2 worsening monotonically to −96c at C6. It grows with
   pitch — the same fixed-group-delay signature as the reed flatness — so it is a phase-delay/
   fractional-delay compensation residual (`delay_tuning` in `prepared_model`) and is amenable to the
   same closed-form phase-delay-at-f0 correction now used for the reed.

   **Update 2026-06-09:** current low-register probes show this is **not** a global low-register
   delay error. Sustained D3/G3/C4/E4 are within about ±1.5 cents; the drift only starts near the
   register break (G4 −5c, G#4 −8c) and the register-key high sustain remains flatter. Treat the
   remaining pitch problem as a near-break/high-register impedance or mode-locking issue, not a
   whole-tube static tuning offset.

### D. Control ranges that aren't fully playable (the user's recurring concern)

5. **`boundary_reflection` (bell) has a large dead range.** ADR-0032: the termination is
   "physically self-narrowing — an open bell radiates too much for the reed to sustain,
   so only the closed-bell band oscillates." So much of the 0..1 control collapses to
   silence/non-oscillation. This is exactly the "a parameter must have a valid range
   across 0..1 or be fixed" rule from the Mesh-damp lesson — currently violated for the
   bell control.
6. **`loop_nonlinearity` is a dead control for the Tube** — it's bypassed in the wind
   path (stacking it period-doubles). Either repurpose it (e.g. drive the cuivré bloom)
   or hide it for Tube.

### E. Reed-model fragility (band-aids, not robust operating points)

7. **Period-2 sub-harmonic lock is dodged, not solved.** The pressure window *skips* the
   near-threshold zone where the bore period-doubles (`driver.rs` comment ~276–283), and
   a hard-blown reed is kept out of period-2 lock only by injected breath noise dither
   (`REED_BREATH_NOISE=0.02`). Both are workarounds around an operating point the model
   can't sit on cleanly.
8. **Low-register ff overblows the octave** (accepted in ADR;
   `driven_tube_holds_fundamental_across_velocity` explicitly excludes vel 127). So
   fortissimo in the bottom octave is pitch-unreliable.
9. **Gain staging is a brittle magic constant.** The reed makeup trim is `0.13`
   (`makeup.rs:43`) hand-tuned to not slam the clipper — fragile, and the kind of thing
   that re-breaks when the reed level shifts (this is the exact trap that produced the
   bit-crushed-saw regression).

### F. Playability / UX (backlog "Driven Wind Tube (post-M1)")

10. **Forced settings are applied silently.** `normalize_drivers_for_resonator_models`
    rewrites driver→reed and polyphony→1 on load with no UI feedback. The editor should
    **lock** the mono + reed-driver controls with an explanatory tooltip, not quietly
    overwrite the patch.
11. **Articulation is implicit in note timing only.** Tongued vs slurred is inferred from
    note overlap; there's no bound, automatable parameter or key-switch to select
    phrasing live.
12. **Default reed excitation is the generic struck-instrument impulse,** not a tuned
    breath/tongue chiff — so a fresh Tube patch lacks a musical onset out of the box.

### G. Liveliness / humanization

13. **The sustained tone is sterile/identical note-to-note** — no breath-pressure variation
    or embouchure quiver feeding micro-changes into the reed pressure equation. A real player
    constantly micro-modulates blowing pressure and embouchure; without it the voice reads as
    static and "digital." (New, from the 2026-06-03 audition. User: defer, backlog.)
14. **Breath turbulence is unshaped white noise.** `REED_BREATH_NOISE * mouth * next_noise()`
    in `driver.rs` feeds raw xorshift white noise into the breath, so the breath reads as
    white/pink hiss rather than air moving through a bore. It should be **spectrally shaped**
    (bandpass/formant-filtered wind noise, weighted to a breath-like band and coupled to the
    bore), or ultimately driven from a **recorded breath sample**. (New, 2026-06-03 audition.)

## Audition outcome (2026-06-03) — item B landed, tone not yet there

User auditioned the re-rendered articulation cases (tongued + slur, C4→C5 scale):

- ✅ **Slur and tongued transitions sound good** — confirms the legato glide is *not* an audible
  click (resolves the open question from item B; my "bounded portamento transient, not a step"
  read was correct by ear).
- ✅ Articulations are **clearly distinct**; top of the scale is **fine** (clean at forte).
- ⚠️ **Tongued attack is a "fat tongue," not agile** — points straight at **F12** (the generic
  struck-instrument impulse instead of a reed chiff).
- ⚠️ **Tone is still "mostly a square — digital highs, no resonant body or warm tones."** Item B's
  brightness *landed* (tone "slightly better") but it rides on a bare odd-harmonic square, so the
  added highs read as digital. This is **A1** (formant/resonant-body spectrum) and is now the
  headline tone deficiency: the body warmth is what will make B's brightness sound musical.
- ⚠️ Part of the "digital" feel is **sameness** — see new item **13** (humanization).

Net: the changes landed but were not 100% successful — the *dynamic* improved; the *timbre* (body)
and *onset* (chiff) are the gap.

## Audition outcome (2026-06-04) — item B's brightness is perceptually inaudible

User re-auditioned the **baseline-dynamics** velocity cases (tube C4 at v20/100/127): **no audible
tone difference — only quiet/loud.** So the measured centroid rise (2460→3900) does *not* translate
to a perceived brightness change. Combined with the code review of the bell tap, the conclusion is
that item B's brightness mechanism is a non-functional band-aid:

- **The bell HF-radiation tap is not physically sound** (`tube_1d.rs`): it (a) **double-counts
  energy** — `boundary.right` is both reflected back into the loop (`end_reflection`) *and*
  re-emitted to the output (`radiated`), instead of a real open end *splitting* incident into
  reflected (low-pass) + radiated (complementary high-pass); (b) uses an **unphysical gain**
  (`RADIATION_GAIN = 2.5`, >1 — emits more than arrives); (c) **gates radiation by effort²**, but a
  bell's radiation *efficiency* is fixed — what should rise with blowing is the *harmonic content
  the reed generates*, and the reed spectrum is blowing-pressure-invariant (measured), so the tap
  exists to fake brightness onto a fixed square. That's why it reads "digital" and barely moves.
- `effort = max(velocity, aftertouch)` (`modulation_state.rs`), so effort ≈ velocity; the tap *is*
  wired velocity²-dependent, but the contribution is too weak/synthetic to hear as timbre.

**New backlog item (part of step #1 / A1): redesign the bell model + its contribution together with
the bore model** — an energy-conserving frequency-dependent open end (reflected = low-pass, radiated
= complementary high-pass, unity gain) and brightness generated *at the source* (reed duty cycle
shifting with blowing pressure), so the body (A1) has a coherent signal to color instead of a hot
square + non-conservative HF boost to fight. A `bell_radiation` patch scale (0..1, default 1.0) now
exists as the audition/diagnostic A/B handle for the current tap.

## Audition outcome (2026-06-04b) — bell on/off A/B: the tap SQUARES the tone (my metrics were wrong)

User auditioned the new `tube_dynamics` bell on/off A/B. **My centroid/DFT read was wrong** — the bell
is not a near-inaudible band-aid, it is *dominant and harmful*:

- **v20:** bell on/off makes no audible difference (the tap is effort²-gated → ~off at low effort).
- **v100:** **bell ON = a square wave; bell OFF = a (digital) clarinet** — an *exceptional* difference,
  and peak/RMS differ wildly too. The bell tap is what was making the voice sound like a square; the
  clarinet was underneath it the whole time.

Corrected conclusions:

1. **The radiation tap (output-side `RADIATION_GAIN·highpass·effort²`) is the tone-wrecker**, not a weak
   add. At higher effort it dumps a huge HF layer that squares the tone and inflates the level. The
   in-loop steepening (still on with bell off) is *not* the culprit — bell-off already sounds clarinet.
2. **Bell OFF is the better base** ("digital clarinet"). The shipped default should not be the square.
3. **Item B's brightness direction is wrong**: it makes loud notes brighter by turning this tap *up*,
   i.e. by squaring them — the opposite of a musical cuivré. Brightness-with-effort must come from the
   source/formant, not the HF tap.
4. **Do not trust centroid/DFT for tube tone** — they did not distinguish square from clarinet here.
   Audition is the arbiter (the standing T1 lesson, reinforced).

Immediate options: (a) drop the default `bell_radiation` to ~0 (ship the clarinet) as a one-line stopgap
ahead of the full redesign; (b) go straight to the energy-conserving bell+bore redesign under A1.

## Investigation outcome (2026-06-05) — reed-inertia pitch flatness diagnosed and compensated

Picking up lever #5: finite reed-aperture inertia (the damped 2nd-order aperture, ω≈1.4 kHz, ζ=0.92)
materially improved timbre but played badly flat. This thread diagnosed the flatness from first
principles, compensated it, and cleaned out two dead experiments.

**Diagnosis — a fixed loop group delay, not a structural oscillator defect.** The inertial aperture is
a minimum-phase 2nd-order low-pass on the pressure→opening map sitting *inside* the reed→bore feedback
loop, so it lags the wave it gates and lengthens the effective acoustic loop. Evidence:

- The flatness grows ~linearly with pitch (uncompensated reed drag −53c at C2 → −368c at C5; ≈−190c at
  C4) — the signature of a *fixed sample delay* eating a larger fraction of a shorter period.
- The delay matches the aperture filter's analytic phase delay (`arg(H(z))/θ` ≈ `2ζ/ω` ≈ 9 samples),
  and the discrete symplectic-Euler integrator runs *less* delay than the continuous ideal (−1 sample),
  so it is honest physics, **not** a numerical artifact. There is no "free" integrator win, and
  lightening the reed to recover pitch would re-brighten the tone (undoing the timbre gain).

This supersedes the earlier "treat it as a structural oscillator problem, do not add cents/delay
compensation" guidance: a *fixed* loop delay is exactly what the bore-length tuning already compensates
for the mouth-loss and damping filters, so compensating the reed is the same principled correction, not
a per-note fudge.

**Fix — compensate the reed's loop phase in the bore-length tuning.** An explicit `reed_phase_delay_samples`
term, computed closed-form from the aperture resonator transfer function
(`ReedDriver::aperture_phase_delay_samples`), is folded into `delay_offset_samples` with a **fixed ½**
round-trip→one-way factor (`REED_PHASE_ONE_WAY_FACTOR`), deliberately *not* the empirical `warm_bore`
phase_scale ramp — riding that ramp doubled the comp in warm bores and overshot to +240c sharp. Two
source-derived scale factors lift the bare filter phase to the loop phase the nonlinearity actually
sees: `REED_LOOP_PHASE_COUPLING = 1.25` (the lag reaches the loop through the nonlinear `√|Δp|` flow
product, ≈1.25× the bare phase) and `REED_EFFORT_PHASE_SLOPE = 0.35` (the √-flow's higher incremental
gain at quiet swings gives the lag more leverage, so soft notes need more comp — without it pp/ff
detune relative to each other). Validated: reed drag nulled to within a few cents across C2–C5 (C4
−190c → +0.4c in rendered audio), effort spread at G3 47c → 11c; `lindelion-wind` and `lamath-tube`
suites green.

**Experiments removed.** The integrated mouth-boundary coupling (did not move pitch) and the predicted
inertial aperture (Test 3 — "no audible/material change") are both dead ends. The prediction experiment
is now fully removed: the `aperture_prediction` param (`reed.rs`/`patch.rs`/`processor.rs`), the
`InertialPredicted` catalog variant, and the `13_tube_aperture_prediction` group/case/render dirs +
manifest entries. Only `12_tube_reed_aperture` (instant vs inertial) remains as this thread's A/B.

**Timbre standing — a real but PARTIAL A1 win, now finally auditionable in tune.** With pitch fixed, the
inertial reed can be judged on tone for the first time (the ≈190c flatness had dominated every prior
audition). It is genuine *source-side* HF de-harshening — band-limiting the reed's hard gating *inside*
the feedback loop, so it darkens the sustained tone, not just the onset (≈72% less 1–3 kHz energy at
C4, centroid 1383→943 Hz) — and it moves in the right A1 direction. But it is **subtractive** (a
low-pass that darkens), not **constructive**: it does not add the clarinet's h3 formant ring (~1.2 kHz)
or a resonant body, and it does not make the bore's own sustained wave lossy. Underneath it is still a
*darker square*. Beware the standing T1 trap — "darker reads as fixed but isn't." A1 (lossy bore +
formant body + energy-conserving bell, brightness generated at the reed source) is still the headline
tone fix; the inertial reed is one keepable source-side piece of it.

## Audition outcome (2026-06-05) — Humanize is useful but register-dependent

The new `Humanize` knob drives independent steady random walks for reed pressure, embouchure, and
body/formant voicing. The auditioned low-E result is good: pressure + embouchure variance adds the
held-note motion that was missing from the reference match, and voicing variance adds the expected
formant motion. The maximum setting is intentionally beyond normal performance, but it must still keep
the note speaking; pressure depth above ≈0.40 can cross the reed's oscillation threshold and create
complete cutouts, so the current max pressure depth is backed down to 0.37 while embouchure remains
0.35 and voicing remains 2.0.

Open caveat: the effect is **register dependent**. It works well on the low-E reference but is too
strong on the register-key C# audition. Before treating the knob as finished, Humanize needs
pitch/register-aware scaling (likely from the tracked note/f0 already available to the Tube processor),
so the high register gets less pressure/voicing excursion than the low-register sustain.

## Implementation note (2026-06-05) — first register-key bore shunt

The Tube now has a first physical register-key model: a user-settable `Register Break` MIDI note
(default 69, concert A4 / written B on B-flat clarinet) opens a register vent above the break. The
processor keeps the sounding pitch high, uses a 2.88 bore-mode ratio, and applies a lossy side-hole
shunt near one third of the bore length. The shunt scatters pressure inside the traveling-wave pair
before pickup and bell output. The side-hole flow is also radiated as an **output-only** bright register
key path, so it can add upper-register reed/vent color without feeding that radiation signal back into
the bore or reed. A small output-only turbulent side-hole component now fills the missing 2-6 kHz
non-harmonic floor; it is driven by open-vent flow and band-limited so this stays a register-key/nozzle
radiation effect, not a full-output noise blanket.

Audition status: `19_tube_reference_match` now includes
`tube_ref_register_key_high_no_register_vent.wav` as the old short-bore comparison and level-matches
the vented high sustain. The vented case is now a real timbre A/B, not only a pitch/topology A/B: the
vent path fills some missing upper partials while keeping the known high-register flatness as a separate
problem. The latest audition matches the reference's 2-3 kHz broadband floor much better and gets close
at 3-4 kHz, but the >4 kHz rolloff remains too shallow and h5 is still low. The high sustain also
remains ~20 cents flat. Treat this as a useful first register-key topology, not a finished
upper-register model.

## Correction (2026-06-05) — contour is legacy reference only

The note-tracked post-oscillation `clarinet_contour_sample()` path was a bad low-E fix: it forced h3/h5
and upper contour after the oscillator instead of making the reed/bore/body/radiation system generate
the right spectrum. The code now keeps that path behind an internal legacy switch, but the normal Tube
model has it **disabled**. `19_tube_reference_match` includes
`tube_ref_low_e_sustain_legacy_contour.wav` as a contour-on comparator, while
`tube_ref_low_e_sustain_current.wav` exposes the physical-only path.

Physical low-E repair now started in the model itself:

- the body resonance can track the true third partial in the low register instead of clamping to
  ~760 Hz, which had put the "h3 body" on low-E h5;
- the resonant body coupling was reduced from fitted-EQ strength to a smaller radiated-body
  contribution;
- the low body resonance was moved off low-E h2 and reduced, so it no longer acts as an even-partial
  boost;
- bell radiation cutoff was raised so the bell no longer re-emits low-E h5/h7 as efficiently as a
  generic 500 Hz high-pass tap.

Current low-E physical-only audition: h3 and h5 are now close without the contour (`h3 -7.4 dB`,
`h5 -19.1 dB` vs reference `h3 -9.6`, `h5 -17.2`). A small bell-radiation selectivity move
(`1600 -> 1750 Hz`) pulled h7/h9 down modestly without the failed upper-loss filter that over-featured
h5. Remaining misses are h2 still too strong and upper odd periodic peaks (h7/h9) still too hot; h2 is
not materially affected by body leakage or bell cutoff, so it likely needs reed/bore odd-mode behavior
rather than more body or bell level changes.

## Investigation checkpoint (2026-06-09) — register-key pitch/tone and low-register probes

This is the current state of the active Tube investigation.

### What is now established

1. **Low E is now a physical-model match, not a post-contour cheat.** The note-tracked
   `clarinet_contour_sample()` path remains a legacy/internal comparator only; the current low-E
   path uses the physical reed/bore/body/radiation model. The last low-E work fixed the major
   h2/h4 even leakage and added the h9/h11/h13 upper odd body/radiation tail. User audition:
   "probably good enough." Keep low E as the lower-register reference baseline, not the target for
   register-key work.
2. **The register-key high sustain is close enough in general timbre to be worth refining, but it is
   still restrained/stuffed and slightly flat.** Current reference-match auditions live in
   `19_tube_reference_match`, grouped by `reference_gesture=register_key_high_sustain`.
3. **The air band is over-present.** The register-key current render still has too much high
   airy/breathy content relative to the reference, while the reference's musically important
   register-body region is stronger around the 1.2-2 kHz h3/h4/h5 area. Treat "air" as harmonic/
   turbulent radiation balance, not as disposable noise; do not solve it by broad final filtering.

### Register-key harmonic/tap read

Steady-window comparison for `tube_ref_register_key_high_reference` vs
`tube_ref_register_key_high_current`:

- Reference fundamental peak: about 441.3 Hz.
- Current full sim with register vent and phase compensation: about 437.3 Hz.
- Current is therefore roughly 16 cents flat against the reference's actual pitch, but the pitch
  issue is smaller than the timbre issue.
- Band read after RMS matching:
  - current is close in 0.2-0.8 kHz fundamental-region energy;
  - current is too strong around 0.8-1.2 kHz;
  - current is much too weak in 1.2-2 kHz, the reference's h3/h4/h5 body-color area;
  - current is still a few dB too hot around 6-12 kHz.

Tap attribution:

- `register_vent_flow` is strong internally, so the vent is affecting the bore topology.
- `register_vent_output` and `bell_radiated` are very low compared with `body_output`, so direct
  vent/bell radiation is not carrying the audible register character.
- The current output is too h1/low-band centered after the vented topology; the model needs better
  register-mode body/radiation admittance, not a simple vent-output gain boost.

### Register-vent phase A/B result

Implemented a small, physically framed register-vent phase/length correction:

- `ReedTubeParams::register_vent_phase_compensation`
- derived from register mode ratio, vent admittance, vent position, and a modal velocity-coupling
  term;
- folded into the same `delay_offset_samples` path as mouth-loss, damping, and reed-aperture phase;
- exposed to the catalog only as an internal A/B scalar.

Audition case added:

```text
tube_ref_register_key_high_vent_phase_off
```

Measured result:

```text
reference      peak 441.33 Hz  +5.2c vs 440
full phase on  peak 437.33 Hz -10.5c vs 440
phase off      peak 436.67 Hz -13.2c vs 440
no vent        peak 436.67 Hz -13.2c vs 440
```

Conclusion: the phase term is auditionable and slightly improves pitch, but only by about 2-3 cents.
It is **not** the source of the register-key flatness. Do not keep chasing pitch by increasing this
term unless a later impedance model shows the physical scale is wrong.

### Low-register pitch probe result

Added a dedicated audition group:

```text
20_tube_low_register_pitch/
make render-lamath-audio LAMATH_RENDER_ARGS="--group tube_low_register_pitch"
```

Cases:

```text
tube_low_register_pitch_d3_m050
tube_low_register_pitch_g3_m055
tube_low_register_pitch_c4_m060
tube_low_register_pitch_e4_m064
tube_low_register_pitch_g4_m067
tube_low_register_pitch_g_sharp4_m068
```

Measured over the steady window:

```text
D3   MIDI 50  target 146.83 Hz  measured 146.71 Hz   -1.4 cents
G3   MIDI 55  target 196.00 Hz  measured 195.99 Hz   -0.1 cents
C4   MIDI 60  target 261.63 Hz  measured 261.67 Hz   +0.3 cents
E4   MIDI 64  target 329.63 Hz  measured 329.50 Hz   -0.6 cents
G4   MIDI 67  target 392.00 Hz  measured 390.86 Hz   -5.0 cents
G#4  MIDI 68  target 415.30 Hz  measured 413.36 Hz   -8.1 cents
```

Conclusion: flatness is **not systemic between low E and the break**. It emerges near the top of the
pre-break range, then becomes more obvious in the register-key high case. That rules out a global
static delay-line correction as the right next move.

### Current next model target

Do **not** solve the next step with final EQ, a per-note cents table, or more broad delay tuning.

The next physical target is the near-break/register-mode impedance model:

1. Improve register-mode body/radiation admittance so the vented high note radiates useful h3/h4/h5
   body color instead of collapsing into a restrained h1-heavy tone. **✅ done 2026-06-09, see the
   register-body-admittance checkpoint below — awaiting audition.**
2. Keep the vent as a bore-topology shunt, but treat direct vent radiation as a secondary color path
   until tap levels prove otherwise.
3. Revisit pitch only after the register-mode impedance/body balance is closer; the low-register
   probe shows the pitch issue is local to the upper-low/register transition, not the entire tube.
4. Track the high-air band separately from pitch. The reference has reed/air character, but the
   Lamath air band is too exposed; tune the component balance, not a final high shelf.

## Implementation checkpoint (2026-06-09b) — register-aware body/radiation admittance

Implemented checkpoint item 1 as a register-aware body admittance model in `lindelion-wind`,
gated behind a new internal A/B scalar `register_body_admittance` (`1.0` = new model, `0.0` =
legacy chalumeau-style register body; patch field + `ReedTubeParams`, default `1.0`, bit-inert
below the break). New paired audition case in `19_tube_reference_match`:
`tube_ref_register_key_high_register_body_legacy` vs `tube_ref_register_key_high_current`, same
gain class, single variable.

**The decisive finding (from `--tube-tap-analysis` + WAV band reads): the vented bore's standing
wave carries no sounding h3.** With `register_mode_ratio = 2.88`, 3x the sounding note is not a
closed-open bore mode, so `body_input` h3 sits ~55 dB under its h1 — no radiation-side gain on the
pickup path can recover it. Worse, the final output's mid/upper harmonics ride on the plugin's
output-only `reed_radiated` layer, and the old final h3 was a *phase-cancellation valley* between
that layer and the body path (boosting the pickup-fed ring made h3 worse, not better). Two
failed in-loop attempts confirmed a second trap: the body's `reaction_flow` couples every
radiation gain back into per-band bore damping, so trimming h1 radiation un-damped the bore
fundamental and boosting the ring damped the very band it should radiate.

What shipped, per mechanism:

- **Source-fed register color (the actual h3 fix).** Above the break, the open tone-hole/vent
  lattice radiates the *reed source spectrum* (like a real clarion register above the lattice
  cutoff), not the bore's standing wave: `TubeBody` gains a radiation-only register color path
  fed by `mouth_wave` — a **cascaded 4th-order bandpass at 3f** (gain 9.4; the cascade is load-
  bearing — a single biquad's first-order skirts re-injected the source's broadband breath noise
  an octave either side, +9.6 dB at 6–12 kHz) plus a modest single bandpass at 4f (0.45).
- **Reaction pinned to legacy gains.** The fundamental/ring reactions onto the bore keep the
  legacy gains (`low/high_resonance_reaction_gain`), making the register rebalance radiation-only.
- **h4 even-notch released, h2 notch kept** (the vent breaks the even-cancelling symmetry, but
  0.8–1.2 kHz was already hot); **bore-period odd-mode projection comb bypassed** in register mode
  (at ratio 2.88 it is misaligned with the sounding period and attenuates h2–h5).
- **Fundamental body gain trimmed** (0.95 → 0.55 radiation-only), **pickup-fed ring boost**
  (scale 2.8 → 9.0; minor in practice — input-starved).
- **RMS-match gains un-saturated.** The reference-match `output_gain_db` values silently clamped
  at the +12 dB patch ceiling; the catalog now applies the residual as a render-time scale with a
  per-case full-scale cap, and the gain classes were recalibrated by measurement
  (`LowESustainPhysical` 24.0 → 15.9, `RegisterKeyHighSustainVented` 22.9 → 23.9). Current and
  low-E now match their references within 0.15 dB RMS.

Measured (steady window, RMS-matched, delta vs reference; legacy → current):

```text
band         legacy   current
800-1200     +9.0     +4.6
1200-2000   -14.1     -0.8
2000-4000    -2.4     -1.4
4000-6000    +8.8     +8.6   (pre-existing air band, item 4)
6000-12000   +5.1     +3.5
h3 line     -25.5    -12.5   (reference -12.8 — the defining clarinet harmonic, on target)
h2 line     -16.6    -22.6   (reference -24.5)
pitch       -11.3c    -7.9c  (known near-break flatness, item 3)
```

Remaining known gaps: the h4 *line* is still weak (-52.9 vs reference -19.0 — the reed source is
odd-dominant so h4 has no strong carrier; needs its own mechanism if ears ask), the 4–6 kHz air
band is untouched (item 4), and the ~-8c near-break flatness stands (item 3).

Guards: four new unit tests in `lindelion-wind` (h4 release, ring-up/fundamental-down recolor,
bit-inert below break, material A/B difference in register mode). Renders refreshed for the
affected register-key/articulation cases + pitch probes; manifest regenerated; review MP3s
compressed.

**Pre-existing red tests (NOT from this work — verified by disabling the new mechanism):**
`lindelion-wind::bell_radiation_does_not_depend_on_effort` (soft-effort bell ratio 2.25),
`lindelion-wind::driven_onsets_are_continuous_against_held_reference`,
`lamath-tube::scale_notes_are_tuned_well_enough_for_audition` (note 71 locks 737 Hz — the known
near-break mode-locking defect), `lamath-tube::phrase_onsets_do_not_click`. These came in with
the 2026-06-08 "Tune Lamath tube reed radiation" landing, which also left its
`lindelion-dsp-utils` `notch()` hunk uncommitted (HEAD does not build standalone without the
working tree). **Update 2026-06-10 (user direction): these tone guards are low-value while the
model is under audition-driven revision — they encode pre-audition behavior, not accuracy. All
four are now `#[ignore]`-parked with that reason; re-derive them against the audition-approved
model, do not "fix" them before then.**

## Audition outcome (2026-06-10) — register body admittance accepted direction; breath dominates

User audition of the 2026-06-09b register-body A/B: **low E sustain sounds good** (keep as the
approved lower-register baseline). The register-key high sustain is **much closer than it has
been**, but both the full sim and the vent-off comparison are **dominated by breath sounds with a
faint / weakly voiced actual note.**

### Diagnosis (objective, 2026-06-10)

Voiced-to-breath ratio over 200 Hz–8 kHz (steady window, harmonic-line energy vs inter-line
residual): reference **+44.6 dB**, ours **+21.4 dB** — the voiced lines actually *match* the
reference level; the residual was ~23 dB too hot. Attribution by kill-switch renders (band reads
and tap floors both misled — the T1 lesson again):

- The **reed's in-loop white breath dither (`REED_BREATH_NOISE = 0.02`) is the wash**: zeroing it
  alone took V/B to **+42.6** (≈ reference). The loop circulates it and every radiation path
  (pickup/body, bell, the new source-fed register color) re-emits it.
- The shed-jet turbulence (`REED_TURBULENCE_NOISE_GAIN`), the output-only register-vent
  turbulence (the 2026-06-05 "fill the 2–6 kHz floor" stopgap), and the slot radiation each
  contributed only a few dB.

### Fix — register-mode coherent radiation (2026-06-10, awaiting audition)

New internal A/B scalar `register_coherent_radiation` (patch + `ReedTubeParams` + `ReedParams.
breath_noise`, default 1.0 = coherent, 0.0 = legacy raw noise; **inert below the break — low E is
bit-identical**, verified by render hash). Above the break, with the handle at 1.0:

- the reed's in-loop dither is scaled to zero (its period-2-dodge job is a *low-register*
  full-bore concern; the vented register speaks cleanly without it — same level, same pitch);
- the reed slot radiation and the register body color radiate the reed's **coherent**
  (turbulence-free) source flow (`ReedDriver::coherent_source_flow/coherent_output`) instead of
  the noisy one;
- the output-only register-vent turbulence stopgap is silenced (it predates the register body
  color; deliberate spectrally-shaped breath is item 14's future mechanism).

Measured: register-key V/B **+21.4 → +42.6 dB** (reference +44.6). A/B comparator case:
`tube_ref_register_key_high_breath_raw` (raw radiation) vs `tube_ref_register_key_high_current`.
Affected register/articulation renders refreshed; manifest + review MP3s regenerated.

Remaining after this round: the per-band line-vs-residual above 2 kHz is still line-poor (the
sounding h4/h5/h7 lines are weak — needs its own carrier mechanism, same lattice-radiation story
as h3), pitch ~−8c near the break (item 3), and deliberate breath texture (item 14) once the
voiced core is approved.

## Audition outcome (2026-06-10b) — breath fix accepted; slow vented attack diagnosed and fixed

User: breath fix "sounds good enough." New complaint: **the vented register takes ~500–700 ms to
become full-bodied.** Measured: reference reaches 50% level in ~70 ms; ours took **1.3 s** —
and only when vented (the unvented same-pitch note and low E both reach 50% in ~60 ms). Not
intentional: the cause was the **frequency-flat resistive vent shunt** (Y = 1.2 → ~4 dB loss per
bore transit at *every* frequency, including the register mode it exists to enable), leaving a
sliver of oscillation margin → second-long exponential bloom.

### Fix — vent mode-choke (chimney anti-resonance), `register_vent_mode_choke`

A failed first attempt is itself a finding: a **one-pole inertive shunt** (admittance ∝ 1/ω, √2
DC boost) freed the mode but its −45° minimum-phase rotation at the bore fundamental weakened and
detuned f0's suppression — **the low register spoke** (harmonics of 186 Hz). Lesson: the bore
fundamental's hard, *resistive* (zero-phase) loading is what makes the reed abandon the low
register; no minimum-phase rolloff can free the mode without rotating f0.

The shipped design instead carves the shunt out in a **narrow band at the played mode only**
(bandpass-subtraction notch at the sounding frequency, Q 2 — physically the register chimney's
anti-resonance, where the hole's input impedance peaks and it effectively closes). Zero phase at
the notch center; f0 sees exactly the legacy resistive shunt. Internal A/B scalar
`register_vent_mode_choke` (1.0 = choked, 0.0 = legacy flat shunt), case
`tube_ref_register_key_high_vent_unchoked`.

Two closed-form consequences, both calibrated by measurement:

- **The legacy 2.88 mode ratio was the resistive drag.** With the mode unloaded the bore speaks
  its natural third mode (measured 2.994× bore f0), so `register_key_state` blends the ratio
  2.88 → 2.994 with the choke — and the register-key pitch landed at **440.00 Hz (+0.0 c)**: the
  near-break flatness (item 3) was the shunt drag, now gone. The legacy vent phase-compensation
  term is scaled out by the choke (the notch is zero-phase at the mode).
- **+12.8 dB more steady level** (real margin → stronger oscillation): the Vented RMS gain class
  recalibrated 23.9 → 11.1 dB (now far below the patch ceiling, no render-time residual needed).

Measured (register-key A4 case): onset to-50% **1300 ms → 200 ms** (reference 70 ms), f0
**−7.9 c → +0.0 c**, V/B **+49.8 dB** (reference +44.6), RMS-match within 0.6 dB. Low E
bit-identical (hash-verified; the choke only exists with the vent open). All vented-register and
articulation renders refreshed; manifest + review MP3s regenerated.

Remaining: onset 200 ms vs reference 70 ms (likely excitation kick / residual margins — judge by
ear whether it matters), the weak h4/h5/h7 line carriers, deliberate breath texture (item 14).

## Audition outcome (2026-06-10c) — attack accepted; reed character restored via lattice window

User: vented attack "pretty good"; **"the reed character is somewhat absent compared to the
reference."** That is the tracked weak h4–h7 line family (reference h4 −19 / h5 −26 / h7 −33
rel h1; ours were ~−52). With the coherent (noise-free) source now in place, the fix was the
planned lattice-radiation extension, made simple:

- **Lattice radiation window** in the register color path: the coherent register source through a
  **4th-order band window** (HP ×2 at 1,650 Hz, LP ×2 at 2,800 Hz, gain 2.9), replacing the
  near-inaudible single-biquad h4 resonance. The 4th-order edges are load-bearing again —
  2nd-order skirts at this gain leaked the source *fundamental* into the radiation (inflating h1
  itself) below and passed h9+ above.
- **Post-choke recalibration of the source-fed paths:** the vent mode-choke strengthened the
  oscillation, so the source got hotter and the pre-choke h3 cascade gain over-delivered by ~12 dB
  (h3 louder than h1). `TUBE_REGISTER_H3_SOURCE_GAIN` 9.4 → 2.0. Lesson: **source-fed radiation
  gains are calibrated against a specific oscillation operating point — re-measure them after any
  loop-margin change.** Vented RMS class 11.1 → 14.1 dB (match within 0.1 dB).

Measured line contour vs reference (rel h1): h3 −12.6 (ref −12.8 ✓), h4 −17.4 (−19.0 ✓), h5
−28.8 (−26.1, ~3 shy), h6 −40.6 (−35.5, ~5 shy), h8 −42.4 (−41.1 ✓); still off: **h2 −15.5
(ref −24.5, ~9 hot — even-harmonic leak through the bore/body, stronger since the choke)**, h7
−46.3 (−32.9, ~13 shy), h9 −42.6 (−52.7, ~10 hot). V/B +46.7 (ref +44.6). Low E bit-identical
(hash). All vented/articulation/probe renders, manifest, MP3s refreshed.

If ears ask for more reed character later, the next levers are per-line: the hot h2 (even leak),
the shy h7 (between the window's top edge and the bore's weak carriage), and the h9 tail.

## Pitch analysis (2026-06-10d) — both registers measured; range collapse found above C5

User direction: "let's work on pitch; the reference is a bit sharp, so unsure if our register is
on target." New probe group `21_tube_register_key_pitch` (sustained MIDI 69/70/71/72/74/76/79/84,
full vented model). All sustained measurements vs the A440 equal-tempered grid:

**The references are not uniformly sharp.** Chalumeau D3 fixture: **−1.4 c**; register-key A4
fixture: **+6.2 c**. The player's clarion runs sharp (normal for the instrument); the A440 grid
is the correct model target, and matching the clarion reference's absolute pitch would be
matching their intonation, not correctness.

**Ours, sustained:**

```text
chalumeau   D3 −1.5   G3 −0.1   C4 +0.4   E4 −0.6   G4 −5.1   G#4 −8.3
clarion     A4 −0.1   A#4 −2.8  B4 −0.8   C5 +2.9
clarion     D5/E5/G5/C6: DO NOT SPEAK (−60 dBFS noise — silent-config defect)
```

Findings, prioritized:

1. **Range collapse above C5 (the headline defect).** MIDI ≥ 74 with the vent open fails to
   oscillate (rms −60 to −65 dBFS, residual colored noise around 2.4–3.3 kHz). The unvented bore
   speaks these pitches (the C2–C6 sweep claim predates the register key), so this is the vented
   topology failing to sustain higher register modes. Violates the no-silent-configs rule. This
   is the next physical investigation: likely the loop gain at the mode through the 3x-longer
   bore (per-second loss scaling), the reed aperture band-limit at ~600+ Hz sounding, or the
   choke/vent interaction at higher mode numbers.
2. **Pre-break sag: G4 −5.1 c, G#4 −8.3 c** (B♭4 region, long unvented bore). Monotonic with
   pitch right below the break; the same fixed-loop-phase signature family as the old reed drag.
   Small but audible at sustained dynamics.
3. **B4 mode-lock is FIXED.** The parked scale test's note-71 → 737 Hz failure no longer exists
   sustained (B4 −0.8 c); the mode-choke repaired the mode selection.
4. **Onset pitch for short notes.** The parked scale test still fails at note 72, but for a new
   reason: it reads pitch 170 ms after note-on with 300 ms notes, inside the ~200 ms bloom (reads
   620 Hz mid-attack). Fast passages need lock well before 170 ms — same thread as the
   attack-speed item (reference blooms in 70 ms). User audition agrees: the fast low/high
   articulation case sounds good low and "like bird noises" high (chirping = notes shorter than
   the bloom never lock; no DC offset found — baseline wander < 0.001).
5. A#4 −2.8 c / C5 +2.9 c: minor wiggle across the vent's choke band, acceptable.

## Fix checkpoint (2026-06-10e) — register range collapse solved: tracked embouchure

The user's two-vent hypothesis was tested and ruled out for *this* model: in the silent D5
renders the vent was doing its job (bore fundamental decaying hard, vent geometry per-note ideal
since the bore scales with the note). The mode-3 band was getting kicked by the excitation and
**flatlining** — loop gain ≈ 1.0 − ε. The margin curve measured across the speaking notes
(growth 44.8/31.8/18.8/5.0 /s for A4/A#4/B4/C5) extrapolates to zero just above C5.

**Root cause: the inertial reed aperture's phase lag at the played mode rises with pitch and the
reed's energy pumping falls as cos(lag)** — at a fixed ~1.36 kHz aperture resonance the margin
crosses zero near C#5. Kill-switch confirmation: raising the aperture resonance alone made D5
speak. A real player firms the embouchure ascending the register; the model now does the same:

- **`ReedParams.tracking_frequency_hz` + `REED_APERTURE_TRACK_RATIO = 3.0`**: above the break the
  aperture resonance gets a floor of 3x the sounding pitch, pinning the lag (and the reed gain)
  at its A4 value up the register. At or below A4 the floor sits under the default resonance —
  the approved A4/low-E tone is bit-identical (hash-verified).
- **Tracked-lift phase trim** (`REED_TRACKED_PHASE_TRIM_*`): the comp's empirical coupling (1.25)
  was fitted at the base aperture; tracked notes played sharp on a smooth bump vs the lift ratio
  (peak +27 c near lift 1.45). Trimmed with a warm-bore-style fitted curve `A·y·e^(1−y)` in the
  reed comp; the fit nulled all speaking notes in one calibration (the predicted-vs-measured
  scale was exactly 2x off once — unit conversion — then exact).
- Ruled out along the way: choke-Q pitch attractor (Q 2→6 changed nothing), register body
  reaction (kill-switch, nothing).

Measured (sustained, vs A440 grid; previously D5+ were SILENT):

```text
A4 −0.1   A#4 +0.7   B4 −0.7   C5 −1.3   D5 −1.7   E5 −1.2   G5 −2.4   (blooms 165–230 ms)
C6 −38.5 (speaks, but concert C6 is altissimo above the written-C6 clarion top — out of scope)
```

The supported vented register is now **A4–G5 within ±2.4 cents at consistent levels**. Remaining
pitch work: the pre-break G4/G#4 sag (−5/−8 c, separate unvented-bore mechanism), onset lock
time for fast passages (the "bird noises" item — notes shorter than the ~200 ms bloom), and
altissimo tuning if concert C6+ ever matters.

## Fix checkpoint (2026-06-10f) — onset lock: tongue-release attack overpressure

The ~200 ms bloom (and the "bird noises" on fast high notes — notes shorter than the bloom never
lock) is a seed-and-margin problem: amplitude grows exponentially from the excitation's tiny
seed at a thin margin. Measured bloom-to-50% vs effective steady pressure (A4/D5): 0.58 (patch
default) → 180–205 ms, 0.68 → 70–75 ms, 0.75 → 50–60 ms, **0.85 → the reed chokes silent** (the
overpressure cliff). A real attack transiently overblows — the reed's gain is highest during the
build — so the processor now models the tongue release:

- On every vented-register note-on (including legato note changes, which must re-lock the new
  mode), an attack envelope drives the effective pressure toward `ATTACK_PRESSURE_TARGET = 0.75`,
  decaying to the steady patch pressure with τ = 70 ms.
- The boosted pressure is hard-ceilinged at `ATTACK_PRESSURE_MAX = 0.78` (humanize walks
  included) — well under the 0.85 choke cliff.
- Below the break the envelope never arms: the approved low register is untouched.

Measured: vented bloom-to-50% **180–205 → 65–75 ms** (reference 70 ms). Sustained pitch table
unchanged (A4–G5 within ±2.4 c). Fast low/high articulation: the high notes now lock within
their own 170–215 ms windows (back-half pitch +5/+7/+17/+6 c at −24.5 dBFS; previously 620 Hz
chirps that never arrived). Renders/manifest/MP3s refreshed. The parked scale test's onset
failure mode should also be re-checked when the tone guards are re-derived post-audition.

### Release discontinuity (2026-06-10g) — register fingering now persists through release

User audition (with waveform screenshot): an abrupt near-discontinuity at the END of the fast
high notes. Measured: high-note offsets stepped up to **Δ0.252 in one sample** (2x the note's own
peak; onsets were fine). Cause: `note_off` cleared `current_note`, so `register_key_state`
snapped to "no vent, ratio 1.0" — the ringing vented long bore re-tuned to the unvented short
bore in a single sample, truncating the release. Fix: a persistent `sounding_note` carries the
register fingering (vent topology, bore ratio, embouchure tracking) through the release;
note-off only stops the breath. Post-release tails now decay smoothly over ~20 ms (worst step
Δ0.014, equal to the approved low notes). All vented renders refreshed.

## How I'd prioritize (post-audition)

1. **A1 — resonant body / formant-shaped spectrum + bell/bore redesign (headline tone fix).** The
   voice is still an odd-harmonic square with digital highs; it needs bore-resonance/formant
   coloration and a resonant body so it reads warm. **As part of this, redesign the bell model and
   its contribution together with the bore** (energy-conserving open end: reflected low-pass +
   complementary radiated high-pass, unity gain; brightness generated at the source via the reed
   duty cycle, not a post-EQ tap). This is what makes item B's brightness *musical* rather than
   digital and gives the body a coherent signal to color (it has historically been overwhelmed when
   fed a hot square + non-conservative HF boost, partly with now-outdated params). Biggest
   perceptual payoff. (Leverages the existing `WaveguideBody` / body-coloration path.)
2. **F12 + item 14 — breath/onset texture (the agile tongue + real air).** Two coupled fixes to the
   reed excitation/breath layer: (a) replace the generic struck-instrument builtin impulse with a
   tuned breath/tongue chiff so tongued attacks are crisp and agile, not a "fat tongue"; (b) shape
   the sustained breath turbulence — currently raw white noise — into formant/bandpass-filtered wind
   air (or a recorded breath sample). Targeted, low DSP risk; together they replace the synthetic
   "hiss + thud" with believable air.
3. **Item 13 — humanization (breath/embouchure micro-variation).** Backlogged by the user, but it is
   the other half of the "digital" feel: small slow quasi-random modulation of mouth pressure +
   embouchure into the reed equation so sustained notes breathe. Do after A1/F12.
4. **Robustness / ranges (standing methodology):** D5 (bell dead range) and E7 (period-doubling)
   — derive the oscillation-threshold boundary in closed form and make the control ranges honest.
5. **Polish:** C4 (top-octave flatness), E9 (replace the magic `0.13` gain constant with a
   level-targeted trim), F10/F11 (UI-lock mono+reed, bound articulation control).
