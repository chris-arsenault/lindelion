use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use lindelion_midi::MidiClip;

use crate::{
    AnalysisJob, AnalysisJobResult, AnalysisSequence, RequantizeJob,
    analysis_job::{run_analysis_job, run_requantize_job},
};

pub(crate) trait GlirdirWorkerQueue {
    fn schedule_analysis(&self, job: AnalysisJob) -> bool;

    fn schedule_requantize(&self, job: RequantizeJob) -> bool;

    fn schedule_midi_export(&self, sequence: AnalysisSequence, clip: MidiClip) -> bool;

    fn drain_results(&self, publish: &mut dyn FnMut(GlirdirWorkerResult)) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GlirdirWorkerResult {
    Analysis(AnalysisJobResult),
    MidiExport {
        sequence: AnalysisSequence,
        bytes: Vec<u8>,
    },
}

pub(crate) struct GlirdirWorker {
    commands: Sender<GlirdirWorkerCommand>,
    results: Receiver<GlirdirWorkerResult>,
    handle: Option<JoinHandle<()>>,
}

impl Default for GlirdirWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl GlirdirWorker {
    pub(crate) fn new() -> Self {
        Self::with_runner(DefaultAnalysisRunner)
    }

    pub(crate) fn with_runner<R>(runner: R) -> Self
    where
        R: AnalysisJobRunner,
    {
        let (command_tx, command_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let handle = thread::spawn(move || run_worker(command_rx, result_tx, runner));
        Self {
            commands: command_tx,
            results: result_rx,
            handle: Some(handle),
        }
    }
}

impl GlirdirWorkerQueue for GlirdirWorker {
    fn schedule_analysis(&self, job: AnalysisJob) -> bool {
        self.commands
            .send(GlirdirWorkerCommand::Analyze(job))
            .is_ok()
    }

    fn schedule_requantize(&self, job: RequantizeJob) -> bool {
        self.commands
            .send(GlirdirWorkerCommand::Requantize(job))
            .is_ok()
    }

    fn schedule_midi_export(&self, sequence: AnalysisSequence, clip: MidiClip) -> bool {
        self.commands
            .send(GlirdirWorkerCommand::ExportMidi { sequence, clip })
            .is_ok()
    }

    fn drain_results(&self, publish: &mut dyn FnMut(GlirdirWorkerResult)) -> usize {
        let mut count = 0;
        while let Ok(result) = self.results.try_recv() {
            publish(result);
            count += 1;
        }
        count
    }
}

impl Drop for GlirdirWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(GlirdirWorkerCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) trait AnalysisJobRunner: Send + 'static {
    fn run_analysis(&mut self, job: AnalysisJob) -> AnalysisJobResult;
}

#[derive(Debug, Default)]
struct DefaultAnalysisRunner;

impl AnalysisJobRunner for DefaultAnalysisRunner {
    fn run_analysis(&mut self, job: AnalysisJob) -> AnalysisJobResult {
        run_analysis_job(&job)
    }
}

enum GlirdirWorkerCommand {
    Analyze(AnalysisJob),
    Requantize(RequantizeJob),
    ExportMidi {
        sequence: AnalysisSequence,
        clip: MidiClip,
    },
    Shutdown,
}

fn run_worker<R>(
    commands: Receiver<GlirdirWorkerCommand>,
    results: Sender<GlirdirWorkerResult>,
    mut runner: R,
) where
    R: AnalysisJobRunner,
{
    while let Ok(command) = commands.recv() {
        match command {
            GlirdirWorkerCommand::Analyze(job) => {
                let result = runner.run_analysis(job);
                let _ = results.send(GlirdirWorkerResult::Analysis(result));
            }
            GlirdirWorkerCommand::Requantize(job) => {
                let result = run_requantize_job(job);
                let _ = results.send(GlirdirWorkerResult::Analysis(result));
            }
            GlirdirWorkerCommand::ExportMidi { sequence, clip } => {
                if let Ok(bytes) = clip.to_smf_bytes() {
                    let _ = results.send(GlirdirWorkerResult::MidiExport { sequence, bytes });
                }
            }
            GlirdirWorkerCommand::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    use lindelion_midi::{DetectedNote, MidiClip, QuantizedNote};
    use lindelion_pitch_detect::{PitchContour, PitchFrame};

    use super::*;
    use crate::{AnalysisError, ScratchpadAudio};

    #[test]
    fn worker_runs_analysis_jobs_with_injected_runner() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = GlirdirWorker::with_runner(CountingRunner {
            calls: Arc::clone(&calls),
        });
        let job = analysis_job(7);

        assert!(worker.schedule_analysis(job));

        let result = wait_for_one_result(&worker);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            result,
            GlirdirWorkerResult::Analysis(AnalysisJobResult::error(
                7,
                AnalysisError::EmptyScratchpad
            ))
        );
    }

    #[test]
    fn worker_exports_midi_bytes_off_thread() {
        let worker = GlirdirWorker::with_runner(CountingRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        });

        assert!(worker.schedule_midi_export(3, MidiClip::empty(120)));

        let result = wait_for_one_result(&worker);
        let GlirdirWorkerResult::MidiExport { sequence, bytes } = result else {
            panic!("expected MIDI export result");
        };
        assert_eq!(sequence, 3);
        assert!(bytes.starts_with(b"MThd"));
    }

