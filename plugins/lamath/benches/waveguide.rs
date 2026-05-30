use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lamath::{
    BenchMeshResonator as MeshResonator, BenchMeshVoiceParams as MeshVoiceParams,
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

fn mesh_params() -> MeshVoiceParams {
    MeshVoiceParams {
        frequency_hz: 220.0,
        material: 0.6,
        size: 0.55,
        damping: 0.3,
        tension: 0.45,
        strike_position: 0.35,
        pickup_spread: 0.3,
    }
}

fn process_mesh_block(input: &[f32]) -> Vec<f32> {
    let mut mesh = MeshResonator::new(SAMPLE_RATE);
    mesh.configure(mesh_params());
    input
        .iter()
        .copied()
        .map(|sample| mesh.process_sample(sample))
        .collect()
}

fn bench_waveguide(criterion: &mut Criterion) {
    let input = excitation_block();
    assert_all_finite(&process_block(WaveguideStyle::String, &input));
    assert_all_finite(&process_block(WaveguideStyle::Tube, &input));
    assert_all_finite(&process_mesh_block(&input));

    let mut group = criterion.benchmark_group("waveguide");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    bench_waveguide_style(&mut group, "string_512", WaveguideStyle::String, &input);
    bench_waveguide_style(&mut group, "tube_512", WaveguideStyle::Tube, &input);
    group.bench_function("mesh_512", |bench| {
        let params = mesh_params();
        bench.iter_batched(
            || {
                let mut mesh = MeshResonator::new(SAMPLE_RATE);
                mesh.configure(params);
                (mesh, vec![0.0; input.len()])
            },
            |(mut mesh, mut output)| {
                for (output, sample) in output.iter_mut().zip(black_box(&input)) {
                    *output = mesh.process_sample(*sample);
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
    targets = bench_waveguide
}
criterion_main!(benches);
