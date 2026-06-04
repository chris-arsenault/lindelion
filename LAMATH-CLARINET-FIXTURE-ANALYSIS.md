# Lamath Clarinet Fixture Analysis

Source: repo-root `clarinet.wav`, uploaded by the user.

Purpose: preserve real clarinet phrases and sustained notes as fixtures for harmonic and time-domain
analysis. These are not pitch-center snippets: attacks, releases, breath/reed variation, phrase timing,
and register changes are kept intact.

## Extracted Fixtures

All derived fixtures are mono 44.1 kHz / 16-bit PCM in `testdata/audio/`, downmixed from the source
with 10 ms edge fades and no peak normalization.

| File | Source window | Content |
| --- | ---: | --- |
| `owner_clarinet_written_c_major_scale.wav` | 2.05-15.65 s | written C-major scale phrase |
| `owner_clarinet_mid_register_phrase.wav` | 15.85-19.60 s | mid-register phrase |
| `owner_clarinet_low_register_phrase.wav` | 19.30-22.90 s | low-register phrase |
| `owner_clarinet_low_e_sustain.wav` | 27.60-34.10 s | low E sustain, full bore |
| `owner_clarinet_register_key_high_sustain.wav` | 34.60-41.25 s | same fingering with register key |
| `owner_clarinet_low_high_articulation.wav` | 41.60-48.50 s | repeated articulation: 8 low, 8 high, 8 low, 8 high |
| `owner_clarinet_quick_altissimo_phrase.wav` | 48.30-55.40 s | quick upper-register and altissimo phrase |
| `owner_clarinet_altissimo_sustain.wav` | 55.55-59.20 s | altissimo sustain with embouchure down-bend tail |

## Method

Analysis used 120 ms Hann frames with 40 ms hops. Each voiced frame gets a YIN-style f0 estimate,
then harmonic magnitudes are sampled at `h * f0` for h1-h13. Harmonic levels are median dB relative
to h1 across all voiced frames, so phrases contribute their moving pitch content instead of being
collapsed to one steady FFT center. Pitch estimates are concert pitch; B-flat clarinet written pitch
is roughly a whole step higher.

`upper odd tail` means the average of h7, h9, h11, and h13 relative to h1.

## Per-Fixture Summary

| File | dur | median f0/note (concert) | pitch span p10-p90 | env p5-p95 | centroid | h3/h1 | upper odd tail | even/odd |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `owner_clarinet_written_c_major_scale.wav` | 13.60 s | 344.0 Hz F4 | 12.01 st | 17.1 dB | 874 Hz | -8.7 dB | -32.5 dB | -11.4 dB |
| `owner_clarinet_mid_register_phrase.wav` | 3.75 s | 442.3 Hz A4 | 1.08 st | 6.3 dB | 633 Hz | -8.1 dB | -50.0 dB | -14.1 dB |
| `owner_clarinet_low_register_phrase.wav` | 3.60 s | 230.9 Hz A#3 | 0.91 st | 10.9 dB | 627 Hz | -6.8 dB | -25.4 dB | -15.6 dB |
| `owner_clarinet_low_e_sustain.wav` | 6.50 s | 146.9 Hz D3 | 0.19 st | 7.8 dB | 281 Hz | -4.8 dB | -24.3 dB | -15.5 dB |
| `owner_clarinet_register_key_high_sustain.wav` | 6.65 s | 441.7 Hz A4 | 0.13 st | 4.8 dB | 524 Hz | -11.8 dB | -43.5 dB | -18.6 dB |
| `owner_clarinet_low_high_articulation.wav` | 6.90 s | 148.8 Hz D3 | 19.15 st | 10.9 dB | 327 Hz | -10.4 dB | -40.6 dB | -20.7 dB |
| `owner_clarinet_quick_altissimo_phrase.wav` | 7.10 s | 439.0 Hz A4 | 27.05 st | 22.0 dB | 582 Hz | -6.4 dB | -40.4 dB | -16.9 dB |
| `owner_clarinet_altissimo_sustain.wav` | 3.65 s | 991.5 Hz B5 | 0.16 st | 3.3 dB | 1993 Hz | +0.7 dB | -40.8 dB | +11.1 dB |

## Aggregate Harmonic Shape

Across all voiced frames, real clarinet energy is odd-favored but not square-wave-like. The third
harmonic is a meaningful body/register feature, while the upper odd tail falls quickly.

