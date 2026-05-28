# Pitch-Shift Regression Signature

This note preserves a local waveform-analysis finding for diagnosing pitch-shift
quality regressions. It is meant as durable DSP context, not as a product
specification or a golden-sample fixture.

## Analyzed Render

The source render was a 44.1 kHz, 32-bit integer, stereo file around 4.054
seconds long. The left and right channels were bit-identical, so the content was
effectively mono in a stereo container.

Basic measurements:

- Peak: almost full scale, about -0.0001 dBFS.
- RMS: about -11.78 dBFS.
- Crest factor: about 11.79 dB.
- Clipping indicator: 24 samples at or above 0.99.
- DC offset: about -0.0128.

The content was a repeating short phrase with roughly a 400 ms cycle, ten cycles
across the render. Each cycle had a sharp transient onset and sustained decay.
Within each cycle, the spectral peak walked through a chromatic cluster around
Bb4, C5, and Db5, roughly 466 Hz, 523 Hz, and 554 Hz. Occasional autocorrelation
dips near 232 Hz were interpreted as subharmonic detection errors at note
boundaries, not as real pitch drops.

The spectrum below about 4 kHz was a dense harmonic/intermodulation stack. Strong
content appeared near 466, 524, 554, 1394, 1577, 1846, 2078, and 2320 Hz. This is
consistent with a saturated or distorted chord cluster.

## A/B Signature

The render had a clear A/B split at the 2.000 second midpoint: five cycles before
the midpoint and five after it. The two halves kept the same fundamental peak
structure within roughly 10 to 15 cents, but their noise and high-frequency
behavior diverged.

| Metric | Half A, 0-2 s | Half B, 2-4 s |
| ---- | ----: | ----: |
| RMS | 0.269 | 0.244 |
| Crest factor | 11.5 dB | 12.3 dB |
| 85% spectral rolloff | 3768 Hz | 3704 Hz |
| 95% spectral rolloff | 5249 Hz | 4177 Hz |
| Inter-peak floor, about 100-400 Hz | about -80 dB | about -65 dB |

The tonal peaks landed in the same places, but Half B lifted the valley floor
between harmonic peaks by about 10 to 15 dB and lost some extreme high-frequency
rolloff. That points at a quality regression rather than a hard structural bug:
pitch locations were preserved, but energy leaked from tonal peaks into broadband
hash between them.

There were no obvious sample-to-sample glitches and no strong alias-energy
signature near Nyquist.

## Likely Failure Mode

The signature matches loss of phase coherence in a phase-vocoder-style shifter,
especially on dense saturated material.

Horizontal coherence means each bin's phase in frame `n + 1` follows from the
true estimated frequency and the previous synthesized phase, not from wrapped
frame-local phase alone. If this is broken, a sinusoid that should be a clean
peak turns into a peak plus decorrelated leakage around it.

Vertical coherence means the bins around a detected partial stay phase-locked to
the peak bin. This is the peak-locking idea associated with Laroche/Dolson-style
phase vocoders. Without peak locking, simple single-sine material can still look
acceptable, while dense harmonic or saturated material falls apart because many
close-spaced partials compete.

The analyzed material is exactly the kind of input that exposes this: partials
and intermodulation products are packed closely enough that weak phase-locking
spreads energy into the gaps. The 95% rolloff drop from about 5.2 kHz to 4.2 kHz
is consistent with weaker high-frequency partials losing concentration first.

## Most Probable Causes

Most likely causes, in descending order:

1. Peak-locking or partial tracking disabled, bypassed, or thresholded out.
2. Phase propagation regression, such as using wrapped phase instead of true
   frequency, or tracking hop count in the wrong units.
3. Lower overlap factor, for example moving from 8x to 4x or from 4x to 2x.
4. Window-function downgrade, such as moving from a low-sidelobe window to Hann.
5. Smaller FFT size causing coarser frequency resolution and more inter-bin
   leakage on close partials.

The slightly higher crest factor in the bad half points more toward phase
coherence or peak-locking loss than toward a pure transient-smearing failure.
Coherence loss can create random inter-peak noise spikes without moving the main
tonal peaks.

## Diagnostic Tests

Use controlled renders to separate the failure modes:

1. Single sustained sine at about 500 Hz through the shifter.
   - A coherent shifter should produce one clean shifted peak and a very low
     floor, roughly -80 to -100 dB for this kind of diagnostic.
   - Broken horizontal coherence produces a visible skirt or pedestal around the
     shifted peak.
2. Two close sustained sines, for example 500 Hz and 540 Hz.
   - If the single-sine case passes but this fails, suspect vertical coherence or
     peak-locking.
3. Dense harmonic stack or saturated chord material.
   - If simple tones pass but dense material lifts the inter-peak floor, suspect
     overlap, window, FFT-size, or peak-tracking thresholds.

The core acceptance signal is that tonal peaks remain concentrated and the
inter-peak floor does not rise by the 10 to 15 dB pattern seen in the regressed
half of the local render.
