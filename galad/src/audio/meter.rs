//! Level/loudness metering — peak + RMS of an audio block, and a `MeterSnapshot` the UI displays.
//! Computed on the audio thread (cheap, allocation-free) and published off-thread (Step 2). RMS
//! stands in for "loudness" until a true ITU-R LUFS meter lands in `lindelion-dsp-utils`.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering, fence};

use lindelion_dsp_utils::analysis::{peak_abs, rms};

/// A snapshot of input/output levels for the UI meters.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct MeterSnapshot {
    pub input_left_peak: f32,
    pub input_right_peak: f32,
    pub input_left_rms: f32,
    pub input_right_rms: f32,
    pub input_peak: f32,
    pub input_rms: f32,
    pub output_left_peak: f32,
    pub output_right_peak: f32,
    pub output_left_rms: f32,
    pub output_right_rms: f32,
    pub output_peak: f32,
    pub output_rms: f32,
}

/// Left/right peak + RMS for an interleaved stereo block.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct StereoLevels {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_rms: f32,
    pub right_rms: f32,
}

impl MeterSnapshot {
    /// Update the input side from chain-width stereo levels.
    pub fn set_input_stereo(&mut self, levels: StereoLevels) {
        self.input_left_peak = levels.left_peak;
        self.input_right_peak = levels.right_peak;
        self.input_left_rms = levels.left_rms;
        self.input_right_rms = levels.right_rms;
        self.input_peak = levels.left_peak.max(levels.right_peak);
        self.input_rms = stereo_rms(levels.left_rms, levels.right_rms);
    }

    /// Update the output side from chain-width stereo levels.
    pub fn set_output_stereo(&mut self, levels: StereoLevels) {
        self.output_left_peak = levels.left_peak;
        self.output_right_peak = levels.right_peak;
        self.output_left_rms = levels.left_rms;
        self.output_right_rms = levels.right_rms;
        self.output_peak = levels.left_peak.max(levels.right_peak);
        self.output_rms = stereo_rms(levels.left_rms, levels.right_rms);
    }
}

/// Peak (max |x|) and RMS of `block`.
pub fn levels(block: &[f32]) -> (f32, f32) {
    (peak_abs(block), rms(block))
}

/// Peak and RMS for the left/right channels of an interleaved stereo block.
pub fn stereo_levels(block: &[f32]) -> StereoLevels {
    let frames = block.len() / 2;
    if frames == 0 {
        return StereoLevels::default();
    }

    let mut left_peak = 0.0f32;
    let mut right_peak = 0.0f32;
    let mut left_sum = 0.0f32;
    let mut right_sum = 0.0f32;
    for frame in block.chunks_exact(2) {
        let left = frame[0];
        let right = frame[1];
        left_peak = left_peak.max(left.abs());
        right_peak = right_peak.max(right.abs());
        left_sum += left * left;
        right_sum += right * right;
    }

    StereoLevels {
        left_peak,
        right_peak,
        left_rms: (left_sum / frames as f32).sqrt(),
        right_rms: (right_sum / frames as f32).sqrt(),
    }
}

fn stereo_rms(left: f32, right: f32) -> f32 {
    (0.5 * (left * left + right * right)).sqrt()
}

/// Seqlock cell: an even `version` means the data is stable; odd means a write is in progress.
struct MeterCell {
    version: AtomicU32,
    data: UnsafeCell<MeterSnapshot>,
}

// Safe: a single writer (audio thread) and single reader (UI thread) coordinate through `version`;
// the reader retries while a write is in progress, so it never observes a torn snapshot.
unsafe impl Sync for MeterCell {}

/// The audio-thread (writer) side of the meter snapshot. Wait-free, allocation-free.
pub struct MeterPublisher {
    cell: Arc<MeterCell>,
}

/// The UI (reader) side of the meter snapshot.
pub struct MeterReader {
    cell: Arc<MeterCell>,
}

