# lindelion-fidelity

Shared audio-fidelity test harness: a general-signal battery any `lindelion-effect`
implementation can run through, built on the metrics in `lindelion-dsp-utils::analysis`.

The general battery is use-case-neutral and applies to any processor: finite/no-NaN output,
no clicks, denormal handling, bypass-equals-identity, latency-report accuracy, allocation-free
processing, and frequency-response sanity. Per-effect-class objective tests (compressor
gain-reduction curves, EQ magnitude response, de-esser sibilance reduction, denoiser SNR
improvement, and so on) layer on top, in each effect crate.

This harness is distinct from the pitch-shift fidelity battery in `lindelion-pitch-shift`,
which keeps its pitch-specific metrics (f0 error in cents, formant preservation, pre-echo).

Scope and milestones: [HOTMIC-PORT-PLAN.md](../../HOTMIC-PORT-PLAN.md).
