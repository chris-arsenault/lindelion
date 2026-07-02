//! Pure measurement primitives for the default-tuning objective (M5). Each mirrors an idiom already
//! used in the effect crates' tests, plus a real BS.1770 K-weighted integrated-loudness metric
//! (none existed in the repo). All functions operate on `&[f32]` mono blocks at a given sample rate;
//! none allocate beyond small scratch and none touch I/O or the chain — so they run in `make ci`.

use lindelion_dsp_utils::analysis::{peak_abs, rms, windowed_dft_magnitude_at};
use lindelion_dsp_utils::filters::{Biquad as DspBiquad, BiquadCoefficients};

/// Whether every sample is finite (a hard-constraint helper).
pub fn all_finite(signal: &[f32]) -> bool {
    signal.iter().all(|s| s.is_finite())
}

/// Peak absolute sample (re-exported convenience for the clipping constraint).
pub fn peak(signal: &[f32]) -> f32 {
    peak_abs(signal)
}

/// SNR in dB of `estimate` against `reference`, over the error `estimate - reference`. Mirrors
/// `speech/speech-denoiser/tests/integration.rs::snr_db`.
pub fn snr_db(estimate: &[f32], reference: &[f32]) -> f32 {
    let n = estimate.len().min(reference.len());
    if n == 0 {
        return 0.0;
    }
    let sig: f32 = reference[..n].iter().map(|s| s * s).sum();
    let err: f32 = (0..n).map(|i| (estimate[i] - reference[i]).powi(2)).sum();
    10.0 * (sig / err.max(1e-20)).log10()
}

/// Slide `output` forward by `0..=max_lag` and return the `(lag, snr_db)` that best matches
/// `reference` over the overlapping region. Used to compensate the chain's reported latency before
/// comparing the wet output to a dry/clean reference (mirrors the denoiser test's best-lag search).
pub fn best_lag_snr_db(output: &[f32], reference: &[f32], max_lag: usize) -> (usize, f32) {
    (0..=max_lag)
        .map(|lag| {
            let shifted = output.get(lag..).unwrap_or(&[]);
            (lag, snr_db(shifted, reference))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0))
}

/// Late-/gap-energy estimate: the RMS of the quietest quartile of `window`-length frames. A reverb
/// tail or pumping fills the quiet gaps, raising this; suppressing it lowers it. Mirrors
/// `speech/dereverberation/tests/integration.rs::quiet_rms`.
pub fn late_tail_rms(signal: &[f32], window: usize) -> f32 {
    let window = window.max(1);
    let mut frame_rms: Vec<f32> = Vec::new();
    let mut start = 0;
    while start + window <= signal.len() {
        frame_rms.push(rms(&signal[start..start + window]));
        start += window;
    }
    if frame_rms.is_empty() {
        return rms(signal);
    }
    frame_rms.sort_by(|a, b| a.total_cmp(b));
    let quartile = (frame_rms.len() / 4).max(1);
    (frame_rms[..quartile].iter().map(|v| v * v).sum::<f32>() / quartile as f32).sqrt()
}

