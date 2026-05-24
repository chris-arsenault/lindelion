use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_dsp_utils::analysis::{dft_magnitude_at, peak_abs, rms, spectral_centroid_hz};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_512: usize = 512;
const BLOCK_2048: usize = 2048;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn seeded_noise(len: usize) -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..len).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

fn tone_with_noise(len: usize) -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..len)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE;
            let low = (std::f32::consts::TAU * 440.0 * time).sin() * 0.5;
            let high = (std::f32::consts::TAU * 3_200.0 * time).sin() * 0.15;
            let noise = rng.gen_range(-0.03..0.03);
            low + high + noise
        })
        .collect()
}

fn bench_analysis(criterion: &mut Criterion) {
    let block_512 = seeded_noise(BLOCK_512);
    let block_2048 = tone_with_noise(BLOCK_2048);
    assert!(peak_abs(&block_512).is_finite());
    assert!(rms(&block_512).is_finite());
    assert!(dft_magnitude_at(&block_2048, SAMPLE_RATE, 440.0).is_finite());
    assert!(spectral_centroid_hz(&block_2048, SAMPLE_RATE).is_some());

    let mut group = criterion.benchmark_group("analysis");
    group.throughput(Throughput::Bytes(
        (BLOCK_512 * std::mem::size_of::<f32>()) as u64,
    ));
    group.bench_function("peak_abs_512", |bench| {
        bench.iter(|| black_box(peak_abs(black_box(&block_512))));
    });
    group.bench_function("rms_512", |bench| {
        bench.iter(|| black_box(rms(black_box(&block_512))));
    });

    group.throughput(Throughput::Elements(1));
    group.bench_function("dft_magnitude_at", |bench| {
        bench.iter(|| black_box(dft_magnitude_at(black_box(&block_2048), SAMPLE_RATE, 440.0)));
    });
    group.bench_function("spectral_centroid_hz", |bench| {
        bench.iter(|| black_box(spectral_centroid_hz(black_box(&block_2048), SAMPLE_RATE)));
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_analysis
}
criterion_main!(benches);
