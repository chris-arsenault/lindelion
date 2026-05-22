#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    Note(NoteEvent),
    Control(ControlEvent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    On {
        channel: u8,
        note: u8,
        velocity: f32,
    },
    Off {
        channel: u8,
        note: u8,
        velocity: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlEvent {
    PitchBend {
        channel: u8,
        semitones: f32,
    },
    ChannelPressure {
        channel: u8,
        value: f32,
    },
    ContinuousController {
        channel: u8,
        controller: u8,
        value: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpressionStream {
    pub pitch_bend: f32,
    pub pressure: f32,
    pub brightness: f32,
    pub velocity: f32,
    pub gate: bool,
}

impl Default for ExpressionStream {
    fn default() -> Self {
        Self {
            pitch_bend: 0.0,
            pressure: 0.0,
            brightness: 0.0,
            velocity: 0.0,
            gate: false,
        }
    }
}

pub trait ExpressionSource {
    fn next_block(&mut self, voice_id: u32) -> ExpressionStream;
}
