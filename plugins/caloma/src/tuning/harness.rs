//! The full-chain battery evaluator (M5 Step 6): run a candidate patch through an order's **real**
//! chain on every fixture and turn the outputs into a [`CandidateMetrics`](super::score). The chain
//! and the analysis are built **once per order** (the NN models load once) and *reset* between runs,
//! so each candidate is scored from a clean state — the score is a pure, reproducible function of the
//! patch. Analysis is the **synchronous** [`SignalAnalyzer`] (deterministic per block), not the
//! realtime off-thread worker.
//!
//! Building the order's chain/analyzer loads the DFN3/Silero/SwiftF0 models, so the production path
//! is heavy (`make tune-defaults` / `make test-models`). The metric/alignment wiring is also covered
//! by a cheap non-NN unit test (`make ci`) via the `#[cfg(test)]` no-analysis constructor.

use lindelion_speech_signals::{SignalAnalyzer, SignalSnapshot};

use crate::order::SignalOrder;
use crate::patch::CalomaPatch;
use crate::runtime::ChainRuntime;

use super::config::TuningConfig;
use super::metrics;
use super::score::{
    CandidateMetrics, ClarityMetric, DereverbMetric, FixtureMetrics, NoiseMetric, SibilanceMetric,
};

const BLOCK: usize = 512;
const TAIL_WINDOW: usize = 2_048;

/// One battery fixture and the roles it probes.
pub struct BatteryFixture {
    pub name: String,
    /// The chain input. For the dereverb fixture this is the *reverberant* signal.
    pub input: Vec<f32>,
    pub sample_rate: f32,
    /// The matched clean reference for noise-reduction SNR (noisy fixtures only).
    pub clean_reference: Option<Vec<f32>>,
    /// Measure dereverb (late-tail energy of input vs output).
    pub measures_dereverb: bool,
    /// Measure clarity (HF presence) + coloration (core-band) of output vs the dry input.
    pub measures_clarity: bool,
    /// Measure sibilance (es-burst prominence over the program level) of output vs the dry input.
    pub measures_sibilance: bool,
}

impl BatteryFixture {
    /// A plain fixture probing only loudness + the hard constraints.
    pub fn plain(name: impl Into<String>, input: Vec<f32>, sample_rate: f32) -> Self {
        Self {
            name: name.into(),
            input,
            sample_rate,
            clean_reference: None,
            measures_dereverb: false,
            measures_clarity: false,
            measures_sibilance: false,
        }
    }
}

/// Evaluates candidate patches for one order against a fixed battery. Holds the order's chain +
/// (optional) analyzer so the models load once.
pub struct OrderEvaluator {
    chain: ChainRuntime,
    analyzer: Option<SignalAnalyzer>,
    fixtures: Vec<BatteryFixture>,
    cfg: TuningConfig,
}

impl OrderEvaluator {
    /// Production: build the order's **real** topology chain + a synchronous analyzer (loads the NN
    /// models). Heavy — `make tune-defaults`/`make test-models` only.
    pub fn new(order: SignalOrder, fixtures: Vec<BatteryFixture>, cfg: TuningConfig) -> Self {
        let sample_rate = fixtures.first().map_or(48_000.0, |f| f.sample_rate);
        Self {
            chain: ChainRuntime::new(order, sample_rate, BLOCK),
            analyzer: Some(SignalAnalyzer::new(sample_rate as u32)),
            fixtures,
            cfg,
        }
    }

    /// Test-only: a non-NN chain from an explicit slot list and **no** analyzer (the snapshot is the
    /// default), so the metric/alignment wiring runs in the fast suite.
    #[cfg(test)]
    fn from_slots_no_analysis(
        slots: &[crate::slot::SlotId],
        fixtures: Vec<BatteryFixture>,
        cfg: TuningConfig,
    ) -> Self {
        let sample_rate = fixtures.first().map_or(48_000.0, |f| f.sample_rate);
        Self {
            chain: ChainRuntime::from_slots(slots, sample_rate, BLOCK),
            analyzer: None,
            fixtures,
            cfg,
        }
    }

