use ahara_plugin_shell::{
    ControlEvent, HostMidiEvent, MidiControllerRoute, MidiEvent, MidiEventNormalizer,
};
use vst3::Steinberg::Vst::*;

use crate::{RESONATOR_BRIGHTNESS_CONTROLLER, RESONATOR_MOD_WHEEL_CONTROLLER};

pub(super) const RESONATOR_MIDI_CONTROLLER_ROUTES: &[MidiControllerRoute] = &[
    MidiControllerRoute::new(
        ControllerNumbers_::kCtrlModWheel,
        RESONATOR_MOD_WHEEL_CONTROLLER,
    ),
    MidiControllerRoute::new(
        ControllerNumbers_::kCtrlFilterResonance,
        RESONATOR_BRIGHTNESS_CONTROLLER,
    ),
];

pub(super) const fn empty_midi_event() -> MidiEvent {
    MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 0,
        value: 0.0,
    })
}

pub(super) unsafe fn vst_event_to_midi(
    event: Event,
    normalizer: MidiEventNormalizer<'_>,
) -> Option<MidiEvent> {
    normalizer.normalize(vst_event_to_host_midi(event)?)
}

pub(super) unsafe fn vst_event_to_host_midi(event: Event) -> Option<HostMidiEvent> {
    match event.r#type as Event_::EventTypes {
        Event_::EventTypes_::kNoteOnEvent => {
            let note = event.__field0.noteOn;
            Some(HostMidiEvent::NoteOn {
                channel: i32::from(note.channel),
                note: i32::from(note.pitch),
                velocity: note.velocity,
            })
        }
        Event_::EventTypes_::kNoteOffEvent => {
            let note = event.__field0.noteOff;
            Some(HostMidiEvent::NoteOff {
                channel: i32::from(note.channel),
                note: i32::from(note.pitch),
                velocity: note.velocity,
            })
        }
        Event_::EventTypes_::kPolyPressureEvent => {
            let pressure = event.__field0.polyPressure;
            Some(HostMidiEvent::PolyPressure {
                channel: i32::from(pressure.channel),
                note: i32::from(pressure.pitch),
                pressure: pressure.pressure,
            })
        }
        Event_::EventTypes_::kLegacyMIDICCOutEvent => {
            legacy_midi_cc_to_host_event(event.__field0.midiCCOut)
        }
        _ => None,
    }
}

fn legacy_midi_cc_to_host_event(event: LegacyMIDICCOutEvent) -> Option<HostMidiEvent> {
    let channel = i32::from(event.channel);
    match u32::from(event.controlNumber) {
        ControllerNumbers_::kAfterTouch => Some(HostMidiEvent::ChannelPressure {
            channel,
            value: i32::from(event.value),
        }),
        ControllerNumbers_::kPitchBend => Some(HostMidiEvent::PitchBend {
            channel,
            lsb: i32::from(event.value),
            msb: i32::from(event.value2),
        }),
        control_number => Some(HostMidiEvent::ContinuousController {
            channel,
            controller: control_number,
            value: i32::from(event.value),
        }),
    }
}