/// Short-time **gain-envelope** variance, a **pumping** detector: the variance (dB²) of the chain's
/// per-frame gain `20·log10(out_rms / in_rms)`, measured **only over speech-present frames** (those
/// whose input RMS is within `SPEECH_REL_DB` of the loudest frame). This is deliberate: a denoiser
/// or gate *legitimately* changes its gain enormously in noise-only/quiet regions — that is its job,
/// not an artifact — so including those frames would flag every clean chain as "pumping". Restricting
/// to loud speech frames isolates audible pumping (a compressor breathing on sustained speech) from
/// the chain's intended quiet-region action. A transparent chain holds a near-constant gain during
/// speech → low variance.
pub fn gain_envelope_variance_db(output: &[f32], input: &[f32], window: usize) -> f32 {
    /// Speech frames are those within this many dB of the loudest input frame.
    const SPEECH_REL_DB: f32 = 20.0;
    let window = window.max(1);
    let n = output.len().min(input.len());

    // First pass: per-frame input RMS + the chain gain (dB).
    let mut frames: Vec<(f32, f32)> = Vec::new(); // (in_rms, gain_db)
    let mut start = 0;
    while start + window <= n {
        let in_rms = rms(&input[start..start + window]);
        let out_rms = rms(&output[start..start + window]).max(1e-9);
        frames.push((in_rms, 20.0 * (out_rms / in_rms.max(1e-9)).log10()));
        start += window;
    }
    let peak_in = frames.iter().fold(0.0_f32, |m, &(r, _)| m.max(r));
    if peak_in <= 0.0 {
        return 0.0;
    }
    let speech_floor = peak_in * 10.0_f32.powf(-SPEECH_REL_DB / 20.0);

    let gains: Vec<f32> = frames
        .iter()
        .filter(|&&(in_rms, _)| in_rms >= speech_floor)
        .map(|&(_, gain)| gain)
        .collect();
    if gains.len() < 2 {
        return 0.0;
    }
    let mean = gains.iter().sum::<f32>() / gains.len() as f32;
    gains.iter().map(|g| (g - mean).powi(2)).sum::<f32>() / gains.len() as f32
}

/// Log-spaced probe frequencies across `[lo_hz, hi_hz]` (inclusive endpoints).
fn band_probes(lo_hz: f32, hi_hz: f32, count: usize) -> Vec<f32> {
    let count = count.max(2);
    let (lo, hi) = (lo_hz.max(1.0), hi_hz.max(lo_hz * 1.01));
    let ratio = (hi / lo).powf(1.0 / (count - 1) as f32);
    (0..count).map(|i| lo * ratio.powi(i as i32)).collect()
}

/// Mean band magnitude in dB over log-spaced probes in `[lo_hz, hi_hz]` (a band-presence measure).
pub fn band_energy_db(signal: &[f32], sample_rate: f32, lo_hz: f32, hi_hz: f32) -> f32 {
    let probes = band_probes(lo_hz, hi_hz, 12);
    let mean_mag = probes
        .iter()
        .map(|&hz| windowed_dft_magnitude_at(signal, sample_rate, hz))
        .sum::<f32>()
        / probes.len() as f32;
    20.0 * (mean_mag + 1e-9).log10()
}

/// High-frequency presence (clarity term): band energy in the consonant/air band `[lo_hz, hi_hz]`.
pub fn hf_presence_db(signal: &[f32], sample_rate: f32, lo_hz: f32, hi_hz: f32) -> f32 {
    band_energy_db(signal, sample_rate, lo_hz, hi_hz)
}

/// Sibilance prominence: how far the es-bursts stand above the program level. Per 20 ms window,
/// the sibilant-band RMS relative to the full-band RMS (dB), over speech-active windows (those
/// within 20 dB of the loudest window — the same relative gate as the pumping metric, so the
/// measure is level-invariant, matching the de-esser's level-relative detection); returns the mean
/// of the top decile. De-essing lowers it; a static HF cut also lowers it but is caught by the
/// clarity term — the pair rewards *selective* sibilance control.
pub fn sibilance_prominence_db(signal: &[f32], sample_rate: f32, lo_hz: f32, hi_hz: f32) -> f32 {
    const SPEECH_REL_DB: f32 = 20.0;
    /// Prominence returned when nothing is measurable (silence): far below any real value.
    const EMPTY_PROMINENCE_DB: f32 = -80.0;
    let window = ((0.02 * sample_rate) as usize).max(1);
    let center = (lo_hz.max(1.0) * hi_hz.max(1.0)).sqrt();
    let q = (center / (hi_hz - lo_hz).max(1.0)).max(0.1);
    let mut bp = DspBiquad::new(BiquadCoefficients::bandpass(sample_rate, center, q));
    let banded: Vec<f32> = signal.iter().map(|&s| bp.process(s)).collect();

    let mut frames: Vec<(f32, f32)> = Vec::new(); // (full_rms, prominence_db)
    let mut start = 0;
    while start + window <= signal.len() {
        let full = rms(&signal[start..start + window]);
        let band = rms(&banded[start..start + window]);
        frames.push((full, 20.0 * ((band + 1e-9) / (full + 1e-9)).log10()));
        start += window;
    }
    let peak_full = frames.iter().fold(0.0_f32, |m, &(f, _)| m.max(f));
    if peak_full <= 0.0 {
        return EMPTY_PROMINENCE_DB;
    }
    let floor = peak_full * 10.0_f32.powf(-SPEECH_REL_DB / 20.0);
    let mut prominences: Vec<f32> = frames
        .iter()
        .filter(|&&(f, _)| f >= floor)
        .map(|&(_, p)| p)
        .collect();
    if prominences.is_empty() {
        return EMPTY_PROMINENCE_DB;
    }
    prominences.sort_by(|a, b| a.total_cmp(b));
    let decile = (prominences.len() / 10).max(1);
    let top = &prominences[prominences.len() - decile..];
    top.iter().sum::<f32>() / top.len() as f32
}

