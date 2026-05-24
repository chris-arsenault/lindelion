# Plot generation workflow

Plot data lives inside the Rust test suite so it cannot drift from the implementation. SVG rendering happens in a separate `make docs` target so `make ci` stays fast and Python-free for normal contributors.

## Pipeline

1. **Rust test emits CSV.** Add a test in the same crate that runs the DSP under a deterministic stimulus (impulse, log sweep, unit step, seeded sine) and writes the result to `docs/plots/data/<module>_<plot>.csv`. Use `StdRng::seed_from_u64(...)` for any randomness — never `OsRng`, never wall-clock.
2. **Round values.** Format with `{:.6}` so platform floating-point variance does not churn the file.
3. **Commit the CSV.** It is both the plot's source of truth and a golden snapshot. Diffs in PR review reveal accidental DSP drift.
4. **`make ci`** runs the export tests and `git diff --exit-code docs/plots/data/`. Drift fails the build with no Python involved.
5. **`make docs`** (opt-in target) regenerates SVGs from CSVs via `python3 tools/dsp-plot/plot_*.py`. Pin matplotlib/scipy/librosa in `tools/dsp-plot/requirements.txt`. Run locally or on a docs-publish job, not on the hot CI path.

## Recipes

### Frequency response

Rust test runs an impulse through the module, FFTs the response, writes CSV `freq_hz,mag_db,phase_deg`.

```rust
#[test]
fn export_freqz_<module>() {
    let sr = 48_000.0;
    let mut f = Module::new(sr, /* params */);
    let n_fft = 4096;
    let mut impulse = vec![0.0; n_fft];
    impulse[0] = 1.0;
    for s in impulse.iter_mut() {
        *s = f.process(*s);
    }
    let spectrum = fft(&impulse);
    let mut w = std::fs::File::create(
        "../../docs/plots/data/<module>_freqz.csv"
    ).unwrap();
    writeln!(w, "freq_hz,mag_db,phase_deg").unwrap();
    for (k, c) in spectrum.iter().enumerate().take(n_fft / 2) {
        let f = k as f32 * sr / n_fft as f32;
        let mag_db = 20.0 * c.norm().log10();
        let phase_deg = c.arg().to_degrees();
        writeln!(w, "{:.6},{:.6},{:.6}", f, mag_db, phase_deg).unwrap();
    }
}
```

Python renders with `semilogx` on a shared x-axis for magnitude and phase.

### Pole-zero

Rust test exports coefficients as `b,a` CSV. Python:

```python
import numpy as np, matplotlib.pyplot as plt
from scipy.signal import tf2zpk
b, a = np.loadtxt("docs/plots/data/<module>_ba.csv", delimiter=",", unpack=True)
z, p, _ = tf2zpk(b, a)
ax = plt.subplot(111, aspect="equal")
t = np.linspace(0, 2 * np.pi, 512)
ax.plot(np.cos(t), np.sin(t), "k--", lw=0.5)
ax.scatter(z.real, z.imag, marker="o", facecolors="none", edgecolors="b")
ax.scatter(p.real, p.imag, marker="x", color="r")
plt.savefig("docs/plots/<module>_pz.svg")
```

### Impulse / step response

Rust runs `δ[n]` or `u[n]` through the module, writes CSV `time_s,value`. Python plots; use `set_yscale("log")` with a `np.abs(y) + 1e-12` floor for decay tails.

### Spectrogram

Rust synthesizes a seeded test signal (or runs the analyzer pipeline on a synthetic input). Python uses `librosa.display.specshow` on `librosa.stft` for log-y; fall back to `matplotlib.pyplot.specgram` for quick-look.

## Recommended directory layout

```
docs/
  diagrams/
    biquad-direct-form-2-transposed.svg
    modal-bank-topology.svg
  plots/
    onepolelowpass_mag.svg
    onepolelowpass_impulse.svg
    data/
      onepolelowpass_freqz.csv
      onepolelowpass_impulse.csv
tools/
  dsp-plot/
    requirements.txt
    plot_freqz.py
    plot_pz.py
    plot_impulse.py
    plot_spectrogram.py
```

## Quick-sketch escape hatch

If the doc author needs a design sketch before any Rust code exists, `scipy.signal.freqz(b, a)` from coefficients alone is acceptable. The canonical plot that ships in the doc must come from the Rust path.
