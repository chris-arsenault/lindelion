use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lamath::{
    BenchWaveguideParams as WaveguideParams, BenchWaveguideResonator as WaveguideResonator,
    WaveguideStyle,
};
use lindelion_dsp_utils::analysis::assert_all_finite;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 512;

fn excitation_block() -> Vec<f32> {
    let mut input = vec![0.0; BLOCK_SIZE];
    input[0] = 1.0;
    input[11] = 0.2;
    input
}

fn params(style: WaveguideStyle) -> WaveguideParams {
    WaveguideParams {
        style,
        frequency_hz: 220.0,
        loop_filter_cutoff: 12_000.0,
        loop_filter_resonance: 0.2,
        loop_gain: 0.96,
        loop_nonlinearity: 0.1,
        position_of_strike: 0.35,
        ..WaveguideParams::default()
    }
}

fn process_block(style: WaveguideStyle, input: &[f32]) -> Vec<f32> {
    let waveguide_params = params(style);
    let mut waveguide = WaveguideResonator::new(SAMPLE_RATE, 20.0);
    input
        .iter()
        .copied()
        .map(|sample| waveguide.process_sample(sample, waveguide_params))
        .collect()
}

fn bench_waveguide_style(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    style: WaveguideStyle,
    input: &[f32],
) {
    let waveguide_params = params(style);
    group.bench_function(name, |bench| {
        bench.iter_batched(
            || {
                (
                    WaveguideResonator::new(SAMPLE_RATE, 20.0),
                    vec![0.0; input.len()],
                )
            },
            |(mut waveguide, mut output)| {
                for (output, sample) in output.iter_mut().zip(black_box(input)) {
                    *output = waveguide.process_sample(*sample, waveguide_params);
                }
                black_box(output);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_waveguide(criterion: &mut Criterion) {
    let input = excitation_block();
    assert_all_finite(&process_block(WaveguideStyle::String, &input));
    assert_all_finite(&process_block(WaveguideStyle::Tube, &input));

    let mut group = criterion.benchmark_group("waveguide");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    bench_waveguide_style(&mut group, "string_512", WaveguideStyle::String, &input);
    bench_waveguide_style(&mut group, "tube_512", WaveguideStyle::Tube, &input);
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_waveguide
}
criterion_main!(benches);
