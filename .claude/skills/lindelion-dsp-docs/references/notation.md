# DSP notation conventions

Use these conventions in every DSP module doc.

## Signals

- `x[n]` for input, `y[n]` for output — square brackets, discrete time.
- `x(t)`, `y(t)` for continuous-time analog prototypes (rare; only when discussing s-domain).

## Time and sample rate

- `fs` (or `f_s`) for sample rate.
- `T = 1/fs` for sample period.
- `fs/2` for Nyquist.

## Frequency

Prefer Hz with explicit `fs`. If normalizing, name the convention:
- `rad/sample [0, π]` — Smith / Oppenheim default.
- `cycles/sample [0, 0.5]` — Lyons / scipy default.
- MATLAB-style `[0, 1]` where `1 ≡ Nyquist`.

Mixing these without naming is the most common DSP-doc bug.

## Transfer functions

- `H(z)` discrete-time transfer function.
- `H(s)` analog prototype.
- `H(e^{jω})` frequency response.

## Coefficients

- `b_k` feedforward, `a_k` feedback. `a0 = 1` after normalization.
- Sign convention: `y[n] = b0·x[n] + … − a1·y[n-1] − …` (Smith / scipy).

## Levels

- dB for ratios: `20·log10` for amplitude, `10·log10` for power.
- dBFS for absolute level, with `±1.0 = 0 dBFS` by convention. Floating-point audio has no hard 0 dBFS clip.

## Units in parameter tables

Always include units. Acceptable: `Hz`, `dB`, `ms`, `seconds`, `octaves`, `0..1`, `cents`. Never bare numbers.

## Q, bandwidth, slope

- `Q` is dimensionless.
- Bandwidth in octaves or Hz — state which.
- Shelf slope `S` per the RBJ Cookbook.

## In Rust code

- Unicode is fine in comments: `ω`, `π`, `∑`, `ω₀`, `ζ`, `∞`, `≤`, `≥`.
- LaTeX `$…$` does NOT render in rustdoc. Put math in `docs/`, not in `///` docstrings.
- Variable names in code use ASCII: `omega`, `q`, `alpha`, `coefficient`, `frequency_hz`. Math in surrounding doc uses Unicode or LaTeX.
