//! The candidate objective (M5): turn the per-fixture measured metrics into a single score, gated by
//! the hard no-artifact constraints. Pure — it consumes precomputed [`metrics`](super::metrics)
//! values (the heavy full-chain run that produces them is the harness, Step 6); it never runs the
//! chain itself, so it is `make ci`-testable.

use super::config::TuningConfig;

/// Noise-reduction measurement on the matched noisy↔clean pair (best-lag SNR in/out, dB).
#[derive(Debug, Clone, Copy)]
pub struct NoiseMetric {
    pub snr_in_db: f32,
    pub snr_out_db: f32,
}

/// Dereverb measurement on the synthesized-reverb variant (late-tail RMS, dry vs wet).
#[derive(Debug, Clone, Copy)]
pub struct DereverbMetric {
    pub dry_tail: f32,
    pub wet_tail: f32,
}

/// Clarity measurement on clean speech (HF-band presence in/out, dB).
#[derive(Debug, Clone, Copy)]
pub struct ClarityMetric {
    pub hf_in_db: f32,
    pub hf_out_db: f32,
}

/// The measured metrics for one fixture's chain output. The always-present fields feed the hard
/// constraints + the loudness term; the optional fields carry the role a particular fixture probes.
#[derive(Debug, Clone, Copy)]
pub struct FixtureMetrics {
    pub finite: bool,
    pub peak: f32,
    pub pumping_variance_db2: f32,
    pub loudness_lufs: f32,
    pub noise: Option<NoiseMetric>,
    pub dereverb: Option<DereverbMetric>,
    pub clarity: Option<ClarityMetric>,
    pub coloration_db: Option<f32>,
}

impl FixtureMetrics {
    /// A finite, non-clipping, non-pumping fixture at a usable level — the neutral baseline tests
    /// build on.
    pub fn passing(loudness_lufs: f32) -> Self {
        Self {
            finite: true,
            peak: 0.5,
            pumping_variance_db2: 0.0,
            loudness_lufs,
            noise: None,
            dereverb: None,
            clarity: None,
            coloration_db: None,
        }
    }
}

/// All fixtures' metrics for one candidate patch.
#[derive(Debug, Clone)]
pub struct CandidateMetrics {
    pub per_fixture: Vec<FixtureMetrics>,
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Reward (0..1) for hitting the loudness target: 1 at target, → 0 at `tolerance_lu` away.
fn loudness_reward(lufs: f32, cfg: &TuningConfig) -> f32 {
    if !lufs.is_finite() {
        return 0.0;
    }
    1.0 - clamp01((lufs - cfg.loudness.lufs).abs() / cfg.loudness.tolerance_lu)
}

/// Reward (0..1) for noise reduction: SNR improvement scaled by `noise_improvement_db`.
fn noise_reward(m: NoiseMetric, cfg: &TuningConfig) -> f32 {
    clamp01((m.snr_out_db - m.snr_in_db) / cfg.scales.noise_improvement_db)
}

/// Reward (0..1) for dereverb: the fraction of late-tail energy removed.
fn dereverb_reward(m: DereverbMetric) -> f32 {
    if m.dry_tail <= 0.0 {
        return 0.0;
    }
    clamp01(1.0 - m.wet_tail / m.dry_tail)
}

/// Reward (0..1) for clarity: HF presence **preserved-or-raised**. Preserved → 0.5; raised by the
/// scale → 1; cut by the scale → 0.
fn clarity_reward(m: ClarityMetric, cfg: &TuningConfig) -> f32 {
    clamp01(0.5 + (m.hf_out_db - m.hf_in_db) / (2.0 * cfg.scales.clarity_scale_db))
}

/// Reward (0..1) for low coloration: 1 when the core band is untouched, → 0 at `coloration_scale_db`.
fn coloration_reward(deviation_db: f32, cfg: &TuningConfig) -> f32 {
    1.0 - clamp01(deviation_db / cfg.scales.coloration_scale_db)
}

/// Mean of an iterator of rewards, or `0.5` (neutral) if none were measured.
fn mean_or_neutral(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, count) = values.fold((0.0_f32, 0_usize), |(s, c), v| (s + v, c + 1));
    if count == 0 { 0.5 } else { sum / count as f32 }
}

/// Whether `fixture` violates any hard constraint.
///
/// The constraints are **non-finite** and **clipping** (peak above the ceiling). Pumping is *not* a
/// hard gate: no robust objective metric separates artifact pumping from a denoiser/VAD-gate's
/// intended (and large) time-varying gain — every clean chain with those slots "pumps" by any
/// gain-variance measure. `pumping_variance_db2` is retained on `FixtureMetrics` for diagnostics
/// only; transparency is instead shaped by the scored low-coloration term.
fn violates_constraints(fixture: &FixtureMetrics, cfg: &TuningConfig) -> bool {
    !fixture.finite || !fixture.peak.is_finite() || fixture.peak > cfg.constraints.peak_ceiling
}

