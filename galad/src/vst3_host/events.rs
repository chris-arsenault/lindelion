//! Host-side VST3 event lists.

use std::{cell::Cell, mem, ptr};

use vst3::{Class, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use crate::midi::{MAX_BLOCK_MIDI_EVENTS, MidiMessage, MidiNoteEvent};

pub(super) struct HostEventList {
    len: Cell<usize>,
    events: [Cell<Event>; MAX_BLOCK_MIDI_EVENTS],
}

impl HostEventList {
    pub(super) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            len: Cell::new(0),
            events: std::array::from_fn(|_| Cell::new(empty_event())),
        })
    }

    pub(super) fn set_midi_messages(&self, messages: &[MidiMessage], ppq_position: f64) -> usize {
        let mut len = 0usize;
        for message in messages {
            if len >= self.events.len() {
                break;
            }
            if let Some(event) = midi_message_to_vst_event(*message, ppq_position) {
                self.events[len].set(event);
                len += 1;
            }
        }
        self.len.set(len);
        len
    }
}

impl Class for HostEventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for HostEventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.len.get().min(i32::MAX as usize) as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        if e.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let index = index as usize;
        if index >= self.len.get() {
            return kInvalidArgument;
        }
        *e = self.events[index].get();
        kResultOk
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        let len = self.len.get();
        if len >= self.events.len() {
            return kResultFalse;
        }
        self.events[len].set(*e);
        self.len.set(len + 1);
        kResultOk
    }
}

fn midi_message_to_vst_event(message: MidiMessage, ppq_position: f64) -> Option<Event> {
    let note = message.note_event()?;
    let mut event = empty_event();
    event.busIndex = 0;
    event.sampleOffset = 0;
    event.ppqPosition = ppq_position;
    event.flags = Event_::EventFlags_::kIsLive as uint16;
    match note {
        MidiNoteEvent::On {
            channel,
            note,
            velocity,
        } => {
            event.r#type = Event_::EventTypes_::kNoteOnEvent as uint16;
            event.__field0.noteOn = NoteOnEvent {
                channel: channel as int16,
                pitch: note as int16,
                tuning: 0.0,
                velocity: f32::from(velocity) / 127.0,
                length: 0,
                noteId: -1,
            };
        }
        MidiNoteEvent::Off {
            channel,
            note,
            velocity,
        } => {
            event.r#type = Event_::EventTypes_::kNoteOffEvent as uint16;
            event.__field0.noteOff = NoteOffEvent {
                channel: channel as int16,
                pitch: note as int16,
                velocity: f32::from(velocity) / 127.0,
                noteId: -1,
                tuning: 0.0,
            };
        }
    }
    Some(event)
}

fn empty_event() -> Event {
    unsafe { mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn event_list_maps_midi_note_messages() {
        let events = HostEventList::new();
        let iface = events.to_com_ptr::<IEventList>().expect("IEventList");
        let messages = [
            MidiMessage::from_bytes(0x90, 60, 100),
            MidiMessage::from_bytes(0x80, 60, 64),
        ];

        assert_eq!(events.set_midi_messages(&messages, 12.0), 2);
        assert_eq!(unsafe { iface.getEventCount() }, 2);

        let mut event = empty_event();
        assert_eq!(unsafe { iface.getEvent(0, &mut event) }, kResultOk);
        assert_eq!(event.r#type, Event_::EventTypes_::kNoteOnEvent as uint16);
        assert_eq!(unsafe { event.__field0.noteOn.pitch }, 60);
        assert!((unsafe { event.__field0.noteOn.velocity } - (100.0 / 127.0)).abs() < f32::EPSILON);

        assert_eq!(unsafe { iface.getEvent(1, &mut event) }, kResultOk);
        assert_eq!(event.r#type, Event_::EventTypes_::kNoteOffEvent as uint16);
        assert_eq!(unsafe { event.__field0.noteOff.pitch }, 60);
        assert_eq!(event.ppqPosition, 12.0);
    }

    #[test]
    fn event_list_ignores_non_note_messages() {
        let events = HostEventList::new();
        let messages = [MidiMessage::from_bytes(0xb0, 1, 127)];
        assert_eq!(events.set_midi_messages(&messages, 0.0), 0);
    }

    #[test]
    fn add_event_appends_until_capacity() {
        let events = HostEventList::new();
        let mut event = midi_message_to_vst_event(MidiMessage::from_bytes(0x90, 60, 1), 0.0)
            .expect("note event");
        assert_eq!(unsafe { events.addEvent(&mut event) }, kResultOk);
        assert_eq!(unsafe { events.getEventCount() }, 1);
        assert_eq!(
            unsafe { events.addEvent(ptr::null_mut()) },
            kInvalidArgument
        );
    }
}
