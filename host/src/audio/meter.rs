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
    pub input_peak: f32,
    pub input_rms: f32,
    pub output_peak: f32,
    pub output_rms: f32,
}

/// Peak (max |x|) and RMS of `block`.
pub fn levels(block: &[f32]) -> (f32, f32) {
    (peak_abs(block), rms(block))
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
            input_peak: 0.5,
            input_rms: 0.3,
            output_peak: 0.9,
            output_rms: 0.7,
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
}
