# DSP plot tooling

Renders SVG response plots for DSP module docs in `docs/dsp/` from CSV data emitted by Rust integration tests.

## Pipeline

1. Rust integration tests in `crates/lindelion-dsp-utils/tests/plot_data.rs` and `plugins/lamath/tests/plot_data.rs` emit deterministic CSVs under `docs/plots/data/`.
2. `make ci` runs those tests as part of the workspace test pass and fails on `git diff --exit-code docs/plots/data/`.
3. `make docs` (this directory) reads the CSVs and renders SVGs under `docs/plots/`.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

Or, with `uv`:

```bash
uv venv && source .venv/bin/activate
uv pip install -r requirements.txt
```

## Scripts

| Script | Input | Output | Notes |
| ---- | ---- | ---- | ---- |
| `plot_freqz.py` | CSV with `freq_hz` column + one or more magnitude/phase columns | SVG | Log frequency axis. Multi-column CSVs render overlaid curves with a legend. |
| `plot_pz.py` | CSV with `b0, b1, b2, a1, a2` columns (one row per filter; optional `name` column for labels) | SVG | Plots zeros (○) and poles (×) on the unit circle. Uses `scipy.signal.tf2zpk`. |
| `plot_time.py` | CSV with `time_s` (or `sample`) column + one or more value columns | SVG | Linear or log-y. Multi-column CSVs render overlaid curves. |

## Invocation

```bash
python3 plot_freqz.py docs/plots/data/onepolelowpass_freqz.csv docs/plots/onepolelowpass_mag.svg
python3 plot_pz.py     docs/plots/data/biquad_ba.csv             docs/plots/biquad_pz.svg
python3 plot_time.py   docs/plots/data/adsr_step.csv              docs/plots/adsr_step.svg --title "ADSR step response"
```

## Conventions

- CSVs commit under `docs/plots/data/`; SVGs commit under `docs/plots/`.
- All numbers rounded to six decimals at emission to avoid platform-float churn.
- Tests use a fixed `SAMPLE_RATE = 48_000.0` Hz and seeded RNG when randomness applies.
- One CSV per artifact. Multi-curve plots use multi-column CSVs, not multiple files.

## Reproducibility

The Rust tests run as part of `cargo test --workspace`. The CSVs they emit are committed and treated as golden files: `make ci` fails if they drift. SVGs are regenerated on demand via `make docs`.
