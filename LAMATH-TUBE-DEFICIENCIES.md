# Lamath Tube (driven wind voice + reed driver) — current deficiencies

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
