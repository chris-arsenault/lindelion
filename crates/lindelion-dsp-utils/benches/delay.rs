use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_dsp_utils::{analysis::assert_all_finite, delay::DelayLine};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const BLOCK_SIZE: usize = 1024;
const DELAY_CAPACITY: usize = 2048;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn seeded_noise(len: usize) -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..len).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

fn process_delay(input: &[f32]) -> Vec<f32> {
    let mut delay = DelayLine::new(DELAY_CAPACITY);
    let mut output = vec![0.0; input.len()];
    for (output, sample) in output.iter_mut().zip(input) {
        *output = delay.read(DELAY_CAPACITY as f32 * 0.5 + 0.25);
        delay.push(*sample);
    }
    output
}

fn bench_delay(criterion: &mut Criterion) {
    let input = seeded_noise(BLOCK_SIZE);
    assert_all_finite(&process_delay(&input));

    let mut group = criterion.benchmark_group("delay");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("process_1024", |bench| {
        bench.iter_batched(
            || (DelayLine::new(DELAY_CAPACITY), vec![0.0; input.len()]),
            |(mut delay, mut output)| {
                let delay_samples = DELAY_CAPACITY as f32 * 0.5 + 0.25;
                for (output, sample) in output.iter_mut().zip(black_box(input.as_slice())) {
                    *output = delay.read(delay_samples);
                    delay.push(*sample);
                }
                black_box(output);
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
    targets = bench_delay
}
criterion_main!(benches);