/// A connected publisher/reader pair over one snapshot cell.
pub fn meter_channel() -> (MeterPublisher, MeterReader) {
    let cell = Arc::new(MeterCell {
        version: AtomicU32::new(0),
        data: UnsafeCell::new(MeterSnapshot::default()),
    });
    (MeterPublisher { cell: cell.clone() }, MeterReader { cell })
}

impl MeterPublisher {
    /// Publish the latest snapshot (wait-free; never blocks the audio thread).
    pub fn publish(&self, snapshot: MeterSnapshot) {
        let version = self.cell.version.load(Ordering::Relaxed);
        self.cell
            .version
            .store(version.wrapping_add(1), Ordering::Relaxed); // mark writing (odd)
        fence(Ordering::Release);
        unsafe {
            self.cell.data.get().write_volatile(snapshot);
        }
        self.cell
            .version
            .store(version.wrapping_add(2), Ordering::Release); // done (even)
    }
}

impl MeterReader {
    /// Read the latest consistent snapshot (retries while a write is in progress).
    pub fn read(&self) -> MeterSnapshot {
        loop {
            let before = self.cell.version.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let snapshot = unsafe { self.cell.data.get().read_volatile() };
            fence(Ordering::Acquire);
            let after = self.cell.version.load(Ordering::Relaxed);
            if before == after {
                return snapshot;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_of_known_block() {
        let block = [0.5f32, -0.8, 0.3, -0.1];
        let (peak, level) = levels(&block);

        assert!((peak - 0.8).abs() < 1e-6);
        let expected = ((0.25 + 0.64 + 0.09 + 0.01) / 4.0_f32).sqrt();
        assert!((level - expected).abs() < 1e-6);
    }

    #[test]
    fn levels_of_silence_are_zero() {
        let (peak, level) = levels(&[0.0f32; 8]);
        assert_eq!(peak, 0.0);
        assert_eq!(level, 0.0);
    }

    #[test]
    fn meter_snapshot_round_trips() {
        let (publisher, reader) = meter_channel();
        let snap = MeterSnapshot {
            input_left_peak: 0.4,
            input_right_peak: 0.5,
            input_peak: 0.5,
            input_rms: 0.3,
            output_left_peak: 0.7,
            output_right_peak: 0.9,
            output_peak: 0.9,
            output_rms: 0.7,
            ..Default::default()
        };
        publisher.publish(snap);
        assert_eq!(reader.read(), snap);

        let snap2 = MeterSnapshot {
            input_peak: 0.1,
            ..Default::default()
        };
        publisher.publish(snap2);
        assert_eq!(reader.read(), snap2);
    }

    #[test]
    fn publish_is_allocation_free() {
        let (publisher, _reader) = meter_channel();
        let snap = MeterSnapshot {
            output_peak: 0.5,
            ..Default::default()
        };
        lindelion_test_allocator::assert_no_allocations("meter publish", || {
            publisher.publish(snap);
        });
    }

    #[test]
    fn stereo_levels_are_split_by_channel() {
        let block = [0.1f32, -0.6, -0.5, 0.2];
        let levels = stereo_levels(&block);

        assert_eq!(levels.left_peak, 0.5);
        assert_eq!(levels.right_peak, 0.6);
        assert!((levels.left_rms - ((0.01 + 0.25) / 2.0_f32).sqrt()).abs() < 1e-6);
        assert!((levels.right_rms - ((0.36 + 0.04) / 2.0_f32).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn snapshot_keeps_lr_and_aggregate_levels() {
        let mut snapshot = MeterSnapshot::default();
        snapshot.set_input_stereo(StereoLevels {
            left_peak: 0.2,
            right_peak: 0.8,
            left_rms: 0.1,
            right_rms: 0.3,
        });

        assert_eq!(snapshot.input_left_peak, 0.2);
        assert_eq!(snapshot.input_right_peak, 0.8);
        assert_eq!(snapshot.input_peak, 0.8);
        assert!((snapshot.input_rms - ((0.01 + 0.09) * 0.5_f32).sqrt()).abs() < 1e-6);
    }
}