/// The weighted candidate score in 0..1, or `None` if any fixture violates a hard constraint.
pub fn score(candidate: &CandidateMetrics, cfg: &TuningConfig) -> Option<f32> {
    if candidate.per_fixture.is_empty()
        || candidate
            .per_fixture
            .iter()
            .any(|f| violates_constraints(f, cfg))
    {
        return None;
    }

    let loudness = mean_or_neutral(
        candidate
            .per_fixture
            .iter()
            .map(|f| loudness_reward(f.loudness_lufs, cfg)),
    );
    let noise = mean_or_neutral(
        candidate
            .per_fixture
            .iter()
            .filter_map(|f| f.noise)
            .map(|m| noise_reward(m, cfg)),
    );
    let dereverb = mean_or_neutral(
        candidate
            .per_fixture
            .iter()
            .filter_map(|f| f.dereverb)
            .map(dereverb_reward),
    );
    let clarity = mean_or_neutral(
        candidate
            .per_fixture
            .iter()
            .filter_map(|f| f.clarity)
            .map(|m| clarity_reward(m, cfg)),
    );
    let coloration = mean_or_neutral(
        candidate
            .per_fixture
            .iter()
            .filter_map(|f| f.coloration_db)
            .map(|d| coloration_reward(d, cfg)),
    );

    let w = cfg.weights;
    Some(
        w.loudness * loudness
            + w.noise_reduction * noise
            + w.dereverb * dereverb
            + w.clarity * clarity
            + w.low_coloration * coloration,
    )
}

#[cfg(test)]
mod tests {
    use super::super::config::default_config;
    use super::*;

    fn battery(loudness: f32) -> CandidateMetrics {
        // One fixture carrying every role, at the loudness target.
        CandidateMetrics {
            per_fixture: vec![FixtureMetrics {
                noise: Some(NoiseMetric {
                    snr_in_db: 3.0,
                    snr_out_db: 6.0, // +3 dB → mid reward, leaving headroom for the comparison tests
                }),
                dereverb: Some(DereverbMetric {
                    dry_tail: 1.0,
                    wet_tail: 0.5,
                }),
                clarity: Some(ClarityMetric {
                    hf_in_db: -20.0,
                    hf_out_db: -20.0,
                }),
                coloration_db: Some(0.0),
                ..FixtureMetrics::passing(loudness)
            }],
        }
    }

    #[test]
    fn clipping_fixture_is_rejected() {
        let cfg = default_config();
        let mut c = battery(-16.0);
        c.per_fixture[0].peak = 0.98; // > 0.95 ceiling
        assert_eq!(score(&c, &cfg), None);
    }

    #[test]
    fn non_finite_is_rejected_but_pumping_is_not_a_gate() {
        let cfg = default_config();
        let mut nf = battery(-16.0);
        nf.per_fixture[0].finite = false;
        assert_eq!(score(&nf, &cfg), None);

        // High gain-envelope variance is NOT a hard gate (a denoiser/gate legitimately pumps).
        let mut pump = battery(-16.0);
        pump.per_fixture[0].pumping_variance_db2 = cfg.constraints.pumping_variance_db2 + 1_000.0;
        assert!(score(&pump, &cfg).is_some());
    }

    #[test]
    fn on_target_loudness_beats_off_target() {
        let cfg = default_config();
        let on = score(&battery(-16.0), &cfg).unwrap();
        let off = score(&battery(-22.0), &cfg).unwrap();
        assert!(
            on > off,
            "on-target loudness must score higher: {on} vs {off}"
        );
    }

    #[test]
    fn better_noise_reduction_raises_the_score() {
        let cfg = default_config();
        let weak = battery(-16.0);
        let mut strong = battery(-16.0);
        strong.per_fixture[0].noise = Some(NoiseMetric {
            snr_in_db: 3.0,
            snr_out_db: 12.0, // larger improvement
        });
        assert!(score(&strong, &cfg).unwrap() > score(&weak, &cfg).unwrap());
    }

    #[test]
    fn raised_clarity_beats_cut_clarity() {
        let cfg = default_config();
        let mut raised = battery(-16.0);
        raised.per_fixture[0].clarity = Some(ClarityMetric {
            hf_in_db: -20.0,
            hf_out_db: -16.0,
        });
        let mut cut = battery(-16.0);
        cut.per_fixture[0].clarity = Some(ClarityMetric {
            hf_in_db: -20.0,
            hf_out_db: -26.0,
        });
        assert!(score(&raised, &cfg).unwrap() > score(&cut, &cfg).unwrap());
    }

    #[test]
    fn empty_candidate_scores_none() {
        let cfg = default_config();
        assert_eq!(
            score(
                &CandidateMetrics {
                    per_fixture: vec![]
                },
                &cfg
            ),
            None
        );
    }
}
