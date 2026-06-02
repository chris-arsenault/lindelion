//! Lock-free SPSC hand-off from the audio-thread analysis tap to the editor: a ring of STFT
//! magnitude frames and an atomic meter snapshot. Applies the same lock-free discipline as
//! `lindelion-dsp-utils::handoff`'s `SampleRing` (single producer / single consumer, `f32` stored
//! through [`AtomicF32`], `Release`/`Acquire` on the indices, power-of-two mask, **lossy on
//! overflow**) but at frame granularity — a distinct structure, so it builds on the shared
//! [`AtomicF32`] cell rather than the sample ring. The producer (audio thread) never allocates; the
//! consumer (editor) loads frames into a caller-owned scratch buffer.

use std::sync::atomic::{AtomicUsize, Ordering};

use lindelion_dsp_utils::handoff::AtomicF32;
use lindelion_speech_signals::SignalSnapshot;

/// Per-bin lanes carried by each frame: STFT magnitude, the reassignment **frequency** offset
/// (fractional bins added to the bin index), and the reassignment **time** offset (samples relative
/// to the window center). Yielded as borrowed sub-slices of the consumer's drain scratch.
pub struct FrameLanes<'a> {
    pub magnitudes: &'a [f32],
    pub freq_offsets: &'a [f32],
    pub time_offsets: &'a [f32],
}

/// Number of per-bin lanes stored per frame (magnitude, frequency offset, time offset).
const LANES: usize = 3;

pub struct FrameRing {
    bins: usize,
    /// `LANES * bins` — the f32 count per frame slot.
    slot_stride: usize,
    slots_mask: usize,
    buffer: Box<[AtomicF32]>,
    write: AtomicUsize,
    read: AtomicUsize,
}

impl FrameRing {
    /// `slots_pow2` (the number of frame slots) must be a power of two; each slot holds `LANES *
    /// bins` values (the magnitude, frequency-offset, and time-offset lanes, planar). All storage is
    /// allocated here, off the audio thread.
    pub fn new(bins: usize, slots_pow2: usize) -> Self {
        assert!(
            slots_pow2.is_power_of_two(),
            "frame slots must be a power of two"
        );
        let slot_stride = LANES * bins;
        let buffer = (0..slots_pow2 * slot_stride)
            .map(|_| AtomicF32::default())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            bins,
            slot_stride,
            slots_mask: slots_pow2 - 1,
            buffer,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    pub fn bins(&self) -> usize {
        self.bins
    }

    /// Producer (audio thread): publish one reassignment frame (three per-bin lanes, stored planar).
    /// Allocation-free. Extra/missing values beyond `bins()` in any lane are ignored.
    pub fn push_frame(&self, magnitudes: &[f32], freq_offsets: &[f32], time_offsets: &[f32]) {
        let w = self.write.load(Ordering::Relaxed);
        let base = (w & self.slots_mask) * self.slot_stride;
        for (lane, src) in [magnitudes, freq_offsets, time_offsets]
            .into_iter()
            .enumerate()
        {
            let off = base + lane * self.bins;
            let n = src.len().min(self.bins);
            for (slot, &v) in self.buffer[off..off + n].iter().zip(&src[..n]) {
                slot.store(v);
            }
        }
        self.write.store(w.wrapping_add(1), Ordering::Release);
    }

    /// Consumer (editor thread): deliver frames written since the last drain, oldest first, loading
    /// each into `scratch` (len ≥ `LANES * bins()`) and splitting it into the three lanes. Lossy: if
    /// the consumer lagged more than the ring depth, the oldest dropped frames are skipped so only
    /// the most recent contiguous run is delivered. Returns the number of frames delivered.
    /// Allocation-free.
    pub fn drain_frames(
        &self,
        scratch: &mut [f32],
        mut on_frame: impl FnMut(u64, FrameLanes),
    ) -> usize {
        let slots = self.slots_mask + 1;
        let w = self.write.load(Ordering::Acquire);
        let mut r = self.read.load(Ordering::Relaxed);
        if w.wrapping_sub(r) > slots {
            r = w.wrapping_sub(slots);
        }
        let available = w.wrapping_sub(r);
        let bins = (scratch.len() / LANES).min(self.bins);
        for k in 0..available {
            let idx = r.wrapping_add(k);
            let base = (idx & self.slots_mask) * self.slot_stride;
            for lane in 0..LANES {
                let off = base + lane * self.bins;
                let dst = lane * bins;
                for (d, s) in scratch[dst..dst + bins]
                    .iter_mut()
                    .zip(&self.buffer[off..off + bins])
                {
                    *d = s.load();
                }
            }
            let (magnitudes, rest) = scratch.split_at(bins);
            let (freq_offsets, rest) = rest.split_at(bins);
            let time_offsets = &rest[..bins];
            on_frame(
                idx as u64,
                FrameLanes {
                    magnitudes,
                    freq_offsets,
                    time_offsets,
                },
            );
        }
        self.read.store(w, Ordering::Release);
        available
    }
}

/// A level/loudness meter readout. Published by the audio thread, read by the editor.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeterSnapshot {
    pub peak: f32,
    pub rms: f32,
    pub crest: f32,
    pub lufs_momentary: f32,
    pub lufs_short: f32,
    pub lufs_integrated: f32,
    pub speech_presence: f32,
}

