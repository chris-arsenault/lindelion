//! Two-axis signal benchmarks (M2 gate): per-signal derivation cost in isolation, and the
//! N× redundant-chain cost that informs whether a signal should be shared (M4). The third axis —
//! allocation safety — is covered by the `assert_no_allocations` tests in `inline` and `worker`.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lindelion_speech_signals::{
    FricativeActivity, SibilanceEnergy, SignalAnalyzer, SpeechPresence,
};

const SR: f32 = 48_000.0;

fn block(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| 0.3 * (std::f32::consts::TAU * 200.0 * i as f32 / SR).sin())
        .collect()
}

fn signal_benches(c: &mut Criterion) {
    let buf = block(512);

    // --- Per-signal cost in isolation (cheap inline signals). ---
    c.bench_function("speech_presence", |b| {
        let mut signal = SpeechPresence::new();
        signal.prepare(SR);
        b.iter(|| {
            for &x in &buf {
                black_box(signal.process(black_box(x)));
            }
        });
    });
    c.bench_function("fricative", |b| {
        let mut signal = FricativeActivity::new();
        signal.prepare(SR);
        b.iter(|| {
            for &x in &buf {
                black_box(signal.process(black_box(x)));
            }
        });
    });
    c.bench_function("sibilance", |b| {
        let mut signal = SibilanceEnergy::new();
        signal.prepare(SR);
        b.iter(|| {
            for &x in &buf {
                black_box(signal.process(black_box(x)));
            }
        });
    });

    // --- N× redundant cost: 4 consumers each self-deriving SpeechPresence. ---
    c.bench_function("speech_presence_x4", |b| {
        let mut signals = [(); 4].map(|_| {
            let mut s = SpeechPresence::new();
            s.prepare(SR);
            s
        });
        b.iter(|| {
            for &x in &buf {
                for s in signals.iter_mut() {
                    black_box(s.process(black_box(x)));
                }
            }
        });
    });

    // --- Heavy worker-path analyzer (SwiftF0 + flux + HNR), isolation and 3× redundancy. ---
    let big = block(4_096);
    c.bench_function("signal_analyzer", |b| {
        let mut analyzer = SignalAnalyzer::new(SR as u32);
        b.iter(|| black_box(analyzer.process(black_box(&big))));
    });
    c.bench_function("signal_analyzer_x3", |b| {
        let mut analyzers = [(); 3].map(|_| SignalAnalyzer::new(SR as u32));
        b.iter(|| {
            for analyzer in analyzers.iter_mut() {
                black_box(analyzer.process(black_box(&big)));
            }
        });
    });
}

criterion_group!(benches, signal_benches);
criterion_main!(benches);
