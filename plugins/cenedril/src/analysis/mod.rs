//! Cenedril's realtime analysis tap and the lock-free hand-off to the editor.
//!
//! The tap is a **parallel** read of the input (ADR-0001 allocation-free): it never touches the
//! output, so the M0 bit-exact, zero-latency passthrough is preserved. Per block it computes an
//! STFT magnitude frame (pushed into the [`FrameRing`]), peak/RMS/crest levels, BS.1770-4 LUFS, and
//! the inline speech-presence signal, and publishes a [`MeterSnapshot`]. The allocating
//! `SignalAnalyzer` runs off-thread (M2 Step 4, via `lindelion-speech-signals`'s `AnalysisWorker`).

pub mod ring;

use std::sync::Arc;

use lindelion_dsp_utils::{
    analysis::{peak_abs, rms},
    lufs::LufsMeter,
    reassign::ReassignStft,
};
use lindelion_plugin_shell::AudioInputBuffer;
use lindelion_speech_signals::{AnalysisWorker, SignalSnapshot, inline::SpeechPresence};

pub use ring::{FrameLanes, FrameRing, MeterCell, MeterSnapshot, SignalCell};

use std::sync::Mutex;

use lindelion_ui::cenedril_vizia::{
    MeterSource, ReassignedFrame, ReassignedSource, SettingsStore, SpectrogramSource,
    meters::{MeterReadout, SignalReadout},
};

use crate::settings::SettingsCell;

/// The plugin's implementation of the editor's [`SpectrogramSource`]: drains the lock-free frame
/// ring on the editor thread. The view (in `lindelion-ui`) holds this as `Arc<dyn SpectrogramSource>`
/// and never names the realtime ring type.
pub struct CenedrilFrameSource {
    ring: Arc<FrameRing>,
    meter: Arc<MeterCell>,
    signal: Arc<SignalCell>,
    settings: Arc<SettingsCell>,
    sample_rate: f32,
    scratch: Mutex<Vec<f32>>,
}

impl CenedrilFrameSource {
    pub fn new(
        ring: Arc<FrameRing>,
        meter: Arc<MeterCell>,
        signal: Arc<SignalCell>,
        settings: Arc<SettingsCell>,
        sample_rate: f32,
    ) -> Self {
        // Three lanes (magnitude, frequency offset, time offset) per bin.
        let scratch_len = 3 * ring.bins();
        Self {
            ring,
            meter,
            signal,
            settings,
            sample_rate,
            scratch: Mutex::new(vec![0.0; scratch_len]),
        }
    }
}

/// STFT frame size from the ring's bin count (`= (bins - 1) * 2`).
fn frame_size_of(ring: &FrameRing) -> usize {
    (ring.bins() - 1) * 2
}

impl SpectrogramSource for CenedrilFrameSource {
    fn drain_frames(&self, sink: &mut dyn FnMut(&[f32])) {
        let mut scratch = self.scratch.lock().expect("spectrogram scratch poisoned");
        self.ring
            .drain_frames(&mut scratch, |_index, lanes| sink(lanes.magnitudes));
    }

    fn bins(&self) -> usize {
        self.ring.bins()
    }

