# DSP Module Documentation

Per-module reference docs for DSP types in the Lindelion workspace. Each doc follows the nine-section template in `.claude/skills/lindelion-dsp-docs/references/skeleton.md`.

| Module | Source | Purpose |
| ---- | ---- | ---- |
| [OnePoleLowpass](onepolelowpass.md) | `crates/lindelion-dsp-utils/src/filters.rs` | Single-pole IIR low-pass |
| [Biquad](biquad.md) | `crates/lindelion-dsp-utils/src/filters.rs` | Direct-Form I biquad, RBJ-cookbook coefficients |
| [Svf](svf.md) | `crates/lindelion-dsp-utils/src/filters.rs` | Zavalishin TPT state-variable filter (LP/BP/HP) |
| [DelayLine](delay-line.md) | `crates/lindelion-dsp-utils/src/delay.rs` | Fractional-delay ring buffer with interpolated read and fractional-tap injection |
| [FirstOrderAllpass](allpass.md) | `crates/lindelion-dsp-utils/src/delay.rs` | Unity-magnitude fractional-sample delay |
| [Smoothing](smoothing.md) | `crates/lindelion-dsp-utils/src/smoothing.rs` | Linear parameter ramp plus spec-driven sanitized wrapper |
| [Adsr](adsr.md) | `crates/lindelion-dsp-utils/src/envelope.rs` | Linear-step ADSR envelope state machine |
| [ModalBank](modal-bank.md) | `plugins/lamath/src/dsp/modal.rs` | Bank of second-order resonant filters per vibrational mode |
| [WaveguideResonator](waveguide.md) | `plugins/lamath/src/dsp/waveguide.rs` | Karplus-Strong-style digital waveguide with string/tube boundaries |
| [Onset detection](onset-detect.md) | `crates/lindelion-onset-detect/` | SuperFlux / energy / pitch-stability / manual-grid onset detectors, batch + streaming |
| [Pitch detection](pitch-detect.md) | `crates/lindelion-pitch-detect/` | SwiftF0 ONNX-backed pitch tracker plus zero-crossing fallback |
| [Phrase analysis](phrase-analysis.md) | `crates/lindelion-phrase-analysis/` | Pipeline orchestrator: pitch → onsets → note segmentation |
| [Resample Pro pitch shifting](resample-pro.md) | `crates/lindelion-pitch-shift/` + `plugins/linnod/src/runtime/prepared_resample_pro.rs` | Phase-aware stretch-plus-resample pitch shifting with setup-time Linnod slice preparation |
| [Ableton Complex Pro reference](ableton-complex-pro.md) | Public Ableton/zplane sources + Lindelion comparison | Clean-room reference for Complex Pro-style pitch and time-stretch behavior |

Plot generation is wired through Rust integration tests in `crates/lindelion-dsp-utils/tests/plot_data.rs` and a unit test in `plugins/lamath/src/dsp/modal.rs`. The tests emit deterministic CSVs under `docs/plots/data/`; `make docs` reads those CSVs and renders SVGs under `docs/plots/`. See [`../../tools/dsp-plot/README.md`](../../tools/dsp-plot/README.md) for the pipeline details and Python setup.
