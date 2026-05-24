# DSP Module Documentation

Per-module reference docs for DSP types in the Lindelion workspace. Each doc follows the nine-section template in `.claude/skills/lindelion-dsp-docs/references/skeleton.md`.

| Module | Source | Purpose |
| ---- | ---- | ---- |
| [OnePoleLowpass](onepolelowpass.md) | `crates/lindelion-dsp-utils/src/filters.rs` | Single-pole IIR low-pass |
| [Biquad](biquad.md) | `crates/lindelion-dsp-utils/src/filters.rs` | Direct-Form I biquad, RBJ-cookbook coefficients |
| [Adsr](adsr.md) | `crates/lindelion-dsp-utils/src/envelope.rs` | Linear-step ADSR envelope state machine |
| [ModalBank](modal-bank.md) | `plugins/lamath/src/dsp/modal.rs` | Bank of second-order resonant filters per vibrational mode |

Plot generation infrastructure (Rust test → CSV → matplotlib → SVG) is not yet wired up. Each module doc names the expected response plots in its §5 with a "Pending" status. See [`../backlog.md`](../backlog.md) for the workspace item.
