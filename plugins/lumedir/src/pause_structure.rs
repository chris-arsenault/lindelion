//! Pause-structure estimator: the silence in a passage, from per-frame energy.
//!
//! `FIXTURES.md` defines `pause` as the fraction of **low-energy frames**, and `SignalAnalyzer`'s
//! `voicing_state` marks silence exactly when a frame's RMS falls below a floor — so this component
//! reproduces that energy-frame method directly from audio. It frames the input, marks each frame
//! silent when its level falls a configurable margin below the running intensity maximum
//! (level-robust), and tracks the runs of silent frames: `pause_fraction` plus the count / length of
//! runs that last at least `min_pause_s`.
//!
//! Plugin-local per the M0 decision. It runs off the audio thread, so it is not required to be
//! allocation-free; its state is O(1) (running counters), so it stays bounded over a session.

/// Tuning for the pause-structure estimator.
#[derive(Debug, Clone, Copy)]
pub struct PauseStructureConfig {
    /// Analysis frame length (seconds).
    pub frame_s: f32,
    /// A frame is silent when its level is more than this many dB below the running intensity max.
    pub silence_threshold_db: f32,
    /// A silence run counts as a pause only if it lasts at least this long (seconds).
    pub min_pause_s: f32,
}

impl Default for PauseStructureConfig {
    fn default() -> Self {
        Self {
            frame_s: 0.02,
            silence_threshold_db: 35.0,
            min_pause_s: 0.2,
        }
    }
}

/// Streaming pause-structure estimator. Cumulative over the fed audio (a clip / session); call
/// [`reset`](Self::reset) to start a new session.
pub struct PauseStructure {
    config: PauseStructureConfig,
    frame_samples: usize,
    // Current partial frame.
    frame_sum_sq: f64,
    frame_count: usize,
    // Classification state.
    running_max_db: f32,
    // Tallies.
    total_frames: u64,
    silent_frames: u64,
    current_run_frames: u64,
    completed_pause_count: u64,
    completed_pause_frames: u64,
}

const DB_FLOOR: f32 = -120.0;
const EPS: f32 = 1.0e-9;

fn amplitude_to_db(amplitude: f32) -> f32 {
    (20.0 * (amplitude + EPS).log10()).max(DB_FLOOR)
}

impl PauseStructure {
    pub fn new(sample_rate: f32, config: PauseStructureConfig) -> Self {
        let frame_samples = (config.frame_s * sample_rate).round().max(1.0) as usize;
        Self {
            config,
            frame_samples,
            frame_sum_sq: 0.0,
            frame_count: 0,
            running_max_db: DB_FLOOR,
            total_frames: 0,
            silent_frames: 0,
            current_run_frames: 0,
            completed_pause_count: 0,
            completed_pause_frames: 0,
        }
    }

    /// Clear all state (keeps the configuration and frame size).
    pub fn reset(&mut self) {
        self.frame_sum_sq = 0.0;
        self.frame_count = 0;
        self.running_max_db = DB_FLOOR;
        self.total_frames = 0;
        self.silent_frames = 0;
        self.current_run_frames = 0;
        self.completed_pause_count = 0;
        self.completed_pause_frames = 0;
    }

    /// Feed a block of mono audio, accumulating into frames and classifying each completed frame.
    pub fn push(&mut self, block: &[f32]) {
        for &sample in block {
            self.frame_sum_sq += (sample as f64) * (sample as f64);
            self.frame_count += 1;
            if self.frame_count >= self.frame_samples {
                let rms = (self.frame_sum_sq / self.frame_count as f64).sqrt() as f32;
                self.classify_frame(amplitude_to_db(rms));
                self.frame_sum_sq = 0.0;
                self.frame_count = 0;
            }
        }
    }

    fn classify_frame(&mut self, frame_db: f32) {
        self.running_max_db = self.running_max_db.max(frame_db);
        self.total_frames += 1;
        let silent = frame_db < self.running_max_db - self.config.silence_threshold_db;
        if silent {
            self.silent_frames += 1;
            self.current_run_frames += 1;
        } else {
            self.finalize_run();
        }
    }

    fn finalize_run(&mut self) {
        if self.run_qualifies(self.current_run_frames) {
            self.completed_pause_count += 1;
            self.completed_pause_frames += self.current_run_frames;
        }
        self.current_run_frames = 0;
    }

    fn run_qualifies(&self, run_frames: u64) -> bool {
        run_frames as f32 * self.config.frame_s >= self.config.min_pause_s
    }

    /// Fraction of low-energy (silent) frames.
    pub fn pause_fraction(&self) -> f32 {
        if self.total_frames == 0 {
            return 0.0;
        }
        self.silent_frames as f32 / self.total_frames as f32
    }

    /// Number of silence runs that qualify as pauses (≥ `min_pause_s`), including a trailing run.
    pub fn pause_count(&self) -> u64 {
        self.completed_pause_count + u64::from(self.run_qualifies(self.current_run_frames))
    }

    /// Total time in qualifying pauses (seconds), including a trailing run.
    pub fn total_pause_s(&self) -> f32 {
        let trailing = if self.run_qualifies(self.current_run_frames) {
            self.current_run_frames
        } else {
            0
        };
        (self.completed_pause_frames + trailing) as f32 * self.config.frame_s
    }

    /// Mean qualifying-pause length (seconds), or 0 with no pauses.
    pub fn mean_pause_s(&self) -> f32 {
        let count = self.pause_count();
        if count == 0 {
            return 0.0;
        }
        self.total_pause_s() / count as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn tone(secs: f32) -> Vec<f32> {
        let n = (secs * SR) as usize;
        (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / SR).sin())
            .collect()
    }

    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0_f32; (secs * SR) as usize]
    }

    #[test]
    fn measures_a_known_pause_fraction_and_run() {
        // 2 s tone, 1 s silence, 2 s tone → 1/5 silent, one ~1 s pause.
        let mut pause = PauseStructure::new(SR, PauseStructureConfig::default());
        pause.push(&tone(2.0));
        pause.push(&silence(1.0));
        pause.push(&tone(2.0));
        assert!(
            (pause.pause_fraction() - 0.2).abs() < 0.03,
            "expected ~0.2, got {}",
            pause.pause_fraction()
        );
        assert_eq!(pause.pause_count(), 1);
        assert!(
            (pause.mean_pause_s() - 1.0).abs() < 0.1,
            "expected ~1.0 s pause, got {}",
            pause.mean_pause_s()
        );
    }

    #[test]
    fn continuous_tone_has_no_pauses() {
        let mut pause = PauseStructure::new(SR, PauseStructureConfig::default());
        pause.push(&tone(5.0));
        assert!(
            pause.pause_fraction() < 0.02,
            "got {}",
            pause.pause_fraction()
        );
        assert_eq!(pause.pause_count(), 0);
    }
}
