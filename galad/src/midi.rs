//! Minimal MIDI plumbing for Galad.
//!
//! The Windows backend receives packed WinMM short messages and pushes them into this fixed ring.
//! The audio thread drains the ring once per render block and converts supported messages to VST3
//! events. No heap allocation is required in the callback or audio-thread paths.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub const MIDI_QUEUE_CAPACITY: usize = 1024;
pub const MAX_BLOCK_MIDI_EVENTS: usize = 128;

/// One packed MIDI 1.0 short message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiMessage {
    raw: u32,
}

impl MidiMessage {
    pub const fn from_raw(raw: u32) -> Self {
        Self {
            raw: raw & 0x00ff_ffff,
        }
    }

    #[cfg(test)]
    pub const fn from_bytes(status: u8, data1: u8, data2: u8) -> Self {
        Self::from_raw(status as u32 | ((data1 as u32) << 8) | ((data2 as u32) << 16))
    }

    pub const fn raw(self) -> u32 {
        self.raw
    }

    pub const fn status(self) -> u8 {
        (self.raw & 0xff) as u8
    }

    pub const fn data1(self) -> u8 {
        ((self.raw >> 8) & 0xff) as u8
    }

    pub const fn data2(self) -> u8 {
        ((self.raw >> 16) & 0xff) as u8
    }

    pub const fn channel(self) -> u8 {
        self.status() & 0x0f
    }

    pub const fn kind(self) -> u8 {
        self.status() & 0xf0
    }

    pub fn note_event(self) -> Option<MidiNoteEvent> {
        let note = self.data1().min(127);
        let velocity = self.data2().min(127);
        match self.kind() {
            0x80 => Some(MidiNoteEvent::Off {
                channel: self.channel(),
                note,
                velocity,
            }),
            0x90 if velocity == 0 => Some(MidiNoteEvent::Off {
                channel: self.channel(),
                note,
                velocity: 0,
            }),
            0x90 => Some(MidiNoteEvent::On {
                channel: self.channel(),
                note,
                velocity,
            }),
            _ => None,
        }
    }
}

impl Default for MidiMessage {
    fn default() -> Self {
        Self::from_raw(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiNoteEvent {
    On { channel: u8, note: u8, velocity: u8 },
    Off { channel: u8, note: u8, velocity: u8 },
}

/// Fixed-capacity single-producer/single-consumer ring for short MIDI messages.
pub struct MidiEventQueue<const CAP: usize = MIDI_QUEUE_CAPACITY> {
    read: AtomicUsize,
    write: AtomicUsize,
    dropped: AtomicUsize,
    events: [AtomicU32; CAP],
}

impl<const CAP: usize> MidiEventQueue<CAP> {
    pub fn new() -> Self {
        assert!(CAP > 1);
        Self {
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            events: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    pub fn push(&self, message: MidiMessage) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        let next = (write + 1) % CAP;
        if next == self.read.load(Ordering::Acquire) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.events[write].store(message.raw(), Ordering::Relaxed);
        self.write.store(next, Ordering::Release);
        true
    }

    pub fn drain(&self, target: &mut [MidiMessage]) -> usize {
        let mut read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        let mut count = 0usize;
        while read != write && count < target.len() {
            target[count] = MidiMessage::from_raw(self.events[read].load(Ordering::Relaxed));
            count += 1;
            read = (read + 1) % CAP;
        }
        self.read.store(read, Ordering::Release);
        count
    }

    #[cfg(test)]
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl<const CAP: usize> Default for MidiEventQueue<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_message_decodes_note_on_and_off() {
        assert_eq!(
            MidiMessage::from_bytes(0x91, 64, 100).note_event(),
            Some(MidiNoteEvent::On {
                channel: 1,
                note: 64,
                velocity: 100,
            })
        );
        assert_eq!(
            MidiMessage::from_bytes(0x81, 64, 32).note_event(),
            Some(MidiNoteEvent::Off {
                channel: 1,
                note: 64,
                velocity: 32,
            })
        );
        assert_eq!(
            MidiMessage::from_bytes(0x91, 64, 0).note_event(),
            Some(MidiNoteEvent::Off {
                channel: 1,
                note: 64,
                velocity: 0,
            })
        );
    }

    #[test]
    fn midi_queue_drains_in_order_and_drops_when_full() {
        let queue = MidiEventQueue::<3>::new();
        assert!(queue.push(MidiMessage::from_bytes(0x90, 60, 100)));
        assert!(queue.push(MidiMessage::from_bytes(0x80, 60, 0)));
        assert!(!queue.push(MidiMessage::from_bytes(0x90, 62, 100)));
        assert_eq!(queue.dropped(), 1);

        let mut out = [MidiMessage::default(); 4];
        let count = queue.drain(&mut out);
        assert_eq!(count, 2);
        assert_eq!(out[0], MidiMessage::from_bytes(0x90, 60, 100));
        assert_eq!(out[1], MidiMessage::from_bytes(0x80, 60, 0));
        assert_eq!(queue.drain(&mut out), 0);
    }

    #[test]
    fn midi_queue_drain_is_allocation_free() {
        let queue = MidiEventQueue::<16>::new();
        assert!(queue.push(MidiMessage::from_bytes(0x90, 60, 100)));
        let mut out = [MidiMessage::default(); 8];
        lindelion_test_allocator::assert_no_allocations("midi queue drain", || {
            assert_eq!(queue.drain(&mut out), 1);
        });
    }
}
