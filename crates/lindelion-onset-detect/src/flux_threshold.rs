//! Adaptive peak-picking threshold shared by the batch and streaming spectral-flux
//! detectors.

/// Number of frames in the causal (backward-only) moving window used to estimate
/// the local novelty floor. At the 256-sample hop this is roughly 85-90 ms.
const ADAPTIVE_THRESHOLD_AVG_FRAMES: usize = 16;
/// Relative margin above the local mean (a scale-invariant `delta`). Without it a
/// sustained constant-flux plateau (e.g. a slow attack ramp) has `std -> 0`, so
/// the threshold collapses to the mean and the plateau re-triggers spuriously.
const ADAPTIVE_THRESHOLD_RELATIVE_DELTA: f32 = 0.1;
/// Floor as a fraction of the running novelty peak. After a loud onset the local
/// mean decays toward the steady-state ripple, so a peak-relative floor is needed
/// to reject that ripple while staying scale-invariant (batch and streaming agree).
const ADAPTIVE_THRESHOLD_PEAK_FLOOR_FRACTION: f32 = 0.05;
/// Silence guard so float noise in an empty window cannot produce a peak.
const ADAPTIVE_THRESHOLD_FLOOR: f32 = 1e-6;

/// Local peak-picking threshold at `index` over a causal window, given the running
/// novelty peak over `flux[0..=index]`. Both the adaptive term and the floor scale
/// linearly with the novelty, so the peak decision is identical whether the flux is
/// normalized (batch) or raw (streaming).
pub(crate) fn local_flux_threshold(
    flux: &[f32],
    index: usize,
    sensitivity: f32,
    running_peak: f32,
) -> f32 {
    if flux.is_empty() {
        return ADAPTIVE_THRESHOLD_FLOOR;
    }

    let index = index.min(flux.len() - 1);
    let start = index.saturating_sub(ADAPTIVE_THRESHOLD_AVG_FRAMES - 1);
    let window = &flux[start..=index];
    let count = window.len() as f32;
    let mean = window.iter().copied().sum::<f32>() / count;
    let variance = window
        .iter()
        .map(|value| (value - mean) * (value - mean))
        .sum::<f32>()
        / count;
    let std_dev = variance.sqrt();
    // Relative delta keeps a flat plateau from re-triggering; the peak-relative floor
    // rejects low-level steady-state ripple once the local mean has decayed.
    let adaptive = mean * (1.0 + ADAPTIVE_THRESHOLD_RELATIVE_DELTA)
        + std_dev * (1.5 - sensitivity.clamp(0.0, 1.0));
    let floor =
        (running_peak * ADAPTIVE_THRESHOLD_PEAK_FLOOR_FRACTION).max(ADAPTIVE_THRESHOLD_FLOOR);
    adaptive.max(floor)
}
