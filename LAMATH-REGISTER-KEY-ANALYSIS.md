# Lamath Tube Register-Key Analysis

Scope: sustained register-key high reference match only.

Files:

- `review/lamath-render-catalog/19_tube_reference_match/tube_ref_register_key_high_reference.wav`
- `review/lamath-render-catalog/19_tube_reference_match/tube_ref_register_key_high_current.wav`
- diagnostic comparator: `review/lamath-render-catalog/19_tube_reference_match/tube_ref_register_key_high_no_register_vent.wav`

Render state:

```text
make render-lamath-audio LAMATH_RENDER_ARGS="--tag register-key"
```

Rendered 9 WAVs. Highest peak was `tube_ref_register_key_high_humanize_100` at -6.67 dBFS; quietest audible was `tube_ref_register_key_high_no_register_vent` at -36.64 dBFS.

## Measurement

Window: steady sustain, 2.565s-4.065s. Target/scheduled pitch is concert A4, 440 Hz. The reference fixture is slightly sharp; the current vented model is slightly flat.

```text
signal          rms    peak   f0_ac  f0_comb cents_vs_440  dom_hz centroid roll95 rms_sd f0_sd
reference       -18.9  -13.4  441.6   439.9      +6.3      441.3      523   1325  0.31  0.10
current_vented  -19.8  -16.3  437.9   436.3      -8.3      438.0      439    439  0.03  0.05
no_vent         -36.5  -32.4  437.2   435.6     -11.1      437.3      492    438  0.02  0.06
```

Pitch read:

- Current is about 8 cents flat against 440 Hz.
- Current is about 15 cents flat against the reference's actual steady pitch.
- The perceived flatness is real but not the dominant timbre problem.

## Harmonic Ratios

Ratios are local-peak dB relative to each file's own h1 near 440 Hz.

```text
signal          h1    h2     h3     h4     h5     h7     h9    h11    h13    h15    h17    h19
reference       0.0  -25.1  -11.5  -17.4  -24.0  -30.2  -48.0  -43.8  -47.5  -65.6  -62.4  -61.0
current_vented  0.0  -36.1  -39.3  -57.1  -38.9  -46.7  -85.9  -55.1  -53.4  -71.8  -76.9  -66.1
no_vent         0.0  -19.3  -18.9  -22.0  -28.5  -27.9  -35.5  -36.9  -40.6  -33.4  -39.5  -53.4
```

Current vented error vs reference:

```text
h2  -11.1 dB
h3  -27.8 dB
h4  -39.7 dB
h5  -15.0 dB
h7  -16.5 dB
h9  -37.9 dB
h11 -11.3 dB
h13  -5.9 dB
h15  -6.2 dB
h17 -14.5 dB
h19  -5.1 dB
```

Absolute harmonic dBFS:

```text
signal          h1     h2     h3     h4     h5     h7     h9    h11    h13    h15    h17    h19
reference      -17.2  -42.2  -28.6  -34.6  -41.2  -47.4  -65.2  -61.0  -64.7  -82.8  -79.5  -78.2
current_vented -16.9  -53.1  -56.2  -74.0  -55.9  -63.7 -102.8  -72.1  -70.4  -88.8  -93.8  -83.0
no_vent        -33.9  -53.2  -52.8  -55.9  -62.4  -61.7  -69.4  -70.8  -74.5  -67.3  -73.4  -87.3
```

Interpretation:

1. The current vented render is not primarily too quiet in RMS. Its h1 is almost exactly the reference's h1 level.
2. The reference has strong register-body color above the fundamental, especially h3/h4/h5.
3. Current vented is close to a sine-like A4 with heavily suppressed h3/h4/h5/h7/h9. This matches the subjective "restrained/stuffed" read.
4. The no-vent render is not valid as output because it is much too quiet, but its harmonic ratios are much richer. The vented model is suppressing or failing to re-radiate upper register modes.

## Broadband And Floors

Band powers are dB relative to each file's total RMS.