/// Atomic single-slot hand-off for the meter snapshot (per-field `AtomicU32`, like the
/// `speech/signals` worker's snapshot). Reads may be cosmetically torn across fields, which is
/// acceptable for meters; both publish and read are allocation-free.
pub struct MeterCell {
    peak: AtomicF32,
    rms: AtomicF32,
    crest: AtomicF32,
    lufs_momentary: AtomicF32,
    lufs_short: AtomicF32,
    lufs_integrated: AtomicF32,
    speech_presence: AtomicF32,
}

impl MeterCell {
    pub fn new() -> Self {
        Self {
            peak: AtomicF32::default(),
            rms: AtomicF32::default(),
            crest: AtomicF32::default(),
            lufs_momentary: AtomicF32::default(),
            lufs_short: AtomicF32::default(),
            lufs_integrated: AtomicF32::default(),
            speech_presence: AtomicF32::default(),
        }
    }

    pub fn publish(&self, snapshot: &MeterSnapshot) {
        self.peak.store(snapshot.peak);
        self.rms.store(snapshot.rms);
        self.crest.store(snapshot.crest);
        self.lufs_momentary.store(snapshot.lufs_momentary);
        self.lufs_short.store(snapshot.lufs_short);
        self.lufs_integrated.store(snapshot.lufs_integrated);
        self.speech_presence.store(snapshot.speech_presence);
    }

    pub fn read(&self) -> MeterSnapshot {
        MeterSnapshot {
            peak: self.peak.load(),
            rms: self.rms.load(),
            crest: self.crest.load(),
            lufs_momentary: self.lufs_momentary.load(),
            lufs_short: self.lufs_short.load(),
            lufs_integrated: self.lufs_integrated.load(),
            speech_presence: self.speech_presence.load(),
        }
    }
}

impl Default for MeterCell {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic single-slot hand-off for the worker's analysis [`SignalSnapshot`] (per-field `AtomicF32`,
/// exactly like [`MeterCell`]). Published on the audio thread from `AnalysisWorker::latest()`, read by
/// the editor. Reads may be cosmetically torn across fields, which is acceptable for a readout; both
/// publish and read are allocation-free.
pub struct SignalCell {
    pitch_hz: AtomicF32,
    pitch_confidence: AtomicF32,
    voicing_score: AtomicF32,
    voicing_state: AtomicF32,
    onset_flux_high: AtomicF32,
    spectral_flux: AtomicF32,
    hnr_db: AtomicF32,
}

impl SignalCell {
    pub fn new() -> Self {
        Self {
            pitch_hz: AtomicF32::default(),
            pitch_confidence: AtomicF32::default(),
            voicing_score: AtomicF32::default(),
            voicing_state: AtomicF32::default(),
            onset_flux_high: AtomicF32::default(),
            spectral_flux: AtomicF32::default(),
            hnr_db: AtomicF32::default(),
        }
    }

    pub fn publish(&self, snapshot: &SignalSnapshot) {
        self.pitch_hz.store(snapshot.pitch_hz);
        self.pitch_confidence.store(snapshot.pitch_confidence);
        self.voicing_score.store(snapshot.voicing_score);
        self.voicing_state.store(snapshot.voicing_state);
        self.onset_flux_high.store(snapshot.onset_flux_high);
        self.spectral_flux.store(snapshot.spectral_flux);
        self.hnr_db.store(snapshot.hnr_db);
    }

    pub fn read(&self) -> SignalSnapshot {
        SignalSnapshot {
            pitch_hz: self.pitch_hz.load(),
            pitch_confidence: self.pitch_confidence.load(),
            voicing_score: self.voicing_score.load(),
            voicing_state: self.voicing_state.load(),
            onset_flux_high: self.onset_flux_high.load(),
            spectral_flux: self.spectral_flux.load(),
            hnr_db: self.hnr_db.load(),
        }
    }
}

impl Default for SignalCell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_drain_in_order_with_correct_magnitudes() {
        let ring = FrameRing::new(4, 8);
        let z = [0.0f32; 4];
        ring.push_frame(&[1.0, 2.0, 3.0, 4.0], &z, &z);
        ring.push_frame(&[5.0, 6.0, 7.0, 8.0], &z, &z);
        ring.push_frame(&[9.0, 10.0, 11.0, 12.0], &z, &z);

