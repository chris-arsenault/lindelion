use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_psola::{PitchAnalysis, PitchEpoch, PitchEstimate, PitchShift, PsolaEngine};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_SIZE: usize = 2048;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn synthetic_audio() -> Vec<f32> {
    (0..BLOCK_SIZE)
        .map(|index| {
            (std::f32::consts::TAU * 220.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.6
        })
        .collect()
}

fn pitch_analysis() -> PitchAnalysis {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    let epochs = (0..BLOCK_SIZE)
        .step_by(109)
        .map(|position_samples| PitchEpoch { position_samples })
        .collect();
    let estimates = (0..BLOCK_SIZE)
        .step_by(256)
        .map(|position_samples| PitchEstimate {
            position_samples,
            fundamental_hz: 220.0 + rng.gen_range(-0.5..0.5),
            confidence: 0.95,
        })
        .collect();
    PitchAnalysis {
        sample_rate: SAMPLE_RATE,
        epochs,
        estimates,
    }
}

fn verify_synthesis(input: &[f32], analysis: &PitchAnalysis) {
    let mut engine = PsolaEngine;
    let mut output = vec![0.0; input.len()];
    engine.process_placeholder(
        input,
        &mut output,
        analysis,
        PitchShift {
            semitones: 0,
            cents: 0.0,
        },
    );
    assert!(output.iter().all(|sample| sample.is_finite()));
}

fn bench_psola(criterion: &mut Criterion) {
    let input = synthetic_audio();
    let analysis = pitch_analysis();
    assert!(analysis.median_fundamental_hz().is_some());
    verify_synthesis(&input, &analysis);

    let mut group = criterion.benchmark_group("psola");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("analysis_2048", |bench| {
        bench.iter_batched(
            pitch_analysis,
            |analysis| black_box(analysis.median_fundamental_hz()),
            BatchSize::SmallInput,
        );
    });
    group.bench_function("synthesis_2048", |bench| {
        bench.iter_batched(
            || (PsolaEngine, vec![0.0; input.len()]),
            |(mut engine, mut output)| {
                engine.process_placeholder(
                    black_box(&input),
                    &mut output,
                    black_box(&analysis),
                    PitchShift {
                        semitones: 0,
                        cents: 0.0,
                    },
                );
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
    targets = bench_psola
}
criterion_main!(benches);