/// Coloration: the mean absolute per-probe magnitude change (dB) of `output` vs `dry` across the
/// band `[lo_hz, hi_hz]`. ~0 means the chain left that band's spectral shape alone; large means it
/// muddied/sharpened it. Used to keep the speech core band (~300–3000 Hz) transparent.
pub fn coloration_deviation_db(
    output: &[f32],
    dry: &[f32],
    sample_rate: f32,
    lo_hz: f32,
    hi_hz: f32,
) -> f32 {
    let probes = band_probes(lo_hz, hi_hz, 12);
    let sum: f32 = probes
        .iter()
        .map(|&hz| {
            let out = windowed_dft_magnitude_at(output, sample_rate, hz) + 1e-9;
            let dry = windowed_dft_magnitude_at(dry, sample_rate, hz) + 1e-9;
            (20.0 * (out / dry).log10()).abs()
        })
        .sum();
    sum / probes.len() as f32
}

// --- BS.1770 K-weighted integrated loudness (LUFS) -------------------------------------------------

/// A direct-form-II transposed biquad (the K-weighting stages).
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// The two K-weighting stages (ITU-R BS.1770-4 coefficients, **calibrated for 48 kHz** — every
/// Calóma fixture is 48 kHz). Stage 1 is the head high-shelf pre-filter; stage 2 is the RLB
/// high-pass.
fn k_weight(signal: &[f32]) -> Vec<f64> {
    let mut stage1 = Biquad::new(
        1.535_124_859_586_97,
        -2.691_696_189_406_38,
        1.198_392_810_852_85,
        -1.690_659_293_182_41,
        0.732_480_774_215_85,
    );
    let mut stage2 = Biquad::new(1.0, -2.0, 1.0, -1.990_047_454_833_98, 0.990_072_250_366_21);
    signal
        .iter()
        .map(|&x| stage2.process(stage1.process(x as f64)))
        .collect()
}

const LUFS_OFFSET: f64 = -0.691;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;

/// Mean square of a slice.
fn mean_square(block: &[f64]) -> f64 {
    if block.is_empty() {
        return 0.0;
    }
    block.iter().map(|s| s * s).sum::<f64>() / block.len() as f64
}