    #[test]
    fn worker_requantizes_without_analysis_runner() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = GlirdirWorker::with_runner(CountingRunner {
            calls: Arc::clone(&calls),
        });
        let job = RequantizeJob::new(
            9,
            analysis_result_with_note(61),
            lindelion_midi::QuantizeSettings {
                root: lindelion_midi::RootNote::C,
                scale: lindelion_midi::Scale::Major,
                snap_mode: lindelion_midi::SnapMode::Hard,
                ..lindelion_midi::QuantizeSettings::default()
            },
            48_000,
        );

        assert!(worker.schedule_requantize(job));

        let result = wait_for_one_result(&worker);
        let GlirdirWorkerResult::Analysis(result) = result else {
            panic!("expected analysis result");
        };
        let result = result.result.expect("requantize should succeed");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(result.midi_clip.notes[0].midi_note, 60);
    }

    #[derive(Debug)]
    struct CountingRunner {
        calls: Arc<AtomicUsize>,
    }

    impl AnalysisJobRunner for CountingRunner {
        fn run_analysis(&mut self, job: AnalysisJob) -> AnalysisJobResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            AnalysisJobResult::error(job.sequence, AnalysisError::EmptyScratchpad)
        }
    }

    fn analysis_job(sequence: AnalysisSequence) -> AnalysisJob {
        AnalysisJob::new(
            sequence,
            ScratchpadAudio::new(48_000, vec![0.0]),
            crate::AnalysisSettings::default(),
            lindelion_midi::QuantizeSettings::default(),
        )
    }

    fn analysis_result_with_note(midi_note: u8) -> crate::AnalysisResult {
        crate::AnalysisResult {
            pitch_contour: PitchContour {
                source_sample_rate: 48_000,
                analysis_sample_rate: 16_000,
                hop_size: 256,
                frames: vec![PitchFrame {
                    frame_index: 0,
                    source_sample_position: 0,
                    timestamp_seconds: 0.0,
                    f0_hz: Some(277.18),
                    raw_f0_hz: 277.18,
                    confidence: 0.95,
                    voiced: true,
                    rms: 0.2,
                }],
            },
            markers: Vec::new(),
            detected_notes: vec![DetectedNote {
                start_sample: 0,
                end_sample: 24_000,
                pitch_hz: 277.18,
                peak_rms: 0.5,
                mean_rms: 0.3,
            }],
            midi_clip: MidiClip {
                ppq: 960,
                bpm: 120,
                time_signature_numerator: 4,
                time_signature_denominator: 4,
                notes: vec![QuantizedNote {
                    start_tick: 0,
                    duration_ticks: 960,
                    midi_note,
                    velocity: 100,
                }],
            },
        }
    }

    fn wait_for_one_result(worker: &GlirdirWorker) -> GlirdirWorkerResult {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut result = None;
            worker.drain_results(&mut |worker_result| result = Some(worker_result));
            if let Some(result) = result {
                return result;
            }
            assert!(Instant::now() < deadline, "worker did not produce a result");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
