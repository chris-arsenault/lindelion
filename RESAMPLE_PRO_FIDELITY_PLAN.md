# Resample Pro Fidelity Plan

Experiment program to raise the fidelity of the **Resample Pro** pitch-shift engine
toward élastique-class quality (Ableton Complex Pro / Kontakt Time Machine Pro).
Synthesized from the in-repo code + docs (`docs/dsp/resample-pro.md`,
`docs/dsp/ableton-complex-pro.md`, quality contract) and academic/industry/forum
research (Laroche–Dolson, Průša & Holighaus, Röbel, Rubber Band, Signalsmith,
zplane/élastique).

**How to use this file:** each section below is a numbered **milestone (M0–M8)**.
To instruct work, reference the milestone number (e.g. "do M1", "start M2"). M0 is a
prerequisite for everything; M1–M3 are the high-value core; M4–M5 are comparative /
cheap; M6–M8 are deferred-tier. Each milestone is independently testable and gated
on the M0 battery with **no existing quality-contract threshold allowed to regress**.

---

## Diagnosis — where the gap is

Resample Pro is essentially the Laroche–Dolson 1999 phase vocoder: STFT (sqrt-Hann,
75 % overlap) → identity peak phase-locking (`resample_pro_stretch.rs`
`propagate_phase_locked_frame`) → whole-frame phase reset on transients + a raw
"direct transient" splice (`resample_pro_render.rs apply_direct_transient_layer`) →
per-bin spectral-envelope formant gain (`spectral.rs spectral_envelope`, currently
**moving-RMS box smoothing**) → windowed-sinc resample. The fidelity gap is
concentrated in three places (aliasing and stereo are **not** problems — the
resampler is clean and Linnod is mono):

| Symptom | Root cause in current code | Reference fix |
| --- | --- | --- |
| Phasiness / "not clean" on tonal parts | Identity peak-locking can't represent broadband/chirp phase; discards the frequency-direction phase gradient `∆_fφ`. | RTPGHI (M2) or Signalsmith multi-prediction (M5b). |
| Transient softening | Whole-frame phase reset damages sustained content under the window; raw splice is seam-prone. | Bin-level group-delay (COG) transient handling (M3) or HPSS dual-path (M5a). |
| Dullness / loss of clarity | Formant envelope is moving-RMS box smoothing → follows spectrum mean, mislocates formants. | True-Envelope estimator (M1). |

**Phase-5 lesson that constrains M1:** the spectral envelope is shared by the
spectral-peak path *and* `resample_pro` (`resample_pro_stretch.rs:414`). A naive
cepstral envelope was more accurate in isolation but its sharpness caused a
downstream conflict (resample_pro pure-tone residual −38 dB vs the −45 dB contract;
spectral-peak dominance dropping below 4×). **Any envelope change must be measured
against the downstream synthesis residual, not just envelope accuracy.**

---

## M0 — Measurement harness + baseline (PREREQUISITE)

Build the objective fidelity battery that every later milestone is measured against.
Fidelity is partly perceptual, so this is objective proxies **plus** rendered WAVs for
A/B listening. Most metrics already exist in `lindelion-dsp-utils::analysis`.

- **Fixtures:** pure sine; harmonic stack; harmonic stack + vibrato; impulse/click
  train; noise burst; vowel-like formant signal; existing real sax & wind fixtures.
  Shifts: ±1 cent, ±2 / ±5 / ±7 / ±12 semitones; formant-preserve and formant-track.
- **Metrics (one number each, logged to a baseline table):**
  - Tonal phasiness → `inter_peak_floor_ratio` + `fitted_sine_rms_error` on shifted partials.
  - Transient → **pre-echo energy ratio** (RMS before onset ÷ onset peak) + **transient peak preservation** (output crest factor ÷ source).
  - Aliasing → `high_frequency_artifact_ratio` / folded-band rejection (regression guard only).
  - Formant → envelope peak Hz vs known formant + `spectral_centroid_hz` drift.
  - Pitch accuracy → `estimate_f0_autocorrelation`; roughness → `zero_crossing_period_jitter`.
  - **Round-trip reconstruction** (best single fidelity proxy): shift +N then −N semitones, compare to original via `gain_fitted_rms_difference`.
