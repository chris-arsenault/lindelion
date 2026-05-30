//! Shared general-signal fidelity battery for [`Effect`](lindelion_effect::Effect) implementations.
//!
//! These checks apply to any processor regardless of what it does: finite output, no clicks,
//! denormal robustness, bypass-equals-identity, latency-report accuracy, frequency-response
//! sanity, and (as a separate panic-style assertion) allocation-free processing. They are the
//! use-case-neutral counterpart to the pitch-shift-specific battery in `lindelion-pitch-shift`;
//! per-effect-class tests layer on top in each effect crate.

use std::fmt;

use lindelion_dsp_utils::analysis::{max_adjacent_delta, peak_abs, windowed_dft_magnitude_at};
use lindelion_effect::Effect;

const SAMPLE_BLOCK: usize = 4096;
const PROBE_HZ: f32 = 220.0;

/// A failed fidelity check: which check, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryFailure {
    /// Stable name of the check that failed.
    pub check: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl BatteryFailure {
    fn new(check: &'static str, detail: impl Into<String>) -> Self {
        Self {
            check,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for BatteryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fidelity check '{}' failed: {}", self.check, self.detail)
    }
}

/// Which general-battery checks to run. Defaults to all of them.
#[derive(Debug, Clone, Copy)]
pub struct BatteryOptions {
    /// Run the impulse-based latency-report check. Disable for neural-network effects: their
    /// warm-up transients produce output before the declared latency, so this linear-delay
    /// heuristic does not apply. Such effects validate latency instead by signal-alignment
    /// (best-lag) tests. See [ADR-0014](../../../docs/adr/0014-nn-inference-allocation.md).
    pub check_latency: bool,
}

impl Default for BatteryOptions {
    fn default() -> Self {
        Self {
            check_latency: true,
        }
    }
}

/// Run the general-signal battery against `effect` at `sample_rate` with all checks. Returns the
/// first failed check or `Ok(())`. The allocation-free check is the separate
/// [`assert_allocation_free`] assertion, because it panics on allocation rather than returning a
/// value.
pub fn run_general_battery(
    effect: &mut dyn Effect,
    sample_rate: f32,
) -> Result<(), BatteryFailure> {
    run_general_battery_with(effect, sample_rate, BatteryOptions::default())
}

/// Run the general-signal battery with explicit [`BatteryOptions`] (e.g. to scope out the
/// latency check for neural-network effects).
pub fn run_general_battery_with(
    effect: &mut dyn Effect,
    sample_rate: f32,
    options: BatteryOptions,
) -> Result<(), BatteryFailure> {
    effect.prepare(sample_rate, SAMPLE_BLOCK);
    let sine = probe_sine(sample_rate);

    // Finite / no-NaN.
    effect.reset();
    effect.set_bypassed(false);
    let mut out = sine.clone();
    effect.process(&mut out);
    if out.iter().any(|s| !s.is_finite()) {
        return Err(BatteryFailure::new(
            "finite",
            "output contains non-finite samples",
        ));
    }

    // No clicks: a band-limited tone has small sample-to-sample deltas.
    let out_peak = peak_abs(&out).max(f32::MIN_POSITIVE);
    let delta = max_adjacent_delta(&out);
    if delta > 0.2 * out_peak {
        return Err(BatteryFailure::new(
            "no-clicks",
            format!("max adjacent delta {delta:.4} exceeds 0.2 * peak {out_peak:.4}"),
        ));
    }

    // Frequency-response sanity: finite spectrum at the probe and no runaway broadband gain.
    let in_peak = peak_abs(&sine).max(f32::MIN_POSITIVE);
    let mag = windowed_dft_magnitude_at(&out, sample_rate, PROBE_HZ);
    if !mag.is_finite() {
        return Err(BatteryFailure::new(
            "frequency-response",
            "probe magnitude is non-finite",
        ));
    }
    if out_peak > in_peak * 32.0 {
        return Err(BatteryFailure::new(
            "frequency-response",
            format!("output peak {out_peak:.3} exceeds 32x input peak {in_peak:.3}"),
        ));
    }

    // Denormal robustness: denormal-magnitude input stays finite and bounded.
    effect.reset();
    let mut denorm = vec![1.0e-39_f32; SAMPLE_BLOCK];
    effect.process(&mut denorm);
    if denorm.iter().any(|s| !s.is_finite()) {
        return Err(BatteryFailure::new(
            "denormal",
            "denormal input produced non-finite output",
        ));
    }
    if peak_abs(&denorm) > 1.0 {
        return Err(BatteryFailure::new(
            "denormal",
            "denormal input produced large output",
        ));
    }

    // Bypass == identity.
    effect.reset();
    effect.set_bypassed(true);
    let mut bypassed = sine.clone();
    effect.process(&mut bypassed);
    if bypassed != sine {
        return Err(BatteryFailure::new(
            "bypass-identity",
            "bypassed output differs from input",
        ));
    }
    effect.set_bypassed(false);

    // Latency-report accuracy: the effect must not respond before its declared latency (so
    // host PDC is correct), and must produce some output. This tolerates latency-smearing
    // effects (e.g. STFT overlap-add, whose impulse response ramps up via the window) while
    // still catching an effect that responds earlier than it claims.
    if options.check_latency {
        effect.reset();
        let mut impulse = vec![0.0_f32; SAMPLE_BLOCK];
        impulse[0] = 1.0;
        effect.process(&mut impulse);
        let latency = effect.latency_samples().min(impulse.len());
        let response_peak = peak_abs(&impulse);
        if response_peak < 1.0e-6 {
            return Err(BatteryFailure::new("latency", "impulse produced no output"));
        }
        let pre_latency_peak = peak_abs(&impulse[..latency]);
        if pre_latency_peak > 0.05 * response_peak {
            return Err(BatteryFailure::new(
                "latency",
                format!(
                    "output {pre_latency_peak:.4} before declared latency {latency} (peak {response_peak:.4})"
                ),
            ));
        }
    }

    Ok(())
}

/// Assert `effect.process` is allocation-free on a pre-sized block. Panics if it allocates. This
/// is the bar for ordinary DSP effects. Effective only when the running test binary installs
/// `lindelion_test_allocator`'s counting global allocator (`install_test_allocator!()`).
pub fn assert_allocation_free(effect: &mut dyn Effect, block: usize) {
    let mut buffer = vec![0.123_f32; block];
    effect.prepare(48_000.0, block);
    effect.process(&mut buffer); // warm up any lazy state outside the measured region
    lindelion_test_allocator::assert_no_allocations("effect process", || {
        effect.process(&mut buffer);
    });
}

/// Assert `effect.process` allocates at most `max_allocations` times per block in steady state.
/// This is the NN-effect bar (ADR-0014): neural-network inference may allocate a bounded amount
/// inline, where ordinary DSP must be strictly allocation-free ([`assert_allocation_free`]). The
/// block should span at least one model hop so an inference actually runs. Effective only when
/// the test binary installs the counting allocator.
pub fn assert_bounded_allocation(effect: &mut dyn Effect, block: usize, max_allocations: usize) {
    let mut buffer = vec![0.123_f32; block];
    effect.prepare(48_000.0, block);
    for _ in 0..4 {
        effect.process(&mut buffer); // settle past one-time setup allocation
    }
    let count = lindelion_test_allocator::count_allocations(|| {
        effect.process(&mut buffer);
    });
    assert!(
        count <= max_allocations,
        "process allocated {count} times per block (bound {max_allocations})"
    );
}

fn probe_sine(sample_rate: f32) -> Vec<f32> {
    let w = std::f32::consts::TAU * PROBE_HZ / sample_rate;
    (0..SAMPLE_BLOCK)
        .map(|n| 0.5 * (w * n as f32).sin())
        .collect()
}

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_effect::EffectParam;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[derive(Clone, Copy)]
    enum Mode {
        Transparent,
        Nan,
        Click,
        Runaway,
        IgnoreBypass,
        WrongLatency,
        Alloc,
    }

    /// One configurable test effect. Each `Mode` breaks exactly one check (or none).
    struct Fixture {
        mode: Mode,
        bypassed: bool,
    }

    impl Fixture {
        fn new(mode: Mode) -> Self {
            Self {
                mode,
                bypassed: false,
            }
        }
    }

    impl Effect for Fixture {
        fn name(&self) -> &str {
            "Fixture"
        }
        fn parameters(&self) -> &[EffectParam] {
            &[]
        }
        fn set_parameter(&mut self, _index: u32, _value: f32) {}
        fn prepare(&mut self, _sample_rate: f32, _max_block: usize) {}

        fn process(&mut self, buffer: &mut [f32]) {
            // IgnoreBypass is the bug: it processes even when bypassed.
            if self.bypassed && !matches!(self.mode, Mode::IgnoreBypass) {
                return;
            }
            match self.mode {
                Mode::Transparent | Mode::WrongLatency => {}
                Mode::Nan => buffer.iter_mut().for_each(|s| *s = f32::NAN),
                Mode::Click => {
                    if !buffer.is_empty() {
                        buffer[buffer.len() / 2] += 1.0;
                    }
                }
                Mode::Runaway => buffer.iter_mut().for_each(|s| *s *= 1000.0),
                Mode::IgnoreBypass => buffer.iter_mut().for_each(|s| *s *= 0.5),
                Mode::Alloc => {
                    let scratch: Vec<f32> = buffer.to_vec();
                    std::hint::black_box(&scratch);
                }
            }
        }

        fn latency_samples(&self) -> usize {
            match self.mode {
                Mode::WrongLatency => 5,
                _ => 0,
            }
        }
        fn is_bypassed(&self) -> bool {
            self.bypassed
        }
        fn set_bypassed(&mut self, bypassed: bool) {
            self.bypassed = bypassed;
        }
        fn reset(&mut self) {}
        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }
        fn load_state(&mut self, _state: &[u8]) {}
    }

    fn failed_check(mode: Mode) -> &'static str {
        run_general_battery(&mut Fixture::new(mode), 48_000.0)
            .expect_err("expected a failed check")
            .check
    }

    #[test]
    fn transparent_effect_passes() {
        run_general_battery(&mut Fixture::new(Mode::Transparent), 48_000.0)
            .expect("transparent effect passes the battery");
    }

    #[test]
    fn nan_trips_finite() {
        assert_eq!(failed_check(Mode::Nan), "finite");
    }

    #[test]
    fn click_trips_no_clicks() {
        assert_eq!(failed_check(Mode::Click), "no-clicks");
    }

    #[test]
    fn runaway_trips_frequency_response() {
        assert_eq!(failed_check(Mode::Runaway), "frequency-response");
    }

    #[test]
    fn ignore_bypass_trips_identity() {
        assert_eq!(failed_check(Mode::IgnoreBypass), "bypass-identity");
    }

    #[test]
    fn wrong_latency_trips_latency() {
        assert_eq!(failed_check(Mode::WrongLatency), "latency");
    }

    #[test]
    fn transparent_effect_is_allocation_free() {
        assert_allocation_free(&mut Fixture::new(Mode::Transparent), 256);
    }

    #[test]
    fn allocating_effect_trips_allocation_check() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            assert_allocation_free(&mut Fixture::new(Mode::Alloc), 256);
        }));
        assert!(result.is_err(), "allocating process should trip the check");
    }

    #[test]
    fn bounded_allocation_admits_bounded_alloc_but_not_excess() {
        // The Alloc fixture allocates once per block: within a bound of 2, outside a bound of 0.
        assert_bounded_allocation(&mut Fixture::new(Mode::Alloc), 256, 2);
        assert_bounded_allocation(&mut Fixture::new(Mode::Transparent), 256, 0);
        let result = catch_unwind(AssertUnwindSafe(|| {
            assert_bounded_allocation(&mut Fixture::new(Mode::Alloc), 256, 0);
        }));
        assert!(
            result.is_err(),
            "allocation beyond the bound should trip the check"
        );
    }
}