        let mut scratch = [0.0f32; 12]; // 3 lanes * 4 bins
        let mut got: Vec<(u64, Vec<f32>)> = Vec::new();
        let n = ring.drain_frames(&mut scratch, |idx, lanes| {
            got.push((idx, lanes.magnitudes.to_vec()))
        });

        assert_eq!(n, 3);
        assert_eq!(got[0], (0, vec![1.0, 2.0, 3.0, 4.0]));
        assert_eq!(got[1], (1, vec![5.0, 6.0, 7.0, 8.0]));
        assert_eq!(got[2], (2, vec![9.0, 10.0, 11.0, 12.0]));

        // A second drain with nothing new returns zero.
        assert_eq!(ring.drain_frames(&mut scratch, |_, _| {}), 0);
    }

    #[test]
    fn frames_carry_three_reassignment_lanes_in_order() {
        let ring = FrameRing::new(2, 4);
        ring.push_frame(&[1.0, 2.0], &[0.1, 0.2], &[-1.0, -2.0]);
        ring.push_frame(&[3.0, 4.0], &[0.3, 0.4], &[-3.0, -4.0]);

        let mut scratch = [0.0f32; 6]; // 3 lanes * 2 bins
        let mut seen = 0usize;
        let n = ring.drain_frames(&mut scratch, |_idx, lanes| {
            match seen {
                0 => {
                    assert_eq!(lanes.magnitudes, [1.0, 2.0].as_slice());
                    assert_eq!(lanes.freq_offsets, [0.1, 0.2].as_slice());
                    assert_eq!(lanes.time_offsets, [-1.0, -2.0].as_slice());
                }
                1 => {
                    assert_eq!(lanes.magnitudes, [3.0, 4.0].as_slice());
                    assert_eq!(lanes.freq_offsets, [0.3, 0.4].as_slice());
                    assert_eq!(lanes.time_offsets, [-3.0, -4.0].as_slice());
                }
                other => panic!("unexpected frame {other}"),
            }
            seen += 1;
        });
        assert_eq!(n, 2);
        assert_eq!(seen, 2);
    }

    #[test]
    fn overflow_drops_oldest_and_keeps_recent_contiguous() {
        let ring = FrameRing::new(2, 4); // depth 4
        for i in 0..10u32 {
            ring.push_frame(&[i as f32, i as f32], &[0.0, 0.0], &[0.0, 0.0]);
        }
        let mut scratch = [0.0f32; 6]; // 3 lanes * 2 bins
        let mut idxs = Vec::new();
        let n = ring.drain_frames(&mut scratch, |idx, _| idxs.push(idx));
        assert_eq!(n, 4);
        assert_eq!(idxs, vec![6, 7, 8, 9]); // only the most recent depth-4 run survives
    }

    #[test]
    fn push_frame_is_allocation_free() {
        let ring = FrameRing::new(1025, 512);
        let frame = vec![0.5f32; 1025];
        crate::assert_no_allocations("frame push", || {
            ring.push_frame(&frame, &frame, &frame);
        });
    }

    #[test]
    fn meter_snapshot_round_trips() {
        let cell = MeterCell::new();
        let snap = MeterSnapshot {
            peak: 0.9,
            rms: 0.3,
            crest: 3.0,
            lufs_momentary: -14.0,
            lufs_short: -15.0,
            lufs_integrated: -16.0,
            speech_presence: 0.7,
        };
        cell.publish(&snap);
        assert_eq!(cell.read(), snap);
    }

    #[test]
    fn meter_publish_is_allocation_free() {
        let cell = MeterCell::new();
        let snap = MeterSnapshot::default();
        crate::assert_no_allocations("meter publish", || {
            cell.publish(&snap);
        });
    }

    #[test]
    fn signal_snapshot_round_trips() {
        let cell = SignalCell::new();
        let snap = SignalSnapshot {
            pitch_hz: 147.0,
            pitch_confidence: 0.8,
            voicing_score: 0.7,
            voicing_state: 2.0,
            onset_flux_high: 0.4,
            spectral_flux: 0.5,
            hnr_db: 12.0,
        };
        cell.publish(&snap);
        assert_eq!(cell.read(), snap);
    }

    #[test]
    fn signal_cell_defaults_to_empty_snapshot() {
        assert_eq!(SignalCell::new().read(), SignalSnapshot::default());
    }

    #[test]
    fn signal_publish_is_allocation_free() {
        let cell = SignalCell::new();
        let snap = SignalSnapshot::default();
        crate::assert_no_allocations("signal publish", || {
            cell.publish(&snap);
        });
    }
}
