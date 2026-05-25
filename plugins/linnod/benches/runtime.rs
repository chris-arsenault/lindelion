use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use lindelion_pitch_shift::PitchShiftAnalyzer;
use lindelion_plugin_shell::{AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ProcessContext};
use lindelion_sample_library::{
    OwnedMonoAudioBuffer, RuntimeMonoAudioBuffer, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use linnod::{Linnod, SourceAnalysis, SourceAnalysisJobResult};

const BLOCK_SIZE: usize = 512;
const SAMPLE_RATE: u32 = 48_000;

fn bench_runtime(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linnod");
    group.bench_function("render_512_poly1", |bench| {
        bench.iter_batched(
            || RuntimeBenchFixture::with_polyphony(1),
            |mut fixture| fixture.render_empty_block(),
            BatchSize::SmallInput,
        );
    });
    group.bench_function("render_512_poly16", |bench| {
        bench.iter_batched(
            || RuntimeBenchFixture::with_polyphony(16),
            |mut fixture| fixture.render_empty_block(),
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

struct RuntimeBenchFixture {
    plugin: Linnod,
    left: [f32; BLOCK_SIZE],
    right: [f32; BLOCK_SIZE],
}

impl RuntimeBenchFixture {
    fn with_polyphony(polyphony: usize) -> Self {
        let mut plugin = Linnod::default();
        let job = plugin.request_source_ingest_job("bench-source.wav");
        assert!(
            plugin.publish_source_analysis_result(SourceAnalysisJobResult::ready(
                job.sequence,
                source_analysis(),
            ))
        );

        let mut left = [0.0; BLOCK_SIZE];
        let mut right = [0.0; BLOCK_SIZE];
        let events = note_on_events(polyphony);
        plugin.process(ProcessContext::new(
            Default::default(),
            AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            &events,
        ));

        Self {
            plugin,
            left: [0.0; BLOCK_SIZE],
            right: [0.0; BLOCK_SIZE],
        }
    }

    fn render_empty_block(&mut self) {
        self.left.fill(0.0);
        self.right.fill(0.0);
        self.plugin.process(ProcessContext::new(
            Default::default(),
            AudioBuffer {
                left: &mut self.left,
                right: &mut self.right,
            },
            &[],
        ));
        black_box(self.left[0] + self.right[0]);
    }
}

fn source_analysis() -> SourceAnalysis {
    let samples = sine_wave(220.0, SAMPLE_RATE, SAMPLE_RATE as usize);
    let owned_audio = OwnedMonoAudioBuffer::new(samples.clone(), SAMPLE_RATE);
    let pitch_contour = PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: SAMPLE_RATE,
        hop_size: 1_200,
        frames: (0..40)
            .map(|frame| pitch_frame(frame, frame * 1_200))
            .collect(),
    };
    let markers = vec![SliceMarker {
        position_samples: 0,
        kind: MarkerKind::Auto,
    }];
    let pitch_shift_cache = PitchShiftAnalyzer::default()
        .analyze(&samples, owned_audio.sample_rate, &pitch_contour, &markers)
        .expect("bench source analysis should build pitch-shift cache");

    SourceAnalysis {
        source: SampleMetadata {
            reference: SampleReference::new("bench-hash", "Samples/bench-source.wav"),
            filename: "bench-source.wav".to_string(),
            duration_ms: 1_000,
            sample_rate: SAMPLE_RATE,
            channels: 1,
            rms_db: None,
            peak_db: None,
            waveform_preview: SampleWaveformPreview { points: Vec::new() },
        },
        audio: RuntimeMonoAudioBuffer::from_owned(owned_audio),
        pitch_contour,
        markers,
        pitch_shift_cache,
    }
}

fn pitch_frame(frame_index: usize, source_sample_position: usize) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / SAMPLE_RATE as f32,
        f0_hz: Some(220.0),
        raw_f0_hz: 220.0,
        confidence: 0.95,
        voiced: true,
        rms: 0.2,
    }
}

fn note_on_events(polyphony: usize) -> Vec<MidiEvent> {
    (0..polyphony)
        .map(|index| {
            MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 36 + index as u8,
                velocity: 1.0,
            })
        })
        .collect()
}

fn sine_wave(frequency_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate as f32).sin() * 0.5
        })
        .collect()
}

criterion_group!(benches, bench_runtime);
criterion_main!(benches);
