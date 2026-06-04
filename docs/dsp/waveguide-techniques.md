# Waveguide Resonator Techniques

Catalog of the digital-waveguide techniques behind the extracted string and wind resonator crates
(`crates/lindelion-string/src/`, `crates/lindelion-wind/src/`) and the extracted idiophone mesh crate
(`crates/lindelion-idiophone/src/`). The module overview is in
[waveguide.md](waveguide.md); the tuning-correction and 2D-mesh decision rationale is in
[ADR-0011](../adr/0011-waveguide-tube-tuning-and-2d-mesh.md).

Tuning and stability are validated in the extracted crates: `string_model_pitch_tracks_target_matrix`
in `lindelion-string`, `tuning_tracks_low_mid_register` plus the register stability sweep in
`lindelion-wind`, and mesh finite/bounded/audibility sweeps in `lindelion-idiophone` and
`lamath-cymbal`. Pitch is read with a periodicity-faithful estimator where the response is
harmonically rich and body-coloured.

## Tuning and loop structure

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| Half-wave string tuning | `crates/lindelion-string/src/model.rs` and `traveling.rs` (`cycle_divisor = 2.0`) | Both terminations invert (fixed ends), so the string is a half-wave resonator: a full round trip is one period of the played pitch. |
| Stiffness dispersion | `dispersion.rs` (`WaveguideDispersion`) | Cascaded first-order all-passes add string-stiffness inharmonicity, with group-delay compensation so the fundamental still tracks `frequency_hz`. String only. |
| Quarter-wave tube tuning | `crates/lindelion-wind/src/tube.rs` and `traveling.rs` (`cycle_divisor = 4.0`) | The bore's terminations are asymmetric - inverting mouth and non-inverting end - which makes it a quarter-wave resonator: the round trip is half a period of the played pitch. |
| Boundary-filter phase-delay compensation | `core.rs` (`filter_phase_delay_samples`) | Loop resonance is set by the phase a wave accumulates per round trip, so each boundary lowpass's phase delay at the played pitch (not its group delay, which drifts as the pitch nears the cutoff) is divided out of the delay length. The tube compensates both of its boundary filters; the string compensates its single loop filter. |
| Cubic (4-point Lagrange) fractional-delay read | `crates/lindelion-dsp-utils/src/interpolation.rs` (`cubic_wrapped`), via `DelayLine::read` | Keeps the fractional-delay group delay flat into the passband, so short loops stay in tune at high frequencies; String tuning is within ~1.5 cents across the whole range. Shared by every `DelayLine` user. |

## Damping

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| Frequency-dependent T60(f) loop damping | `core.rs` (`loop_damping`) | The loop gain is derived so the fundamental decays in the requested T60, dividing out the loop filter's own attenuation there; the filter roll-off then gives higher partials an explicit, calibrated, measurably faster decay. The worst-case round-trip magnitude is held below unity for stability. |

## 2D mesh resonator (Mesh model)

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| Rectangular 2D waveguide mesh | `crates/lindelion-idiophone/src/mesh.rs` (`RectangularMesh2d`) | A fixed grid of scattering junctions with per-edge fixed/free terminations - a struck two-dimensional surface (plate/membrane). Lossless scattering is passive; boundary damping sets decay. |
| Allocation-free in-place re-tuning | `crates/lindelion-idiophone/src/runtime.rs` (`MeshResonator`) | The grid is fixed, so the resonator allocates its mesh once and every later `configure` recomputes strike/pickup weights in place. Lamath Cymbal owns the MIDI strike/damp policy around this persistent body. |

## Tuning measurement

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| DFT magnitude-peak estimator | `crates/lindelion-dsp-utils/src/analysis/pitch.rs` (`estimate_f0_dft_peak`) | Sub-cent, wide-range fundamental estimator (windowed-DFT magnitude scan + parabolic interpolation) that can bracket an octave of error; trustworthy on clean tones, used to validate the string. |
| Periodicity-faithful estimator | `analysis/pitch.rs` (`estimate_f0_autocorrelation_refined`) | Sub-sample parabolic-interpolated autocorrelation. Tracks waveform periodicity rather than the strongest spectral lobe, so it stays sub-cent on the bore's harmonically rich, body-coloured strike response where a magnitude-peak scan is pulled by the spectral envelope. Drives the steady-state tuning gate. |

## Known limits

- **Tube top-octave tuning.** The quarter-wave bore's loops are half the string's length, so at the
  top of the range the round trip is only a few samples (≈ 5.5 samples at 4 kHz / 44.1 kHz), where a
  sub-sample interpolation residual reaches tens of cents. The steady-state gate asserts String
  < 3 cents across the full range and Tube < 3 cents through its tunable range; `lindelion-wind`'s matrix
  and full-range tests cover the taper and the finite/bounded/decaying guarantee across the whole
  30 Hz–4 kHz span.
- **Mesh pitch.** The mesh is inharmonic; "pitch" is its lowest mode steered to the note, so it
  tracks the keyboard as the surface's lowest resonance.
