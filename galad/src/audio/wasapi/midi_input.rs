//! Windows MIDI input via WinMM.
//!
//! This is intentionally small: open every short-message MIDI input device, push packed messages
//! into Galad's fixed MIDI queue, and close devices before the queue is dropped.

use std::mem;
use std::sync::Arc;

use windows::Win32::Media::Audio::{
    CALLBACK_FUNCTION, HMIDIIN, MIDIINCAPSW, midiInClose, midiInGetDevCapsW, midiInGetNumDevs,
    midiInOpen, midiInReset, midiInStart, midiInStop,
};
use windows::Win32::Media::Multimedia::HDRVR;
use windows::Win32::Media::{MM_MIM_DATA, MMSYSERR_NOERROR};

use crate::midi::{MidiEventQueue, MidiMessage};

pub(super) struct MidiInputs {
    ports: Vec<MidiInputPort>,
    _queue: Arc<MidiEventQueue>,
}

impl MidiInputs {
    pub(super) fn open_all(queue: Arc<MidiEventQueue>) -> Self {
        let count = unsafe { midiInGetNumDevs() };
        if count == 0 {
            crate::diagnostics::log("midi: no WinMM input devices found");
            return Self {
                ports: Vec::new(),
                _queue: queue,
            };
        }

        let mut ports = Vec::new();
        for id in 0..count {
            match MidiInputPort::open(id, queue.clone()) {
                Ok(port) => {
                    crate::diagnostics::log(format!(
                        "midi: opened input id={id} name={:?}",
                        port.name
                    ));
                    ports.push(port);
                }
                Err(error) => {
                    crate::diagnostics::log(format!("midi: open input id={id} failed: {error}"));
                }
            }
        }
        crate::diagnostics::log(format!("midi: active inputs={}", ports.len()));
        Self {
            ports,
            _queue: queue,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.ports.len()
    }
}

struct MidiInputPort {
    handle: HMIDIIN,
    name: String,
}

impl MidiInputPort {
    fn open(id: u32, queue: Arc<MidiEventQueue>) -> Result<Self, String> {
        let name = midi_input_name(id);
        let mut handle = HMIDIIN::default();
        let queue_ptr = Arc::as_ptr(&queue) as usize;
        let result = unsafe {
            midiInOpen(
                &mut handle,
                id,
                Some(midi_callback as *const () as usize),
                Some(queue_ptr),
                CALLBACK_FUNCTION,
            )
        };
        if result != MMSYSERR_NOERROR {
            return Err(format!("midiInOpen result={result}"));
        }
        let start = unsafe { midiInStart(handle) };
        if start != MMSYSERR_NOERROR {
            unsafe {
                let _ = midiInClose(handle);
            }
            return Err(format!("midiInStart result={start}"));
        }

        Ok(Self { handle, name })
    }
}

impl Drop for MidiInputPort {
    fn drop(&mut self) {
        unsafe {
            crate::diagnostics::log(format!("midi: closing input name={:?}", self.name));
            let _ = midiInStop(self.handle);
            let _ = midiInReset(self.handle);
            let _ = midiInClose(self.handle);
        }
    }
}

unsafe extern "system" fn midi_callback(
    _hdrvr: HDRVR,
    message: u32,
    user: usize,
    param1: usize,
    _param2: usize,
) {
    if message != MM_MIM_DATA || user == 0 {
        return;
    }
    let queue = unsafe { &*(user as *const MidiEventQueue) };
    let _ = queue.push(MidiMessage::from_raw(param1 as u32));
}

fn midi_input_name(id: u32) -> String {
    let mut caps = MIDIINCAPSW::default();
    let result =
        unsafe { midiInGetDevCapsW(id as usize, &mut caps, mem::size_of::<MIDIINCAPSW>() as u32) };
    if result != MMSYSERR_NOERROR {
        return format!("MIDI Input {id}");
    }
    let raw_name = unsafe { std::ptr::addr_of!(caps.szPname).read_unaligned() };
    let end = raw_name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(raw_name.len());
    String::from_utf16_lossy(&raw_name[..end])
}
