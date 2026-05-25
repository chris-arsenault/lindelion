use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use crate::{SourceAnalysisJob, SourceAnalysisJobResult};

pub trait LinnodWorkerQueue {
    fn schedule_source_analysis(&self, job: SourceAnalysisJob) -> bool;

    fn drain_results(&self, publish: &mut dyn FnMut(LinnodWorkerResult)) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinnodWorkerResult {
    SourceAnalysis(SourceAnalysisJobResult),
}

pub struct LinnodWorker {
    commands: Sender<LinnodWorkerCommand>,
    results: Receiver<LinnodWorkerResult>,
    handle: Option<JoinHandle<()>>,
}

impl Default for LinnodWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinnodWorker {
    pub fn new() -> Self {
        Self::with_runner(DefaultSourceAnalysisRunner)
    }

    pub fn with_runner<R>(runner: R) -> Self
    where
        R: SourceAnalysisJobRunner,
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

impl LinnodWorkerQueue for LinnodWorker {
    fn schedule_source_analysis(&self, job: SourceAnalysisJob) -> bool {
        self.commands
            .send(LinnodWorkerCommand::AnalyzeSource(Box::new(job)))
            .is_ok()
    }

    fn drain_results(&self, publish: &mut dyn FnMut(LinnodWorkerResult)) -> usize {
        let mut count = 0;
        while let Ok(result) = self.results.try_recv() {
            publish(result);
            count += 1;
        }
        count
    }
}

impl Drop for LinnodWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(LinnodWorkerCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub trait SourceAnalysisJobRunner: Send + 'static {
    fn run_source_analysis(&mut self, job: SourceAnalysisJob) -> SourceAnalysisJobResult;
}

#[derive(Debug, Default)]
struct DefaultSourceAnalysisRunner;

impl SourceAnalysisJobRunner for DefaultSourceAnalysisRunner {
    fn run_source_analysis(&mut self, job: SourceAnalysisJob) -> SourceAnalysisJobResult {
        job.run()
    }
}

enum LinnodWorkerCommand {
    AnalyzeSource(Box<SourceAnalysisJob>),
    Shutdown,
}

fn run_worker<R>(
    commands: Receiver<LinnodWorkerCommand>,
    results: Sender<LinnodWorkerResult>,
    mut runner: R,
) where
    R: SourceAnalysisJobRunner,
{
    while let Ok(command) = commands.recv() {
        match command {
            LinnodWorkerCommand::AnalyzeSource(job) => {
                let result = runner.run_source_analysis(*job);
                let _ = results.send(LinnodWorkerResult::SourceAnalysis(result));
            }
            LinnodWorkerCommand::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use super::*;
    use crate::{LinnodPatch, SourceAnalysisError, SourceLoadError};

    #[test]
    fn worker_runs_source_analysis_jobs_off_thread() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = LinnodWorker::with_runner(CountingRunner {
            calls: Arc::clone(&calls),
        });
        let job = SourceAnalysisJob::ingest(
            9,
            "missing.wav",
            &LinnodPatch::default(),
            PathBuf::from("."),
        );

        assert!(worker.schedule_source_analysis(job));

        let result = wait_for_one_result(&worker);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            result,
            LinnodWorkerResult::SourceAnalysis(SourceAnalysisJobResult::error(
                9,
                SourceLoadError::Analysis(SourceAnalysisError::EmptySource)
            ))
        );
    }

    #[derive(Debug)]
    struct CountingRunner {
        calls: Arc<AtomicUsize>,
    }

    impl SourceAnalysisJobRunner for CountingRunner {
        fn run_source_analysis(&mut self, job: SourceAnalysisJob) -> SourceAnalysisJobResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            SourceAnalysisJobResult::error(
                job.sequence,
                SourceLoadError::Analysis(SourceAnalysisError::EmptySource),
            )
        }
    }

    fn wait_for_one_result(worker: &LinnodWorker) -> LinnodWorkerResult {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut result = None;
            worker.drain_results(&mut |next| result = Some(next));
            if let Some(result) = result {
                return result;
            }
            assert!(Instant::now() < deadline, "timed out waiting for worker");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