    fn frame_size(&self) -> usize {
        frame_size_of(&self.ring)
    }

    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl ReassignedSource for CenedrilFrameSource {
    fn drain_frames(&self, sink: &mut dyn FnMut(ReassignedFrame)) {
        let mut scratch = self.scratch.lock().expect("reassigned scratch poisoned");
        self.ring.drain_frames(&mut scratch, |_index, lanes| {
            sink(ReassignedFrame {
                magnitudes: lanes.magnitudes,
                freq_offsets: lanes.freq_offsets,
                time_offsets: lanes.time_offsets,
            })
        });
    }

    fn bins(&self) -> usize {
        self.ring.bins()
    }

    fn frame_size(&self) -> usize {
        frame_size_of(&self.ring)
    }

    fn hop(&self) -> usize {
        // `ReassignStft` uses a 75 % overlap (hop = frame_size / 4).
        frame_size_of(&self.ring) / 4
    }

    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl MeterSource for CenedrilFrameSource {
    fn meters(&self) -> MeterReadout {
        let m = self.meter.read();
        MeterReadout {
            peak: m.peak,
            rms: m.rms,
            crest: m.crest,
            lufs_momentary: m.lufs_momentary,
            lufs_short: m.lufs_short,
            lufs_integrated: m.lufs_integrated,
        }
    }

    fn signals(&self) -> SignalReadout {
        let s = self.signal.read();
        // Speech presence is computed inline on the audio thread (in the `MeterCell`); the rest come
        // from the off-thread worker (the `SignalCell`).
        let speech_presence = self.meter.read().speech_presence;
        SignalReadout {
            pitch_hz: s.pitch_hz,
            pitch_confidence: s.pitch_confidence,
            voicing_score: s.voicing_score,
            voicing_state: s.voicing_state,
            onset_flux_high: s.onset_flux_high,
            spectral_flux: s.spectral_flux,
            hnr_db: s.hnr_db,
            speech_presence,
        }
    }
}

impl SettingsStore for CenedrilFrameSource {
    fn active_view(&self) -> u32 {
        self.settings.get().active_view
    }
    fn freq_scale(&self) -> u32 {
        self.settings.get().freq_scale
    }
    fn color_map(&self) -> u32 {
        self.settings.get().color_map
    }
    fn db_floor(&self) -> f32 {
        self.settings.get().db_floor
    }
    fn db_ceil(&self) -> f32 {
        self.settings.get().db_ceil
    }
    fn set_active_view(&self, value: u32) {
        self.settings.set_active_view(value);
    }
    fn set_freq_scale(&self, value: u32) {
        self.settings.set_freq_scale(value);
    }
    fn set_color_map(&self, value: u32) {
        self.settings.set_color_map(value);
    }
    fn set_db_floor(&self, value: f32) {
        self.settings.set_db_floor(value);
    }
    fn set_db_ceil(&self, value: f32) {
        self.settings.set_db_ceil(value);
    }
}

/// STFT frame size for the spectrogram (power of two, 75 % overlap). Bin count is `FRAME_SIZE/2+1`.
const FRAME_SIZE: usize = 2048;
/// Ring depth in frames (power of two) — the scroll history available to the editor.
const FRAME_SLOTS: usize = 512;

pub struct CenedrilAnalysis {
    reassign: ReassignStft,
    lufs: LufsMeter,
    ring: Arc<FrameRing>,
    meter: Arc<MeterCell>,
    signal: Arc<SignalCell>,
    speech: SpeechPresence,
    mono: Vec<f32>,
    silence: Vec<f32>,
    /// The off-thread heavy-analysis worker (`SignalAnalyzer` → `SignalSnapshot`). `None` until
    /// started by the host path, so the audio-thread core is constructible thread-free for tests.
    worker: Option<AnalysisWorker>,
}

impl CenedrilAnalysis {
    pub fn new(sample_rate: f32, max_block: usize) -> Self {
        let bins = FRAME_SIZE / 2 + 1;
        let max_block = max_block.max(1);
        let mut speech = SpeechPresence::new();
        speech.prepare(sample_rate);
        Self {
            reassign: ReassignStft::new(FRAME_SIZE),
            lufs: LufsMeter::new(sample_rate),
            ring: Arc::new(FrameRing::new(bins, FRAME_SLOTS)),
            meter: Arc::new(MeterCell::new()),
            signal: Arc::new(SignalCell::new()),
            speech,
            mono: vec![0.0; max_block],
            silence: vec![0.0; max_block],
            worker: None,
        }
    }

    /// Reconfigure for a new sample rate / max block. Off the audio thread (may resize scratch and
    /// restart the worker thread).
    pub fn reset(&mut self, sample_rate: f32, max_block: usize) {
        self.reassign.reset();
        self.lufs.reset(sample_rate);
        self.speech.prepare(sample_rate);
        self.speech.reset();
        let max_block = max_block.max(1);
        if self.mono.len() < max_block {
            self.mono.resize(max_block, 0.0);
            self.silence.resize(max_block, 0.0);
        }
        if self.worker.is_some() {
            self.start_worker(sample_rate);
        }
    }

    /// Start (or restart) the off-thread analysis worker for `sample_rate`. Spawns a thread, so it
    /// is only called from the host processing path — never from the audio-thread-core tests.
    pub fn start_worker(&mut self, sample_rate: f32) {
        self.worker = Some(AnalysisWorker::new(sample_rate as u32));
    }

    /// The latest heavy-analysis snapshot (voicing/pitch/flux/HNR), or the default when no worker is
    /// running. Allocation-free.
    pub fn latest_snapshot(&self) -> SignalSnapshot {
        self.worker
            .as_ref()
            .map_or_else(SignalSnapshot::default, AnalysisWorker::latest)
    }

    /// The editor reads frames from this ring (shared `Arc`).
    pub fn frame_ring(&self) -> &Arc<FrameRing> {
        &self.ring
    }

    /// The editor reads the meter snapshot from this cell (shared `Arc`).
    pub fn meter(&self) -> &Arc<MeterCell> {
        &self.meter
    }

    /// The editor reads the latest analysis-signal snapshot from this cell (shared `Arc`). The audio
    /// thread publishes `AnalysisWorker::latest()` into it each block.
    pub fn analysis(&self) -> &Arc<SignalCell> {
        &self.signal
    }

    /// Audio thread: tap the input block. Allocation-free; reads the input only.
    pub fn process(&mut self, input: AudioInputBuffer<'_>) {
        let Some(left) = input.left else { return };
        let n = left.len().min(self.mono.len());
        if n == 0 {
            return;
        }
        let right = input.right;

        // Mono downmix for the STFT / speech-presence path.
        match right {
            Some(r) => {
                for (i, (m, &l)) in self.mono[..n].iter_mut().zip(&left[..n]).enumerate() {
                    *m = 0.5 * (l + r.get(i).copied().unwrap_or(0.0));
                }
            }
            None => self.mono[..n].copy_from_slice(&left[..n]),
        }

        // Levels.
        let peak =
            peak_abs(&left[..n]).max(right.map(|r| peak_abs(&r[..n.min(r.len())])).unwrap_or(0.0));
        let rms_level = rms(&self.mono[..n]);
        let crest = if rms_level > 1.0e-12 {
            peak / rms_level
        } else {
            0.0
        };

        // BS.1770-4 LUFS (mono measures one channel against silence; stereo sums both).
        match right {
            Some(r) => self.lufs.push(&left[..n], &r[..n.min(r.len())]),
            None => self.lufs.push(&left[..n], &self.silence[..n]),
        }

        // Inline speech presence over the (clean) mono downmix, before the STFT clobbers it.
        let mut speech_presence = 0.0;
        for i in 0..n {
            speech_presence = self.speech.process(self.mono[i]);
        }

        // Hand the clean mono block to the off-thread heavy-analysis worker (allocation-free push),
        // then relay its latest snapshot into the editor-facing cell (atomic loads + stores only).
        if let Some(worker) = &self.worker {
            worker.push(&self.mono[..n]);
            self.signal.publish(&worker.latest());
        }

        // Reassignment frames (magnitude + frequency/time offsets) → ring (disjoint field borrows).
        // `ReassignStft` reads the mono block (it does not modify it) and emits per-bin lanes; the
        // magnitude lane is what the M3 spectrogram view consumes.
        let ring = &self.ring;
        self.reassign.process(&self.mono[..n], |frame| {
            ring.push_frame(frame.magnitudes, frame.freq_offsets, frame.time_offsets);
        });

        self.meter.publish(&MeterSnapshot {
            peak,
            rms: rms_level,
            crest,
            lufs_momentary: self.lufs.momentary(),
            lufs_short: self.lufs.short_term(),
            lufs_integrated: self.lufs.integrated(),
            speech_presence,
        });
    }
}

impl Default for CenedrilAnalysis {
    fn default() -> Self {
        Self::new(48_000.0, 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_produces_stft_frames_and_meter_for_a_sine() {
        let sr = 48_000.0;
        let mut analysis = CenedrilAnalysis::new(sr, 512);
        let freq = 3_000.0f32;
        let amp = 0.5f32;
        let block = 512usize;
        let total = (sr * 0.5) as usize;

        let mut left = vec![0.0f32; block];
        let mut right = vec![0.0f32; block];
        let mut i = 0;
        while i < total {
            for j in 0..block {
                let s = amp * (std::f32::consts::TAU * freq * (i + j) as f32 / sr).sin();
                left[j] = s;
                right[j] = s;
            }
            analysis.process(AudioInputBuffer::stereo(&left, &right));
            i += block;
        }

        let mut scratch = vec![0.0f32; 3 * analysis.frame_ring().bins()];
        let mut produced = 0usize;
        let mut last_peak_bin = 0usize;
        analysis
            .frame_ring()
            .drain_frames(&mut scratch, |_, lanes| {
                produced += 1;
                let mut best = (0usize, 0.0f32);
                for (bin, &v) in lanes.magnitudes.iter().enumerate() {
                    if v > best.1 {
                        best = (bin, v);
                    }
                }
                last_peak_bin = best.0;
            });

        assert!(produced > 0, "no STFT frames produced");
        let expected_bin = (freq / (sr / FRAME_SIZE as f32)).round() as i32;
        assert!(
            (last_peak_bin as i32 - expected_bin).abs() <= 2,
            "peak bin {last_peak_bin} vs expected {expected_bin}"
        );

        let meter = analysis.meter().read();
        assert!((meter.peak - amp).abs() < 0.05, "meter peak {}", meter.peak);
        assert!(
            (-30.0..0.0).contains(&meter.lufs_momentary),
            "lufs momentary {}",
            meter.lufs_momentary
        );
    }

    #[test]
    fn latest_snapshot_defaults_without_worker() {
        let analysis = CenedrilAnalysis::new(48_000.0, 512);
        assert_eq!(analysis.latest_snapshot(), SignalSnapshot::default());
    }

    #[test]
    fn meter_source_reads_published_cells() {
        use lindelion_ui::cenedril_vizia::MeterSource;

        let ring = Arc::new(FrameRing::new(8, 4));
        let meter = Arc::new(MeterCell::new());
        let signal = Arc::new(SignalCell::new());
        meter.publish(&MeterSnapshot {
            peak: 0.5,
            rms: 0.2,
            crest: 2.5,
            lufs_momentary: -18.0,
            lufs_short: -19.0,
            lufs_integrated: -20.0,
            speech_presence: 0.6,
        });
        signal.publish(&SignalSnapshot {
            pitch_hz: 147.0,
            pitch_confidence: 0.8,
            voicing_score: 0.7,
            voicing_state: 2.0,
            onset_flux_high: 0.4,
            spectral_flux: 0.5,
            hnr_db: 12.0,
        });

        let settings = Arc::new(SettingsCell::new());
        let source = CenedrilFrameSource::new(ring, meter, signal, settings, 48_000.0);
        let m = source.meters();
        assert_eq!(m.peak, 0.5);
        assert_eq!(m.crest, 2.5);
        assert_eq!(m.lufs_integrated, -20.0);
        let s = source.signals();
        assert_eq!(s.voicing_state, 2.0);
        assert_eq!(s.hnr_db, 12.0);
        assert_eq!(s.speech_presence, 0.6); // routed from the MeterCell into the signal readout
    }

    #[test]
    fn settings_store_reads_and_writes_through_the_cell() {
        use lindelion_ui::cenedril_vizia::SettingsStore;

        let settings = Arc::new(SettingsCell::new());
        let source = CenedrilFrameSource::new(
            Arc::new(FrameRing::new(8, 4)),
            Arc::new(MeterCell::new()),
            Arc::new(SignalCell::new()),
            settings.clone(),
            48_000.0,
        );
        // Defaults are read through the trait.
        assert_eq!(source.active_view(), 0);
        assert_eq!(source.db_floor(), -100.0);
        // Writes via the trait reach the shared cell, and reads see them.
        source.set_active_view(1);
        source.set_freq_scale(1);
        source.set_color_map(2);
        source.set_db_floor(-80.0);
        source.set_db_ceil(-6.0);
        assert_eq!(source.active_view(), 1);
        assert_eq!(source.freq_scale(), 1);
        assert_eq!(source.color_map(), 2);
        assert_eq!(source.db_floor(), -80.0);
        assert_eq!(source.db_ceil(), -6.0);
        assert_eq!(settings.get().active_view, 1); // same underlying cell
    }

    // Threaded + builds the SwiftF0 model — excluded from `make ci`; run via `make test-integration`.
    // The worker's own voicing correctness is covered in `lindelion-speech-signals`; this asserts
    // only that Cenedril's tap pushes audio to the worker and `latest_snapshot` reads it back.
    #[test]
    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration (spawns the analysis worker thread)"
    )]
    fn worker_reports_voiced_through_the_tap() {
        use std::{thread, time::Duration};

        let sr = 48_000.0;
        let mut analysis = CenedrilAnalysis::new(sr, 512);
        analysis.start_worker(sr);

        let voiced: Vec<f32> = (0..16_384)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / sr;
                0.5 * (150.0 * t).sin() + 0.25 * (300.0 * t).sin() + 0.12 * (450.0 * t).sin()
            })
            .collect();
        for chunk in voiced.chunks(512) {
            analysis.process(AudioInputBuffer::stereo(chunk, chunk));
        }

        let mut voiced_seen = false;
        for _ in 0..200 {
            if analysis.latest_snapshot().voicing_state == 2.0 {
                voiced_seen = true;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(voiced_seen, "worker never reported voiced through the tap");
    }
}