- **Deliverables:** a committed baseline numbers table + WAV dumps per fixture; the
  existing quality-contract thresholds become hard regression gates.
- **Cost:** low–moderate. **Risk:** low. Scaffold behind existing `resample_pro` test
  conventions (`db_to_gain` thresholds, fixture helpers); does not touch the realtime path.

---

## M1 — True-Envelope formant estimator (experiment E2)

*Cheapest big win; attacks "dullness/clarity"; re-tests the Phase-5 conflict directly.*

- **Hypothesis:** a peak-riding true envelope locates formants accurately and improves
  clarity **without** the pure-tone residual that the naive cepstral envelope caused.
- **Change (`spectral.rs`):** iterative true-envelope (Röbel & Rodet) —
  `A_i = max(A_{i-1}, cepstral_smooth(A_{i-1}))` to convergence, **Hamming lifter**
  (not rectangular — kills the Gibbs ringing behind the Phase-5 residual), cepstral
  order from f0, sigmoid "don't pre-warp below the fundamental". Bounded iteration
  budget; offline-cache path.
- **Tests:** envelope peak within tight tolerance of known formants + stability across
  shifts; **Phase-5 gate** — resample_pro pure-tone residual ≤ −45 dB AND spectral-peak
  dominance ≥ 4× (this is the experiment's crux).
- **Cost:** moderate (optimized TE is 2.5–11× faster than naive, realtime-proven).
  **Risk:** moderate (the downstream conflict may recur — that's what the gate measures).
- **Refs:** Röbel & Rodet, DAFx-05.

---

## M2 — RTPGHI phase propagation (experiment E1)

*The core phasiness fix. Biggest single quality lever.*

- **Hypothesis:** integrating the full 2-D phase gradient restores broadband + vertical
  coherence that peak-locking structurally cannot, lowering phasiness on tonal material.
- **Change (`resample_pro_stretch.rs`):** add `∆_fφ` (centered finite difference across
  bins; +1 frame look-ahead) and replace `propagate_phase_locked_frame` with
  magnitude-ordered heap integration (RTPGHI). Pre-allocate fixed-capacity heap + index
  buffer (ADR-0001).
- **Tests:** `inter_peak_floor_ratio` ↓ and tonal `fitted_sine_rms_error` ↓; round-trip
  error ↓; guards: pitch accuracy, alias rejection, transient pre-echo no regression.
- **Cost:** moderate–high (allocation-free heap; +1 hop latency). **Risk:** moderate.
- **Refs:** Průša & Holighaus "Phase Vocoder Done Right" (arXiv:2202.07382);
  Průša & Søndergaard RTPGHI (DAFx-16). Rated competitive-to-better than élastique
  Pro / Melodyne in published listening tests.

---

## M3 — Bin-level COG transient handling (experiment E3)

*Transient sharpness without damaging surrounding tonal content. Depends on M2's `∆_fφ`.*

- **Hypothesis:** reinitializing phase only for bins "in front of" an attack sharpens
  onsets while leaving stationary bins coherent, and lets the fragile raw splice retire.
- **Change (`resample_pro_stretch.rs` + `resample_pro_render.rs`):** reuse `∆_fφ` from M2
  as a per-bin center-of-gravity / local group delay; reinit phase for past-threshold
  bins only; keep reposition-to-mapped-output; de-emphasize/remove
  `apply_direct_transient_layer`.
- **Tests:** pre-echo energy ↓, transient peak preservation ↑, tonal residual around
  onsets stable (no level dip); guard: no double-attacks / chirp on vibrato.
- **Cost:** moderate (shares `∆_fφ` with M2). **Risk:** moderate (threshold tuning).
- **Refs:** Röbel ICMC-03 / DAFx-03; Duxbury AES-112. `[depends on M2]`

---

## M4 — Overlap / window-size sweep (experiment E5)

*Cheap A/B knob study.*

- **Hypothesis:** 87.5 % overlap (hop = FFT/8) marginally lowers modulation sidebands /
  phasiness; larger FFT helps bass tonal but hurts transients.
- **Change:** parameterize hop / FFT size; sweep across the M0 battery; pick the sweet spot.
- **Tests:** full M0 battery across settings.
- **Cost:** low. **Risk:** low. Expect marginal (75 % already at −51 dB sidebands).
- **Refs:** Laroche–Dolson 1999.

---

## M5 — Transient/coherence bake-off (experiments E4 vs E6)

*Run only if M1–M3 don't fully close the gap on drums/mixes. Two alternatives, compared.*

- **M5a — HPSS dual-path** (`resample_pro_*`): median-filter harmonic/percussive split,
  PV the harmonic + short-frame OLA the percussive, sum. Robust transients with no
  detector. Refs: Driedger/Müller/Ewert IEEE SPL 2014; FitzGerald DAFx-10.
- **M5b — Signalsmith weighted multi-prediction phase** (alternative to M2): blend a
  horizontal (PV) phase prediction with vertical predictions at bin offsets {1,2,4,8,16},
  amplitude-weighted — emergent transient/tonal handling, no detector. Refs: Signalsmith
  Stretch design write-up (readable MIT C++).
- **Tests:** transient fixtures (pre-echo, peak) + mixed-material round-trip; head-to-head
  against M3 / M2 results.
- **Cost:** moderate–high (architectural). **Risk:** moderate.

---

## M6 — Noise morphing (experiment E7, deferred)

Re-excite the stochastic component's magnitude with fresh phase instead of stretching
noise phases (fixes "metallic" noise under PV). Pairs with M5a. Tested on the noise /
breath fixtures. Refs: Moliner/Rämö/Välimäki IEEE SPL 2024.

## M7 — Multiresolution / dual-window (experiment E8, deferred)

Long window at low frequencies, short at highs (élastique "bass resolution",
Rubber Band R3). High cost; tested by improving bass-tonal residual *and* HF transient
sharpness simultaneously. Refs: Liuni & Röbel ICASSP 2017.

## M8 — (reserved) — only if a measured gap remains after M1–M7

Candidates kept on the bench, **explicitly not** recommended now: scaled phase locking
(same ceiling as identity, adds region-flicker artifacts — moot after M2); full SMS
partial-tracking and neural/DDSP (fragile on polyphonic / not realtime-portable).

---

## Recommended order

`M0 → M1 → M2 → M3 → re-measure → (M4 quick sweep) → M5 bake-off if needed → M6/M7 if a gap remains.`

M1+M2+M3 are the disclosed élastique/Complex-Pro "secret sauce" (formant preservation +
phase coherence + transient separation), are all documented and portable, and compose
cleanly (M2 and M3 share `∆_fφ`; M1 is independent). This is a closeable gap.

## Key references

- Laroche & Dolson, "Improved phase vocoder TSM of audio," IEEE TSAP 7(3), 1999.
- Průša & Holighaus, "Phase Vocoder Done Right," EUSIPCO 2017 (arXiv:2202.07382);
  Průša & Søndergaard, RTPGHI, DAFx-16.
- Röbel, "Transient detection and preservation in the phase vocoder," ICMC-03 / DAFx-03.
- Röbel & Rodet, "Efficient spectral envelope estimation … pitch shifting," DAFx-05.
- Driedger, Müller & Ewert, "Improving TSM … harmonic-percussive separation," IEEE SPL 21(1), 2014.
- Rubber Band Library technical notes (breakfastquay.com); Signalsmith Stretch design
  write-up (signalsmith-audio.co.uk); zplane élastique technology page.
