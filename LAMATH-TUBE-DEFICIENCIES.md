# Lamath Tube (driven wind voice + reed driver) — current deficiencies

Assessment from ADR-0032, the Lamath spec, the backlog, the reed/bore code
(`tube_1d.rs`, `driver.rs`, `makeup.rs`), the wind-path wiring in
`resonator_stack.rs`, the full-synth guards in `tube_voice_tests.rs`, and memory of
this thread.

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

4. **Top octave is a few cents flat** (ADR + spec). The scale guard tolerance is 60
   cents (a full quartertone), which *masks* this rather than fixing it. Likely a
   phase-delay/fractional-delay compensation residual near the high register
   (`delay_tuning` in `prepared_model`).

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
