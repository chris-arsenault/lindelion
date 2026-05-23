use lindelion_plugin_shell::{ControlEvent, MidiControllerRoute, MidiEvent};
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