| harmonic | median rel h1 | p10 | p90 |
| ---: | ---: | ---: | ---: |
| h1 | 0.0 dB | 0.0 | 0.0 |
| h2 | -27.9 dB | -37.1 | -13.3 |
| h3 | -8.2 dB | -15.9 | -3.6 |
| h4 | -23.2 dB | -31.1 | -14.0 |
| h5 | -18.0 dB | -30.6 | -1.2 |
| h6 | -25.9 dB | -41.3 | -7.1 |
| h7 | -27.9 dB | -44.4 | -7.9 |
| h8 | -35.0 dB | -48.6 | -15.8 |
| h9 | -38.9 dB | -50.7 | -17.3 |
| h10 | -42.7 dB | -55.7 | -24.0 |
| h11 | -44.2 dB | -60.8 | -26.4 |
| h12 | -46.5 dB | -64.3 | -29.2 |
| h13 | -47.8 dB | -68.1 | -29.0 |

## Modeling Takeaways

1. The clarinet target is not "more h3 plus a square tail." The aggregate h3 is strong-ish at
   about -8 dB relative to h1, but h9-h13 are typically down roughly 39-48 dB. If Lamath Tube keeps
   an audible 1/n odd tail, the fix needs to reduce the sustained upper odd harmonics, not only add a
   body formant.

2. The low E/register-key pair is a high-value physical reference. The low E sustain (concert D3)
   has h3 at -4.8 dB and h5 at -10.9 dB, with the upper odd tail around -24 dB. The register-key
   high sustain (concert A4) has h3 at -11.8 dB, h5 at -25.5 dB, and the upper odd tail around
   -43.5 dB. That is not a simple pitch-scaled bore: the register-key topology materially changes
   which modes survive.

3. Sustained notes are time-varying targets. The low E has about 0.19 semitones p10-p90 pitch
   movement and 7.8 dB p5-p95 envelope movement across the full held note. The register-key high
   sustain is steadier in pitch but still has about 4.8 dB envelope movement. A static steady-state
   oscillator comparison will miss part of the clarinet sound.

4. Altissimo is a separate regime. The altissimo sustain has h2 above h1 and h3 near h1, with
   even/odd energy positive. It should not be used as the nominal Tube tone target, but it is useful
   for later register-key and nonlinear reed/bore checks.

## Low/High Articulation Detail

`owner_clarinet_low_high_articulation.wav` is not four held alternations. The f0 tracker sees four
pitch regions, but the onset envelope shows 32 attacks grouped as 8 low, 8 high, 8 low, 8 high.
The matched Tube render in `19_tube_reference_match` should therefore use repeated note-on events,
not four sustained MIDI notes.

Detected attack times, relative to the fixture:

- low D3: 0.150, 0.389, 0.594, 0.793, 1.018, 1.217, 1.447, 1.646 s
- high A4: 1.886, 2.080, 2.290, 2.529, 2.724, 2.928, 3.148, 3.357 s
- low D3: 3.582, 3.771, 3.991, 4.181, 4.400, 4.590, 4.824, 5.014 s
- high A4: 5.248, 5.438, 5.652, 5.822, 6.036, 6.256, 6.480, 6.670 s

## Altissimo Sustain Bend Detail

The one-row altissimo summary hides the intended mouth/embouchure bend because most voiced frames are
on the sustained plateau. A focused time-series pass over
`owner_clarinet_altissimo_sustain.wav` shows:

| region | relative time | f0 behavior | envelope |
| --- | ---: | --- | --- |
| attack/settle | 0.00-0.50 s | median 996.3 Hz, 990.6-998.3 Hz p10-p90 | -31.2 to -27.5 dB RMS |
| main sustain | 0.50-2.55 s | median 994.4 Hz, 990.2-997.4 Hz p10-p90 | -28.8 to -27.6 dB RMS |
| down-bend tail | 2.55-3.60 s | median 989.3 Hz, 954.5-990.2 Hz p10-p90 | -29.3 to -27.0 dB RMS before release |

Concert B5 is 987.77 Hz. The main sustain is therefore slightly sharp, about +12 cents. Near the
audible tail, the tracked f0 reaches about 939 Hz at source time 58.97 s, approximately -98 cents
relative to the main sustain and -87 cents relative to equal-tempered B5. After source time about
59.05 s, the envelope is below roughly -49 dB RMS and f0 estimates become release/noise artifacts
or subharmonic jumps; those should not be treated as stable pitch.

Modeling implication: this fixture should be used as two related targets, not one median pitch target:
first altissimo sustain stability, then an embouchure/mouth-pressure bend of roughly one semitone at
the tail without changing fingering.
