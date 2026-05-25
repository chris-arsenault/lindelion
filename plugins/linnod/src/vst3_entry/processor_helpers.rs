use lindelion_plugin_shell::{ControlEvent, MidiEvent};

pub(super) const fn empty_midi_event() -> MidiEvent {
    MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 0,
        value: 0.0,
    })
}