    /// Process one signal through the (freshly reset) chain → output.
    fn run(&mut self, patch: &CalomaPatch, input: &[f32]) -> Vec<f32> {
        self.chain.reset();
        if let Some(analyzer) = self.analyzer.as_mut() {
            analyzer.reset();
        }
        let mut out = input.to_vec();
        let mut start = 0;
        while start < out.len() {
            let end = (start + BLOCK).min(out.len());
            // The analysis tap is at the head: analyze the (pre-chain) input block, then process.
            let snapshot = match self.analyzer.as_mut() {
                Some(analyzer) => analyzer.process(&out[start..end]),
                None => SignalSnapshot::default(),
            };
            self.chain.process(&mut out[start..end], patch, &snapshot);
            start = end;
        }
        out
    }

    fn fixture_metrics(&mut self, patch: &CalomaPatch, fixture: &BatteryFixture) -> FixtureMetrics {
        let sr = fixture.sample_rate;
        let latency = self.chain.latency_samples();
        let output = self.run(patch, &fixture.input);

        // Latency-align the wet output with the dry input for the per-sample comparisons.
        let aligned = output.get(latency..).unwrap_or(&[]);
        let n = aligned.len().min(fixture.input.len());
        let aligned = &aligned[..n];
        let dry = &fixture.input[..n];

        // Noise reduction, isolated from the chain's *intended* processing: run the matched clean
        // reference through the **same** chain, so the EQ/compression/enhancement cancels and the
        // residual is the noise the denoiser failed to remove. SNR_out compares the processed-noisy
        // output to the processed-clean reference (co-aligned: identical chain latency); SNR_in is
        // the raw noisy-vs-clean SNR.
        let noise = fixture.clean_reference.as_ref().map(|clean| {
            let snr_in = metrics::snr_db(&fixture.input, clean);
            let processed_clean = self.run(patch, clean);
            let m = output.len().min(processed_clean.len());
            let snr_out = metrics::snr_db(&output[..m], &processed_clean[..m]);
            NoiseMetric {
                snr_in_db: snr_in,
                snr_out_db: snr_out,
            }
        });
        let dereverb = fixture.measures_dereverb.then(|| DereverbMetric {
            dry_tail: metrics::late_tail_rms(&fixture.input, TAIL_WINDOW),
            wet_tail: metrics::late_tail_rms(&output, TAIL_WINDOW),
        });
        let (clarity, coloration_db) = if fixture.measures_clarity {
            let b = &self.cfg.bands;
            (
                Some(ClarityMetric {
                    hf_in_db: metrics::hf_presence_db(dry, sr, b.hf_lo, b.hf_hi),
                    hf_out_db: metrics::hf_presence_db(aligned, sr, b.hf_lo, b.hf_hi),
                }),
                Some(metrics::coloration_deviation_db(
                    aligned, dry, sr, b.core_lo, b.core_hi,
                )),
            )
        } else {
            (None, None)
        };
        let sibilance = fixture.measures_sibilance.then(|| SibilanceMetric {
            prominence_in_db: metrics::sibilance_prominence_db(
                dry,
                sr,
                self.cfg.bands.sib_lo,
                self.cfg.bands.sib_hi,
            ),
            prominence_out_db: metrics::sibilance_prominence_db(
                aligned,
                sr,
                self.cfg.bands.sib_lo,
                self.cfg.bands.sib_hi,
            ),
        });

        FixtureMetrics {
            finite: metrics::all_finite(&output),
            peak: metrics::peak(&output),
            pumping_variance_db2: metrics::gain_envelope_variance_db(aligned, dry, BLOCK),
            loudness_lufs: metrics::integrated_loudness_lufs(&output, sr),
            noise,
            dereverb,
            clarity,
            coloration_db,
            sibilance,
        }
    }

