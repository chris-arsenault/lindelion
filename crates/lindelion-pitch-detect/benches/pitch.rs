use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_pitch_detect::{
    PitchDetectionConfig, StreamingPitchTracker, ZeroCrossingStreamingPitchTracker,
};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_SIZE: usize = 2048;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn sine_wave() -> Vec<f32> {
    (0..BLOCK_SIZE)
        .map(|index| {
            (std::f32::consts::TAU * 440.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.5
        })
        .collect()
}

fn noisy_sine_wave() -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    sine_wave()
        .into_iter()
        .map(|sample| sample + rng.gen_range(-0.03..0.03))
        .collect()
}

fn verify_pitch_block(audio: &[f32]) {
    let mut tracker =
        ZeroCrossingStreamingPitchTracker::new(SAMPLE_RATE, PitchDetectionConfig::default());
    let frames = tracker
        .next_block(audio)
        .expect("zero-crossing pitch succeeds");
    assert!(!frames.is_empty());
    assert!(frames.iter().all(|frame| frame.raw_f0_hz.is_finite()));
}

fn bench_pitch(criterion: &mut Criterion) {
    let sine = sine_wave();
    let noisy = noisy_sine_wave();
    verify_pitch_block(&sine);
    verify_pitch_block(&noisy);

    let mut group = criterion.benchmark_group("pitch");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("analyze_2048_sine", |bench| {
        bench.iter_batched(
            || ZeroCrossingStreamingPitchTracker::new(SAMPLE_RATE, PitchDetectionConfig::default()),
            |mut tracker| {
                let frames = tracker
                    .next_block(black_box(&sine))
                    .expect("zero-crossing pitch succeeds");
                black_box(frames.len());
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("analyze_2048_noisy", |bench| {
        bench.iter_batched(
            || ZeroCrossingStreamingPitchTracker::new(SAMPLE_RATE, PitchDetectionConfig::default()),
            |mut tracker| {
                let frames = tracker
                    .next_block(black_box(&noisy))
                    .expect("zero-crossing pitch succeeds");
                black_box(frames.len());
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_pitch
}
criterion_main!(benches);
