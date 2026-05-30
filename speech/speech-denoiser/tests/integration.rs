//! Speech Denoiser model-integration suite (heavy: loads the model and runs ONNX Runtime
//! inference). Every test here is `#[ignore]`d so it does **not** run in `make ci` — running
//! hundreds of inferences in parallel with the rest of the workspace's unit tests saturates the
//! CPU. Run this suite on its own with `make test-models` (or
//! `cargo test -p lindelion-speech-denoiser --test integration -- --include-ignored`). Cheap,
//! model-free contract tests live in `tests/contract.rs` and do run in `make ci`.
//!
//! The denoiser runs inference inline (ADR-0014), so it is held to the bounded-allocation bar,
//! not the strict allocation-free one. The streaming tests are the emphasis: the host-side
//! hop-buffering must be block-size invariant and the recurrent state must thread correctly
//! across hops, with no boundary artifacts.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{BatteryOptions, assert_bounded_allocation, run_general_battery_with};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_denoiser::{HOP_SIZE, LATENCY_SAMPLES, MODEL_SAMPLE_RATE, SpeechDenoiser};

lindelion_test_allocator::install_test_allocator!();

// --- helpers ---------------------------------------------------------------

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// SNR in dB of `estimate` against `reference`, over the error `estimate - reference`.
fn snr_db(estimate: &[f32], reference: &[f32]) -> f32 {
    let n = estimate.len().min(reference.len());
    let sig: f32 = reference[..n].iter().map(|s| s * s).sum();
    let err: f32 = (0..n).map(|i| (estimate[i] - reference[i]).powi(2)).sum();
    10.0 * (sig / err.max(1e-20)).log10()
}

/// Deterministic white noise in roughly [-amp, amp).
fn white_noise(n: usize, seed: u64, amp: f32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let u = (s >> 33) as f32 / (1u64 << 31) as f32; // [0, 2)
            (u - 1.0) * amp
        })
        .collect()
}

fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 / ratio;
            let i0 = src.floor() as usize;
            let frac = (src - i0 as f64) as f32;
            let a = input.get(i0).copied().unwrap_or(0.0);
            let b = input.get(i0 + 1).copied().unwrap_or(a);
            a + (b - a) * frac
        })
        .collect()
}

fn process_in_blocks(effect: &mut SpeechDenoiser, signal: &[f32], block: usize) -> Vec<f32> {
    let mut out = signal.to_vec();
    let mut start = 0;
    while start < out.len() {
        let end = (start + block).min(out.len());
        effect.process(&mut out[start..end]);
        start = end;
    }
    out
}

fn prepared() -> SpeechDenoiser {
    let mut effect = SpeechDenoiser::new();
    effect.prepare(MODEL_SAMPLE_RATE, 1024);
    effect
}

/// 48 kHz mono speech, resampled from the 44.1 kHz stereo-downmixed fixture.
fn speech_48k() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio/vocal_spoken.wav")
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode vocal_spoken.wav");
    resample_linear(&decoded.samples, decoded.sample_rate, 48_000)
}

// --- contract with the model loaded ----------------------------------------

#[test]
#[ignore = "loads the model; run via `make test-models`"]
fn reports_model_latency_when_active() {
    let effect = prepared();
    assert_eq!(effect.latency_samples(), LATENCY_SAMPLES);
}

#[test]
#[ignore = "loads the model; run via `make test-models`"]
fn bypass_is_identity() {
    let mut effect = prepared();
    effect.set_bypassed(true);
    assert_eq!(effect.latency_samples(), 0);
    let input = white_noise(2048, 11, 0.3);
    let output = process_in_blocks(&mut effect, &input, 256);
    assert_eq!(output, input);
}