/// Integrated loudness in LUFS (BS.1770: K-weighting, 400 ms / 75%-overlap blocks, absolute −70 LUFS
/// gate, then the relative −10 LU gate). Returns `f32::NEG_INFINITY` for silence/too-short input.
pub fn integrated_loudness_lufs(signal: &[f32], sample_rate: f32) -> f32 {
    if sample_rate <= 0.0 || signal.is_empty() {
        return f32::NEG_INFINITY;
    }
    let weighted = k_weight(signal);
    let block = (0.4 * sample_rate as f64).round() as usize;
    let step = (block / 4).max(1);
    if weighted.len() < block {
        // Too short for a 400 ms block: measure the whole signal as one gated block.
        let z = mean_square(&weighted);
        return if z > 0.0 {
            (LUFS_OFFSET + 10.0 * z.log10()) as f32
        } else {
            f32::NEG_INFINITY
        };
    }

    // Per-block mean square z_j and block loudness.
    let mut z: Vec<f64> = Vec::new();
    let mut start = 0;
    while start + block <= weighted.len() {
        z.push(mean_square(&weighted[start..start + block]));
        start += step;
    }

    // Absolute gate at −70 LUFS.
    let abs_kept: Vec<f64> = z
        .iter()
        .copied()
        .filter(|&zj| zj > 0.0 && LUFS_OFFSET + 10.0 * zj.log10() >= ABSOLUTE_GATE_LUFS)
        .collect();
    if abs_kept.is_empty() {
        return f32::NEG_INFINITY;
    }

    // Relative gate at (mean loudness − 10 LU).
    let abs_mean = abs_kept.iter().sum::<f64>() / abs_kept.len() as f64;
    let rel_gate = LUFS_OFFSET + 10.0 * abs_mean.log10() - 10.0;
    let rel_kept: Vec<f64> = abs_kept
        .iter()
        .copied()
        .filter(|&zj| LUFS_OFFSET + 10.0 * zj.log10() >= rel_gate)
        .collect();
    let kept = if rel_kept.is_empty() {
        abs_kept
    } else {
        rel_kept
    };
    let mean = kept.iter().sum::<f64>() / kept.len() as f64;
    (LUFS_OFFSET + 10.0 * mean.log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(freq: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn snr_of_identical_signals_is_large() {
        let x = sine(220.0, 0.3, 4_096);
        assert!(snr_db(&x, &x) > 100.0);
    }

    #[test]
    fn best_lag_recovers_a_known_delay() {
        let reference = sine(180.0, 0.3, 8_192);
        let delay = 137;
        let mut output = vec![0.0_f32; delay];
        output.extend_from_slice(&reference);
        let (lag, snr) = best_lag_snr_db(&output, &reference, 512);
        assert_eq!(lag, delay, "best lag must recover the delay");
        assert!(snr > 100.0, "aligned output matches the reference");
    }

    #[test]
    fn loudness_scales_with_level() {
        let quiet = sine(1_000.0, 0.1, SR as usize * 2);
        let loud: Vec<f32> = quiet.iter().map(|s| s * 2.0).collect();
        let lq = integrated_loudness_lufs(&quiet, SR);
        let ll = integrated_loudness_lufs(&loud, SR);
        assert!(
            (ll - lq - 6.02).abs() < 0.5,
            "doubling amplitude must raise loudness ~6 LU: {lq:.2} -> {ll:.2}"
        );
    }

    #[test]
    fn loudness_calibration_is_in_a_plausible_band() {
        // A 1 kHz sine at −23 dBFS RMS (amp = 10^(-23/20)*sqrt(2)) reads near −23 LUFS.
        let amp = 10.0_f32.powf(-23.0 / 20.0) * std::f32::consts::SQRT_2;
        let s = sine(1_000.0, amp, SR as usize * 3);
        let lufs = integrated_loudness_lufs(&s, SR);
        assert!(
            (-26.0..=-21.0).contains(&lufs),
            "−23 dBFS 1 kHz sine should read ~−23 LUFS, got {lufs:.2}"
        );
    }

    #[test]
    fn coloration_is_zero_for_identical_signals() {
        let x = sine(500.0, 0.3, 8_192);
        assert!(coloration_deviation_db(&x, &x, SR, 300.0, 3_000.0) < 1e-3);
    }

    #[test]
    fn coloration_grows_when_a_band_is_attenuated() {
        let dry = sine(1_000.0, 0.3, 8_192);
        let out: Vec<f32> = dry.iter().map(|s| s * 0.5).collect(); // −6 dB across the band
        let dev = coloration_deviation_db(&out, &dry, SR, 300.0, 3_000.0);
        assert!(
            dev > 3.0,
            "a −6 dB change must register as coloration: {dev:.2}"
        );
    }

    #[test]
    fn late_tail_drops_when_a_comb_tail_is_removed() {
        // A signal with quiet gaps; adding a feedback-comb tail fills them and raises late-tail rms.
        let mut clean = vec![0.0_f32; 48_000];
        for (i, s) in clean.iter_mut().enumerate() {
            // bursts separated by silence
            if (i / 2_000) % 2 == 0 {
                *s = 0.4 * (std::f32::consts::TAU * 200.0 * i as f32 / SR).sin();
            }
        }
        let mut reverberant = clean.clone();
        let d = 1_920;
        for i in d..reverberant.len() {
            reverberant[i] += 0.6 * reverberant[i - d];
        }
        let clean_tail = late_tail_rms(&clean, 2_048);
        let reverb_tail = late_tail_rms(&reverberant, 2_048);
        assert!(
            reverb_tail > clean_tail * 1.5,
            "reverb tail must fill the gaps: clean {clean_tail:.5} reverb {reverb_tail:.5}"
        );
    }

    #[test]
    fn pumping_gain_variance_isolates_chain_action_from_speech_dynamics() {
        // A dynamic (pause-y) input: a steady-gain copy reads ~0 pumping (constant gain, even though
        // the envelope swings), while a copy whose gain breathes reads high pumping.
        let mut input = sine(300.0, 0.3, 48_000);
        for (i, s) in input.iter_mut().enumerate() {
            if (i / 1_500) % 2 == 1 {
                *s *= 0.05; // quiet "pause" sections — large envelope variance in the input itself
            }
        }
        let steady_gain: Vec<f32> = input.iter().map(|s| s * 0.7).collect();
        let pumping: Vec<f32> = input
            .iter()
            .enumerate()
            .map(|(i, s)| s * if (i / 1_000) % 2 == 0 { 1.0 } else { 0.2 })
            .collect();
        let v_steady = gain_envelope_variance_db(&steady_gain, &input, 1_024);
        let v_pump = gain_envelope_variance_db(&pumping, &input, 1_024);
        assert!(
            v_steady < 0.5,
            "constant gain on dynamic speech must read ~0 pumping: {v_steady:.3}"
        );
        assert!(
            v_pump > v_steady + 10.0,
            "a breathing gain must register as pumping: {v_steady:.2} -> {v_pump:.2}"
        );
    }

    #[test]
    fn sibilance_prominence_tracks_es_bursts_and_is_level_invariant() {
        // "Speech": a 300 Hz voice with periodic 6 kHz es-bursts. Ducking the bursts must lower
        // the prominence; scaling the whole signal must not change it (relative measure).
        let n = 48_000;
        let burst = |i: usize| (i / 4_800) % 4 == 3; // 100 ms es every 400 ms
        let make = |es_gain: f32, level: f32| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let t = i as f32 / SR;
                    let voice = 0.3 * (std::f32::consts::TAU * 300.0 * t).sin();
                    let es = if burst(i) {
                        0.25 * es_gain * (std::f32::consts::TAU * 6_000.0 * t).sin()
                    } else {
                        0.0
                    };
                    level * (voice + es)
                })
                .collect()
        };
        let full = sibilance_prominence_db(&make(1.0, 1.0), SR, 4_500.0, 9_000.0);
        let deessed = sibilance_prominence_db(&make(0.5, 1.0), SR, 4_500.0, 9_000.0);
        let quiet = sibilance_prominence_db(&make(1.0, 0.05), SR, 4_500.0, 9_000.0);
        assert!(
            full > deessed + 3.0,
            "ducking es-bursts must lower prominence: {full:.1} -> {deessed:.1}"
        );
        assert!(
            (full - quiet).abs() < 0.5,
            "prominence must be level-invariant: {full:.1} vs {quiet:.1}"
        );
    }

    #[test]
    fn hf_presence_rises_when_highs_are_boosted() {
        let lo = sine(6_000.0, 0.1, 8_192);
        let hi = sine(6_000.0, 0.4, 8_192);
        let p_lo = hf_presence_db(&lo, SR, 4_000.0, 8_000.0);
        let p_hi = hf_presence_db(&hi, SR, 4_000.0, 8_000.0);
        assert!(
            p_hi > p_lo + 6.0,
            "boosting HF must raise presence: {p_lo:.2} -> {p_hi:.2}"
        );
    }

    #[test]
    fn finite_and_peak_helpers() {
        let x = sine(220.0, 0.7, 1_024);
        assert!(all_finite(&x));
        assert!(!all_finite(&[0.0, f32::NAN]));
        assert!((peak(&x) - 0.7).abs() < 1e-3);
    }
}
