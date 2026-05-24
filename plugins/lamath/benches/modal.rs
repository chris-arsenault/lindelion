use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lamath::{BenchModalBank as ModalBank, BenchModalBankParams as ModalBankParams, ModalPreset};
use lindelion_dsp_utils::analysis::assert_all_finite;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 512;

fn excitation_block() -> Vec<f32> {
    let mut input = vec![0.0; BLOCK_SIZE];
    input[0] = 1.0;
    input[17] = 0.25;
    input
}

fn params(mode_count: usize) -> ModalBankParams {
    ModalBankParams {
        fundamental_hz: 80.0,
        mode_count,
        preset: ModalPreset::GenericStrike,
        inharmonicity: 0.0,
        brightness: 0.55,
        decay_global: 0.8,
        decay_tilt: 0.45,
        ..ModalBankParams::default()
    }
}

fn process_block(mode_count: usize, input: &[f32]) -> Vec<f32> {
    let modal_params = params(mode_count);
    let mut bank = ModalBank::with_capacity(SAMPLE_RATE, mode_count, modal_params);
    input
        .iter()
        .copied()
        .map(|sample| bank.process_sample(sample))
        .collect()
}

fn bench_modal_count(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    mode_count: usize,
    input: &[f32],
) {
    let name = format!("process_512_n{mode_count}");
    let modal_params = params(mode_count);
    group.bench_function(name, |bench| {
        bench.iter_batched(
            || {
                (
                    ModalBank::with_capacity(SAMPLE_RATE, mode_count, modal_params),
                    vec![0.0; input.len()],
                )
            },
            |(mut bank, mut output)| {
                for (output, sample) in output.iter_mut().zip(black_box(input)) {
                    *output = bank.process_sample(*sample);
                }
                black_box(output);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_modal(criterion: &mut Criterion) {
    let input = excitation_block();
    for mode_count in [16, 64, 256] {
        assert_all_finite(&process_block(mode_count, &input));
    }

    let mut group = criterion.benchmark_group("modal");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    bench_modal_count(&mut group, 16, &input);
    bench_modal_count(&mut group, 64, &input);
    bench_modal_count(&mut group, 256, &input);
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_modal
}
criterion_main!(benches);
