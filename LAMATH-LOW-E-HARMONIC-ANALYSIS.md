# Lamath Tube Low-E Harmonic Analysis

Current scope: low-E clarinet reference match only. Do not use this note for the register-key/high-register problem; that needs a different bore/register model framing.

## Measurement

Window: steady low-E sustain, approximately 2.480s-3.980s in:

- `review/lamath-render-catalog/19_tube_reference_match/tube_ref_low_e_sustain_reference.wav`
- `review/lamath-render-catalog/19_tube_reference_match/tube_ref_low_e_sustain_current.wav`
- related output-path renders in the same directory

Method: local peak search around each harmonic of the measured low-E fundamental. Ratios below are dB relative to each file's own h1.

```text
harmonic   ref ratio   current ratio   current error
h2          -38.7 dB     -25.1 dB       +13.6 dB too loud
h3           -5.7 dB      -4.6 dB        +1.0 dB close
h4          -28.2 dB     -17.6 dB       +10.6 dB too loud
h5          -12.5 dB     -10.3 dB        +2.3 dB slightly loud
h7          -21.1 dB     -17.6 dB        +3.5 dB slightly loud
h9          -23.0 dB     -31.8 dB        -8.8 dB too quiet
h11         -29.9 dB     -38.1 dB        -8.2 dB too quiet
h13         -31.3 dB     -42.7 dB       -11.4 dB too quiet
h15-h25      close        within ~-4..+1 dB
```

## Component Attribution

Path deltas against the reference ratio:

```text
error       body_only       dry_pickup      dry+bell
h2          +14.4 dB        +15.3 dB        +15.3 dB
h4          +10.3 dB         -1.0 dB         -1.4 dB
h9           -9.0 dB        -20.5 dB        -18.2 dB
h11         -17.4 dB        -25.0 dB        -22.2 dB
h13         -25.5 dB        -30.0 dB        -27.4 dB
```

Interpretation:

1. h2 is a source/bore even-mode leakage problem. It is too high in every path, including dry pickup. The body path does not reject it.
2. h4 is mainly a body projection problem. Dry pickup h4 is near the reference, but body/current h4 are about +10 dB too strong.
3. h3/h5/h7 are already close. Do not increase the existing h3/h5/h7 formants as the next move.
4. h9/h11/h13 are the missing clarinet tail. The body-only path is too dark above h7, and bell radiation helps but starts too late/too weak to fill the h9-h13 region.
5. This matches the implementation: `TubeBody` currently has tracked resonances for h3, h5, and h7, but no h9/h11/h13 body modes. The body lowpass and bell highpass leave the transition region under-supported.

## Physical Modeling Target

The low-E fix should not be a post contour or a broad bell boost.

Target the model in two places:

1. Strengthen/correct the odd-mode body projection so even modes, especially h2 and h4, are rejected harder. The current half-period odd projection still leaves too much direct/even leakage.
2. Extend body/bore radiation support for upper odd modes around h9/h11/h13. For low E these are roughly 1.32 kHz, 1.62 kHz, and 1.91 kHz. This should be body/bore radiation support, not more h3/h5/h7.

## Next Audition Set

Render low-E-only variants:

1. Stronger/corrected odd-mode projection only.
2. h9/h11/h13 upper odd body-mode extension only.
3. Combined odd projection plus h9/h11/h13 extension.

Evaluate with:

- reference/current harmonic ratio table
- path/tap attribution table
- listening through the audition tool

## Fix Pass 1: Body Even-Mode Rejection

Implemented in the physical body path, not as a final/post contour:

- `TubeBody` now applies h2/h4 anti-resonance filters before its direct and resonance branches.
- The existing corrected closed-open odd projection remains enabled, with the legacy projection retained as an audition case.
- The render tap analysis now includes h4 because h4 is one of the active defects.

Rendered with:

```text
make render-lamath-audio LAMATH_RENDER_ARGS="--case tube_ref_low_e_sustain_current"
make render-lamath-audio LAMATH_RENDER_ARGS="--case tube_ref_low_e_sustain_body_only"
make render-lamath-audio LAMATH_RENDER_ARGS="--case tube_ref_low_e_sustain_legacy_odd_projection"
make render-lamath-audio LAMATH_RENDER_ARGS="--tube-tap-analysis tube_ref_low_e_sustain_current"
```

