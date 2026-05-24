use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use glirdir::{AnalysisJob, AnalysisSettings, ScratchpadAudio, run_analysis_job};
use lindelion_midi::QuantizeSettings;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: u32 = 48_000;
const SECONDS: usize = 3;
const SAMPLE_COUNT: usize = SAMPLE_RATE as usize * SECONDS;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

fn synthetic_singing() -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..SAMPLE_COUNT)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            let phrase = match index / SAMPLE_RATE as usize {
                0 => 220.0,
                1 => 246.94,
                _ => 261.63,
            };
            let vibrato = (std::f32::consts::TAU * 5.2 * time).sin() * 2.5;
            let envelope = if index < 512 {
                index as f32 / 512.0
            } else if index > SAMPLE_COUNT - 512 {
                (SAMPLE_COUNT - index) as f32 / 512.0
            } else {
                1.0
            };
            let tone = (std::f32::consts::TAU * (phrase + vibrato) * time).sin() * 0.45 * envelope;
            tone + rng.gen_range(-0.01..0.01)
        })
        .collect()
}

fn analysis_job() -> AnalysisJob {
    let scratchpad = ScratchpadAudio::new(SAMPLE_RATE, synthetic_singing());
    AnalysisJob::new(
        1,
        scratchpad,
        AnalysisSettings::default(),
        QuantizeSettings::default(),
    )
}

fn bench_analysis(criterion: &mut Criterion) {
    let job = analysis_job();
    assert!(run_analysis_job(&job).result.is_ok());

    let mut group = criterion.benchmark_group("glirdir");
    group.throughput(Throughput::Elements(SAMPLE_COUNT as u64));
    group.bench_function("analyze_synthetic_3s", |bench| {
        bench.iter(|| black_box(run_analysis_job(black_box(&job))));
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