```text
signal          0-0.8k  0.8-1.2k  1.2-2k  2-4k  4-8k  8-12k
reference        -26.3     -48.7    -37.3  -53.4  -72.4  -84.7
current_vented   -26.0     -58.6    -63.7  -67.2  -78.1  -91.0
no_vent          -26.2     -41.9    -42.8  -53.1  -58.3  -72.0
```

Inter-harmonic floor dBFS:

```text
signal          0.8-1.2k  1.2-2k  2-4k  4-8k
reference          -91.1   -84.6  -75.4  -91.0
current_vented     -97.6   -94.1  -86.8  -97.8
no_vent           -101.6   -80.6  -89.4  -94.7
```

Interpretation:

- Current is missing about 10 dB in the 0.8-1.2 kHz band, about 26 dB in the 1.2-2 kHz band, and about 14 dB in 2-4 kHz, relative to total RMS.
- The reference's 1.2-2 kHz region is the largest missing perceptual component. That region includes h3/h4/h5 for A4.
- The current inter-harmonic floor is also 9-11 dB below reference in 1.2-4 kHz, so it is missing both harmonic peaks and some breath/reed/body roughness.

## Envelope

20 ms RMS threshold read from onset at 0.08s:

```text
signal          pre_floor sustain  t10   t50   t90   t99   crest  AM_sd
reference        -72.9    -18.8   0.118 0.150 1.048 1.422  5.5dB  0.31dB
current_vented  -300.0    -19.8   0.180 0.310 0.380 0.414  3.5dB  0.03dB
no_vent         -300.0    -36.5   0.038 0.050 0.060 0.066  4.1dB  0.02dB
```

Interpretation:

- Reference swells for roughly a full second after onset; current reaches steady state in about 0.4s.
- Current has much lower crest factor and almost no amplitude motion in the steady window.
- This contributes to the "restrained" impression: the output is very controlled and static after a short rise, while the real clarinet keeps changing.

## Internal Tap Read

The built-in tap analysis uses exact 440 Hz DFT bins, so harmonic-ratio columns are misleading when the model is several cents flat. Its RMS attribution is still useful:

```text
current vented tap       RMS dBFS
body_output               -31.2
bell_radiated             -55.5
register_vent_output      -56.1
body_bell_sum             -31.3
tube_final_output         -31.7
final_output              -19.8
```

Interpretation:

1. The audible current render is body-dominated.
2. Bell radiation and register-vent radiation are about 24 dB below body output and are not materially adding the missing high-register character.
3. The physical tube output before final gain is around -31.7 dBFS, then the render gain brings it to -19.8 dBFS. This explains the "blowing hard but not much sound comes out" impression: the modeled acoustic output is weak, then gain-matched.
4. The current register vent is acting mainly as a bore shunt/topology perturbation, not as a significant radiating source.

## Physical Diagnosis

Most likely causes, in order:

1. Register-mode body admittance is wrong. In register mode, `TubeBody` makes the sounding fundamental at 440 Hz the primary body resonance. That over-centers the output on h1 and leaves the reference's strong h3/h4/h5 region under-radiated.
2. Register vent damping is too lossy relative to its useful modal coupling. The vented version becomes much louder than no-vent after gain, but its harmonic structure is more suppressed. The no-vent diagnostic has richer ratios, which suggests the vent/topology model is collapsing upper-mode radiation rather than opening it.
3. Register vent radiation output is too quiet to matter. If the register key should add reed/vent edge content, the current output-only vent path is at -56 dBFS and cannot contribute audible color.
4. The model lacks register-specific body/tail support. The low-E upper odd tail is intentionally disabled for register mode, but high register still needs a formant/radiation model around h3/h4/h5 of the sounding note.

## Next Model Direction

Do not solve this with final EQ.

The next physical change should be register-specific body/radiation admittance:

- Reduce the register-mode body's over-reliance on the 440 Hz primary body resonance.
- Add/register-enable body formant support around sounding h3/h4/h5, roughly 1.3-2.2 kHz for this A4 case.
- Rebalance vent radiation so it can contribute audible edge/noise if that is physically intended, rather than being 24 dB below body output.
- Correct pitch after timbre with the register-mode ratio/phase path; current is about 15 cents flat vs reference.
