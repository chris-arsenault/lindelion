use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lindelion_dsp_utils::{
    envelope::{Adsr, AdsrState},
    smoothing::{SmoothedParam, SmoothedParamSpec},
};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 1024;
const BENCH_SEED: u64 = u64::from_le_bytes(*b"L1NDE110");

#[derive(Clone, Copy)]
enum Trigger {
    None,
    On,
    Off,
}

fn trigger_pattern() -> Vec<Trigger> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    let mut triggers = vec![Trigger::None; BLOCK_SIZE];
    triggers[0] = Trigger::On;
    for index in (64..BLOCK_SIZE).step_by(64) {
        triggers[index] = match rng.gen_range(0..3) {
            0 => Trigger::None,
            1 => Trigger::On,
            _ => Trigger::Off,
        };
    }
    triggers
}

fn smoother_targets() -> Vec<f32> {
    let mut rng = Pcg64Mcg::seed_from_u64(BENCH_SEED);
    (0..BLOCK_SIZE / 64)
        .map(|_| rng.gen_range(0.0..1.0))
        .collect()
}

fn verify_adsr(adsr: Adsr, triggers: &[Trigger]) {
    let mut state = AdsrState::default();
    for trigger in triggers {
        match trigger {
            Trigger::None => {}
            Trigger::On => state.note_on(),
            Trigger::Off => state.note_off(),
        }
        assert!(state.next_sample(adsr, SAMPLE_RATE).is_finite());
    }
}

fn verify_smoother(spec: SmoothedParamSpec, targets: &[f32]) {
    let mut smoother = SmoothedParam::new(spec, SAMPLE_RATE);
    for index in 0..BLOCK_SIZE {
        if index % 64 == 0 {
            smoother.set_target(targets[index / 64]);
        }
        assert!(smoother.next_sample().is_finite());
    }
}

fn bench_envelope(criterion: &mut Criterion) {
    let adsr = Adsr {
        attack_ms: 5.0,
        decay_ms: 80.0,
        sustain: 0.55,
        release_ms: 120.0,
    };
    let triggers = trigger_pattern();
    verify_adsr(adsr, &triggers);

    let spec = SmoothedParamSpec::new(0.0, 1.0, 0.25, 10.0, 0.000_1);
    let targets = smoother_targets();
    verify_smoother(spec, &targets);

    let mut group = criterion.benchmark_group("envelope");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("adsr_tick_1024", |bench| {
        bench.iter_batched(
            AdsrState::default,
            |mut state| {
                let mut sum = 0.0;
                for trigger in black_box(triggers.as_slice()) {
                    match trigger {
                        Trigger::None => {}
                        Trigger::On => state.note_on(),
                        Trigger::Off => state.note_off(),
                    }
                    sum += state.next_sample(adsr, SAMPLE_RATE);
                }
                black_box(sum);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();

    let mut group = criterion.benchmark_group("smoothing");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    group.bench_function("tick_1024", |bench| {
        bench.iter_batched(
            || SmoothedParam::new(spec, SAMPLE_RATE),
            |mut smoother| {
                let mut sum = 0.0;
                for index in 0..BLOCK_SIZE {
                    if index % 64 == 0 {
                        smoother.set_target(black_box(targets[index / 64]));
                    }
                    sum += smoother.next_sample();
                }
                black_box(sum);
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
    targets = bench_envelope
}
criterion_main!(benches);