    /// The full per-fixture metrics for `patch`.
    pub fn evaluate(&mut self, patch: &CalomaPatch) -> CandidateMetrics {
        let fixtures = std::mem::take(&mut self.fixtures);
        let per_fixture = fixtures
            .iter()
            .map(|f| self.fixture_metrics(patch, f))
            .collect();
        self.fixtures = fixtures;
        CandidateMetrics { per_fixture }
    }

    /// The scored objective for the search: `score(evaluate(patch))`.
    pub fn objective(&mut self, patch: &CalomaPatch) -> Option<f32> {
        let candidate = self.evaluate(patch);
        super::score::score(&candidate, &self.cfg)
    }

    /// The chain's **reported** latency in samples (fixed-max = Σ slot latencies, D5) — the value the
    /// VST3 adapter hands the host for delay compensation.
    pub fn reported_latency(&self) -> usize {
        self.chain.latency_samples()
    }

    /// The chain's **measured** group delay for `patch`: run `probe` through the chain and return the
    /// lag that best aligns the (colored) output with the input — the denoiser test's best-lag idiom
    /// applied to the whole chain. Used by the M6 latency-accuracy gate to confirm the reported
    /// latency matches the actual delay (a tolerance check — coloration blurs the alignment).
    pub fn measure_latency(&mut self, patch: &CalomaPatch, probe: &[f32]) -> usize {
        let max_lag = self.reported_latency() + 2_048;
        let output = self.run(patch, probe);
        metrics::best_lag_snr_db(&output, probe, max_lag).0
    }
}

