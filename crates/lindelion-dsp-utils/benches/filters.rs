use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_dsp_utils::{
    analysis::assert_all_finite,
    filters::{Biquad, BiquadCoefficients, OnePoleLowpass},
};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 1024;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn seeded_noise(len: usize) -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..len).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

fn process_one_pole(input: &[f32]) -> Vec<f32> {
    let mut filter = OnePoleLowpass::new(1_000.0, SAMPLE_RATE);
    input
        .iter()
        .copied()
        .map(|sample| filter.process(sample))
        .collect()
}

fn process_biquad(input: &[f32]) -> Vec<f32> {
    let coefficients = BiquadCoefficients::lowpass(SAMPLE_RATE, 1_000.0, 0.707);
    let mut filter = Biquad::new(coefficients);
    input
        .iter()
        .copied()
        .map(|sample| filter.process(sample))
        .collect()
}

fn bench_filters(criterion: &mut Criterion) {
    let input = seeded_noise(BLOCK_SIZE);
    assert_all_finite(&process_one_pole(&input));
    assert_all_finite(&process_biquad(&input));

    let mut group = criterion.benchmark_group("one_pole_lowpass");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("process_1024", |bench| {
        bench.iter_batched(
            || {
                (
                    OnePoleLowpass::new(1_000.0, SAMPLE_RATE),
                    vec![0.0; input.len()],
                )
            },
            |(mut filter, mut output)| {
                for (output, sample) in output.iter_mut().zip(black_box(input.as_slice())) {
                    *output = filter.process(*sample);
                }
                black_box(output);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();

    let mut group = criterion.benchmark_group("biquad");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("lowpass_1024", |bench| {
        bench.iter_batched(
            || {
                let coefficients = BiquadCoefficients::lowpass(SAMPLE_RATE, 1_000.0, 0.707);
                (Biquad::new(coefficients), vec![0.0; input.len()])
            },
            |(mut filter, mut output)| {
                for (output, sample) in output.iter_mut().zip(black_box(input.as_slice())) {
                    *output = filter.process(*sample);
                }
                black_box(output);
            },
            BatchSize::SmallInput,
        );
    });

    group.throughput(Throughput::Elements(1));
    group.bench_function("coefs_lowpass", |bench| {
        bench.iter(|| {
            let cutoffs = black_box([20.0, 200.0, 2_000.0, 20_000.0]);
            let mut coefficients = [BiquadCoefficients::lowpass(SAMPLE_RATE, 20.0, 0.707); 4];
            for (coefficient, cutoff) in coefficients.iter_mut().zip(cutoffs) {
                *coefficient = BiquadCoefficients::lowpass(SAMPLE_RATE, cutoff, 0.707);
            }
            black_box(coefficients);
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
    targets = bench_filters
}
criterion_main!(benches);
