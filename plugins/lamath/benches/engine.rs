use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lamath::{
    BenchSelectedExcitations as SelectedExcitations, BenchSynthEngine as SynthEngine,
    BenchVoiceTrigger as VoiceTrigger, ModalConfig, ModalPreset, OutputConfig, ResonatorConfig,
    ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
};
use lindelion_dsp_utils::analysis::assert_all_finite;

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 512;

fn impulse() -> Vec<f32> {
    let mut excitation = vec![0.0; 64];
    excitation[0] = 1.0;
    excitation
}

fn test_patch(polyphony: usize) -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        polyphony: polyphony as u8,
        resonator_a: ResonatorConfig::Modal(ModalConfig {
            mode_count: 16,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.4,
            ..ModalConfig::default()
        }),
        resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.9,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 0.8,
            mix_b: 0.2,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: -6.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}

fn start_voice<'a>(
    engine: &mut SynthEngine<'a>,
    note: u8,
    excitation: &'a [f32],
    patch: &ResonatorSynthPatch,
) -> usize {
    let selected = SelectedExcitations::from_single(excitation, SAMPLE_RATE);
    let trigger = VoiceTrigger::with_excitations(note, 1.0, selected, patch);
    engine.note_on(trigger)
}

fn active_engine<'a>(
    polyphony: usize,
    excitation: &'a [f32],
    patch: &ResonatorSynthPatch,
) -> SynthEngine<'a> {
    let mut engine = SynthEngine::with_live_latch_capacity(SAMPLE_RATE, polyphony, 0);
    for voice in 0..polyphony {
        start_voice(&mut engine, 60 + voice as u8, excitation, patch);
    }
    engine
}

fn render_replace(engine: &mut SynthEngine<'_>, left: &mut [f32], right: &mut [f32]) {
    left.fill(0.0);
    right.fill(0.0);
    engine.render_add(left, right);
}

fn verify_render(polyphony: usize, excitation: &[f32], patch: &ResonatorSynthPatch) {
    let mut engine = active_engine(polyphony, excitation, patch);
    let mut left = vec![0.0; BLOCK_SIZE];
    let mut right = vec![0.0; BLOCK_SIZE];
    render_replace(&mut engine, &mut left, &mut right);
    assert_all_finite(&left);
    assert_all_finite(&right);
}

fn bench_render_polyphony(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    polyphony: usize,
    excitation: &[f32],
    patch: &ResonatorSynthPatch,
) {
    let name = format!("render_replace_512_poly{polyphony}");
    group.bench_function(name, |bench| {
        bench.iter_batched(
            || {
                (
                    active_engine(polyphony, excitation, patch),
                    vec![0.0; BLOCK_SIZE],
                    vec![0.0; BLOCK_SIZE],
                )
            },
            |(mut engine, mut left, mut right)| {
                render_replace(&mut engine, &mut left, &mut right);
                black_box((left, right));
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_note_on_steady_state(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    excitation: &[f32],
    patch: &ResonatorSynthPatch,
) {
    group.throughput(Throughput::Elements(1));
    group.bench_function("note_on_steady_state", |bench| {
        bench.iter_batched(
            || active_engine(16, excitation, patch),
            |mut engine| {
                let slot = start_voice(&mut engine, black_box(84), excitation, patch);
                black_box(slot);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_engine(criterion: &mut Criterion) {
    let excitation = impulse();
    for polyphony in [1, 8, 16] {
        verify_render(polyphony, &excitation, &test_patch(polyphony));
    }

    let patch = test_patch(16);
    let mut group = criterion.benchmark_group("engine");
    group.throughput(Throughput::Elements(BLOCK_SIZE as u64));
    bench_render_polyphony(&mut group, 1, &excitation, &patch);
    bench_render_polyphony(&mut group, 8, &excitation, &patch);
    bench_render_polyphony(&mut group, 16, &excitation, &patch);
    bench_note_on_steady_state(&mut group, &excitation, &patch);
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(700))
        .sample_size(10);
    targets = bench_engine
}
criterion_main!(benches);