/// Synthesize a reverberant variant of `clean` with a decaying feedback comb (the dereverb test's
/// model): a delayed, attenuated copy fills the gaps, peak-limited to stay below clipping.
pub fn synthesize_reverb(clean: &[f32], sample_rate: f32) -> Vec<f32> {
    let mut reverb = clean.to_vec();
    let delay = (0.04 * sample_rate) as usize;
    for i in delay..reverb.len() {
        reverb[i] += 0.5 * reverb[i - delay];
    }
    let peak = metrics::peak(&reverb);
    if peak > 0.99 {
        let g = 0.99 / peak;
        for s in reverb.iter_mut() {
            *s *= g;
        }
    }
    reverb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::SlotId;
    use crate::tuning::config::default_config;

    fn speech_like(n: usize, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / sr;
                0.2 * ((150.0 * t).sin() + 0.5 * (300.0 * t).sin() + 0.25 * (2_500.0 * t).sin())
            })
            .collect()
    }

    fn synthetic_battery(sr: f32) -> Vec<BatteryFixture> {
        // Short fixtures: this test exercises the metric/alignment *wiring*, not real tuning, so it
        // stays in the fast `make ci` suite (loudness falls back to whole-signal below one block).
        let clean = speech_like(6_144, sr);
        let noisy: Vec<f32> = clean
            .iter()
            .enumerate()
            .map(|(i, s)| s + 0.05 * ((i as f32 * 0.37).sin()))
            .collect();
        vec![
            BatteryFixture {
                name: "clean".into(),
                input: clean.clone(),
                sample_rate: sr,
                clean_reference: None,
                measures_dereverb: false,
                measures_clarity: true,
                measures_sibilance: true,
            },
            BatteryFixture {
                name: "noisy".into(),
                input: noisy,
                sample_rate: sr,
                clean_reference: Some(clean.clone()),
                measures_dereverb: false,
                measures_clarity: false,
                measures_sibilance: false,
            },
            BatteryFixture {
                name: "reverb".into(),
                input: synthesize_reverb(&clean, sr),
                sample_rate: sr,
                clean_reference: None,
                measures_dereverb: true,
                measures_clarity: false,
                measures_sibilance: false,
            },
            BatteryFixture::plain("pauses", speech_like(6_144, sr), sr),
        ]
    }

    #[test]
    fn non_nn_harness_produces_finite_deterministic_metrics() {
        let sr = 48_000.0;
        // A non-NN chain so the wiring runs without loading models.
        let slots = [SlotId::HighPass, SlotId::FiveBandEq, SlotId::Limiter];
        let mut evaluator =
            OrderEvaluator::from_slots_no_analysis(&slots, synthetic_battery(sr), default_config());

        let mut patch = CalomaPatch::default();
        patch.high_pass.enabled = true;
        patch.five_band_eq.enabled = true;
        patch.limiter.enabled = true;

        let a = evaluator.evaluate(&patch);
        let b = evaluator.evaluate(&patch);
        assert_eq!(a.per_fixture.len(), 4);
        // Every metric finite; the run is reproducible (reset between candidates).
        for fm in &a.per_fixture {
            assert!(fm.finite && fm.peak.is_finite() && fm.loudness_lufs.is_finite());
        }
        for (x, y) in a.per_fixture.iter().zip(b.per_fixture.iter()) {
            assert_eq!(
                x.loudness_lufs, y.loudness_lufs,
                "evaluation must be deterministic"
            );
            assert_eq!(x.peak, y.peak);
        }
        // The noisy fixture yields a noise metric; the reverb fixture a dereverb metric; clean a
        // clarity + coloration metric.
        assert!(a.per_fixture[1].noise.is_some());
        assert!(a.per_fixture[2].dereverb.is_some());
        assert!(
            a.per_fixture[0].clarity.is_some()
                && a.per_fixture[0].coloration_db.is_some()
                && a.per_fixture[0].sibilance.is_some()
        );
    }

    #[test]
    fn objective_scores_a_constraint_clean_patch() {
        let sr = 48_000.0;
        // An all-disabled (passthrough) chain: output ≈ input → finite, non-clipping at this level.
        let slots = [SlotId::FiveBandEq];
        let mut evaluator =
            OrderEvaluator::from_slots_no_analysis(&slots, synthetic_battery(sr), default_config());
        let score = evaluator.objective(&CalomaPatch::default());
        assert!(
            score.is_some(),
            "a clean passthrough must not violate a hard constraint"
        );
        assert!((0.0..=1.0).contains(&score.unwrap()));
    }

    #[test]
    fn latency_accessors_report_and_recover_the_chain_delay() {
        use crate::runtime::ChainRuntime;
        let sr = 48_000.0;
        // Dereverberation is a non-NN slot with real (STFT) latency; disabled (the default), the
        // chain delay-compensates it into a pure delay, so the best-lag measurement recovers the
        // reported latency exactly.
        let slots = [SlotId::Dereverberation];
        let mut evaluator =
            OrderEvaluator::from_slots_no_analysis(&slots, synthetic_battery(sr), default_config());

        let reported = evaluator.reported_latency();
        assert_eq!(
            reported,
            ChainRuntime::from_slots(&slots, sr, BLOCK).latency_samples(),
            "reported latency must equal the chain's Σ slot latencies"
        );
        assert!(reported > 0, "Dereverberation must contribute latency");

        let probe = speech_like(16_384, sr);
        let measured = evaluator.measure_latency(&CalomaPatch::default(), &probe);
        assert_eq!(
            measured, reported,
            "best-lag must recover the delay-compensated bypass delay exactly"
        );
    }

    #[test]
    fn synthesize_reverb_adds_a_tail_and_stays_below_clipping() {
        let sr = 48_000.0;
        let clean = speech_like(12_288, sr);
        let reverb = synthesize_reverb(&clean, sr);
        assert!(metrics::peak(&reverb) <= 0.99);
        assert!(
            metrics::late_tail_rms(&reverb, TAIL_WINDOW)
                > metrics::late_tail_rms(&clean, TAIL_WINDOW),
            "reverb must add late-tail energy"
        );
    }
}