// --- streaming correctness (the emphasis) ----------------------------------

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn block_size_invariant() {
    // The same input, fed in hop-aligned (480), unaligned (512), and ragged (137) blocks, must
    // produce the same output: the hop-buffering and recurrent-state threading cannot depend on
    // how the host happens to chunk the stream.
    let input = white_noise(48_000, 99, 0.2);

    let out_aligned = process_in_blocks(&mut prepared(), &input, HOP_SIZE);
    let out_unaligned = process_in_blocks(&mut prepared(), &input, 512);
    let out_ragged = process_in_blocks(&mut prepared(), &input, 137);

    let max_diff = |a: &[f32], b: &[f32]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0_f32, f32::max)
    };
    assert!(
        max_diff(&out_aligned, &out_unaligned) < 1e-4,
        "480 vs 512 block sizes diverged"
    );
    assert!(
        max_diff(&out_aligned, &out_ragged) < 1e-4,
        "480 vs 137 block sizes diverged"
    );
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn no_block_boundary_artifacts() {
    // Sample-to-sample deltas must stay bounded across hop boundaries (no clicks where one hop's
    // output meets the next).
    let input = white_noise(24_000, 23, 0.2);
    let output = process_in_blocks(&mut prepared(), &input, 512);
    let max_delta = output
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_delta < 0.5, "boundary delta {max_delta} too large");
    assert!(output.iter().all(|s| s.is_finite()));
}

// --- denoising behaviour ---------------------------------------------------

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn attenuates_broadband_noise() {
    // Pure broadband noise carries no speech; the model should suppress it well below input level.
    let input = white_noise(48_000, 5, 0.3);
    let output = process_in_blocks(&mut prepared(), &input, 480);
    // Skip the warm-up + latency region.
    let warm = 4 * HOP_SIZE;
    let in_rms = rms(&input[warm..]);
    let out_rms = rms(&output[warm..]);
    assert!(
        out_rms < 0.6 * in_rms,
        "noise not suppressed: in {in_rms:.4} out {out_rms:.4}"
    );
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn improves_snr_on_noisy_speech() {
    let clean = speech_48k();
    assert!(clean.len() > 48_000, "fixture too short");
    let noise_amp = 1.2 * rms(&clean);
    let noisy: Vec<f32> = clean
        .iter()
        .zip(white_noise(clean.len(), 1234, noise_amp))
        .map(|(c, n)| c + n)
        .collect();

    let enhanced = process_in_blocks(&mut prepared(), &noisy, 512);

    // `enhanced` is delayed by the model's end-to-end latency. Compare against clean at that lag;
    // the noisy reference is aligned (lag 0). The lag search confirms the declared latency.
    let warm = 6 * HOP_SIZE;
    let len = clean.len() - 3_000 - warm;
    let clean_seg = &clean[warm..warm + len];
    let noisy_seg = &noisy[warm..warm + len];

    let (lag, snr_out) = (0..=2_400)
        .map(|lag| {
            let enh_seg = &enhanced[warm + lag..warm + lag + len];
            (lag, snr_db(enh_seg, clean_seg))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let snr_in = snr_db(noisy_seg, clean_seg);
    eprintln!("best lag {lag} samples, snr_in {snr_in:.1} dB, snr_out {snr_out:.1} dB");
    // The best-alignment lag confirms the declared end-to-end latency for an NN effect (the
    // general battery cannot, see ADR-0014).
    assert!(
        (lag as i32 - LATENCY_SAMPLES as i32).abs() <= 48,
        "best-alignment lag {lag} should match declared latency {LATENCY_SAMPLES}"
    );
    // SNR improves on noisy speech. The margin is conservative: vs. white noise on a quiet
    // resampled fixture the model reliably lifts SNR ~2.8 dB (3.3 -> 6.1 dB observed); a real
    // recorded-noise input would gain more.
    assert!(
        snr_out > snr_in + 2.0,
        "denoiser did not improve SNR: in {snr_in:.1} dB out {snr_out:.1} dB (lag {lag})"
    );
}

// --- battery + allocation bar ----------------------------------------------

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn passes_general_battery() {
    // Latency is scoped out (ADR-0014): the model's warm-up transient produces output before its
    // declared latency, which the linear-delay heuristic cannot model. Latency is validated by
    // the best-lag alignment in `improves_snr_on_noisy_speech` instead.
    let mut effect = SpeechDenoiser::new();
    run_general_battery_with(
        &mut effect,
        MODEL_SAMPLE_RATE,
        BatteryOptions {
            check_latency: false,
        },
    )
    .expect("denoiser passes general battery");
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn inference_allocation_is_bounded() {
    // ADR-0014: NN inference may allocate a bounded amount inline. The block spans two hops so an
    // inference actually runs in the measured call. The bound proves it is bounded (not growing),
    // not that it is small.
    let mut effect = SpeechDenoiser::new();
    assert_bounded_allocation(&mut effect, 2 * HOP_SIZE, 4_000);
}