Steady-window local-peak ratios after the change:

```text
harmonic   ref ratio   current ratio   current error
h2          -39.8 dB     -43.0 dB        -3.3 dB slightly low
h3           -5.7 dB      -4.1 dB        +1.6 dB close
h4          -27.9 dB     -31.3 dB        -3.5 dB slightly low
h5          -12.5 dB     -12.2 dB        +0.2 dB close
h7          -21.1 dB     -19.2 dB        +1.9 dB close
h9          -22.8 dB     -32.4 dB        -9.6 dB too quiet
h11         -29.6 dB     -37.5 dB        -7.9 dB too quiet
h13         -31.3 dB     -41.9 dB       -10.5 dB too quiet
```

Tap attribution after the change:

```text
tap                 h2 ratio   h4 ratio
body_input           -29.2 dB   -32.1 dB
body_output          -62.7 dB   -55.5 dB
bell_radiated         -8.5 dB    +2.2 dB
final_output         -44.0 dB   -33.0 dB
```

Interpretation:

1. The body branch is no longer leaking h2/h4; the even-mode correction worked there.
2. Final h2/h4 are now a few dB under the reference, which is close enough to stop this pass rather than retune a body notch by ear.
3. The remaining low-E spectral mismatch is now the second item: h9/h11/h13 are still about 8-10 dB too quiet.

## Fix Pass 2: Upper Odd Body/Radiation Tail

Implemented in the physical body path, not as a post contour:

- `TubeBody` now includes a low-register upper odd mode bank at h9/h11/h13.
- The bank is controlled by `body_upper_odd_modes`; default/current is enabled.
- The temporary A/B audition case `tube_ref_low_e_sustain_upper_tail_disabled` was removed after this pass; the catalog now keeps only low-E reference/current.
- The bank is disabled for register-mode notes until the register-key bore topology is modeled separately.

Rendered with:

```text
make render-lamath-audio LAMATH_RENDER_ARGS="--tag low-e"
```

Steady-window local-peak ratios after the upper-tail pass:

```text
harmonic   ref ratio   tail-off ratio   current ratio   current error
h2          -39.8 dB     -43.0 dB        -47.3 dB        -7.5 dB low
h3           -5.7 dB      -4.1 dB         -6.0 dB        -0.3 dB close
h4          -27.9 dB     -31.3 dB        -35.9 dB        -8.0 dB low
h5          -12.5 dB     -12.2 dB        -17.1 dB        -4.6 dB low
h7          -21.1 dB     -19.2 dB        -20.3 dB        +0.8 dB close
h9          -22.8 dB     -32.4 dB        -22.1 dB        +0.7 dB close
h11         -29.6 dB     -37.5 dB        -31.2 dB        -1.6 dB close
h13         -31.3 dB     -41.9 dB        -36.2 dB        -4.9 dB low
h15         -40.3 dB     -44.2 dB        -49.6 dB        -9.4 dB low
h17         -43.4 dB     -46.3 dB        -49.2 dB        -5.7 dB low
h19         -45.1 dB     -48.5 dB        -50.1 dB        -5.0 dB low
```

Interpretation:

1. The upper-tail bank fixed the core defect: h9 moved by +10.3 dB, h11 by +6.3 dB, and h13 by +5.7 dB relative to the tail-disabled A/B.
2. h9/h11 now match the reference closely; h13 remains several dB low but is close enough to audition before making the tail bank more aggressive.
3. The tradeoff is that h2/h4/h5 ratios dropped because the added body-tail energy changes the whole harmonic balance. That may be acceptable by ear, but it should be auditioned rather than tuned from the table alone.

## Catalog Cleanup

The low-E reference-match catalog now keeps only:

```text
tube_ref_low_e_sustain_reference
tube_ref_low_e_sustain_current
```

Removed stale low-E audition cases:

```text
tube_ref_low_e_sustain_upper_tail_disabled
tube_ref_low_e_sustain_legacy_odd_projection
tube_ref_low_e_sustain_legacy_contour
tube_ref_low_e_sustain_body_only
tube_ref_low_e_sustain_bell_dry
tube_ref_low_e_sustain_dry
tube_ref_low_e_sustain_humanize_050
tube_ref_low_e_sustain_humanize_100
```
