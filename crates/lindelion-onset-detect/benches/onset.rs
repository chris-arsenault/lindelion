use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_onset_detect::{
    ConfiguredOnsetDetector, DetectionConfig, OnsetDetectionInput, OnsetDetector,
};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_SIZE: usize = 4096;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn synthetic_tone_burst() -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..BLOCK_SIZE)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            let tone = (std::f32::consts::TAU * 880.0 * time).sin();
            let burst = if (1400..2300).contains(&index) {
                0.8
            } else {
                0.08
            };
            tone * burst + rng.gen_range(-0.015..0.015)
        })
        .collect()
}

fn bench_onset(criterion: &mut Criterion) {
    let audio = synthetic_tone_burst();
    let detector = ConfiguredOnsetDetector;
    let config = DetectionConfig::default();
    let input = OnsetDetectionInput::new(&audio, SAMPLE_RATE);
    assert!(!detector.detect(input, config).is_empty());

    let mut group = criterion.benchmark_group("onset");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("detect_block_4096", |bench| {
        bench.iter(|| {
            let input = OnsetDetectionInput::new(black_box(&audio), SAMPLE_RATE);
            black_box(detector.detect(input, black_box(config)));
        });
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_onset
}
criterion_main!(benches);
