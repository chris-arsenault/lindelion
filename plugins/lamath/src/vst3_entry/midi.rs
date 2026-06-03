use lindelion_plugin_shell::{ControlEvent, MidiControllerRoute, MidiEvent};
use vst3::Steinberg::Vst::*;

use crate::{RESONATOR_BRIGHTNESS_CONTROLLER, RESONATOR_PRESSURE_CONTROLLER};

pub(super) const RESONATOR_MIDI_CONTROLLER_ROUTES: &[MidiControllerRoute] = &[
    MidiControllerRoute::new(
        controller_number(ControllerNumbers_::kCtrlModWheel),
        RESONATOR_PRESSURE_CONTROLLER,
    ),
    MidiControllerRoute::new(
        controller_number(ControllerNumbers_::kCtrlFilterResonance),
        RESONATOR_BRIGHTNESS_CONTROLLER,
    ),
];

#[cfg(windows)]
const fn controller_number(value: i32) -> u32 {
    value as u32
}

#[cfg(not(windows))]
const fn controller_number(value: u32) -> u32 {
    value
}

pub(super) const fn empty_midi_event() -> MidiEvent {
    MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 0,
        value: 0.0,
    })
}
