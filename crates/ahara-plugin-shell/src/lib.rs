pub mod events;
pub mod parameters;
pub mod process;
pub mod state;

pub use events::{
    ControlEvent, ExpressionSource, ExpressionStream, MIDI_CHANNEL_COUNT, ManualExpressionSource,
    MidiEvent, MidiExpressionSource, MidiExpressionUpdate, MidiVoiceExpression, NoteEvent,
};
pub use parameters::{AtomicParameter, ParameterFlags, ParameterId, ParameterInfo, ParameterRange};
pub use process::{AudioBuffer, AudioPlugin, ProcessContext, ProcessMode, ProcessSetup};
pub use state::{PluginState, StateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCategory {
    Instrument,
    Effect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub name: &'static str,
    pub vendor: &'static str,
    pub url: &'static str,
    pub email: &'static str,
    pub version: &'static str,
    pub category: PluginCategory,
    pub class_id: [u8; 16],
}

impl PluginDescriptor {
    pub const fn instrument(name: &'static str, class_id: [u8; 16]) -> Self {
        Self {
            name,
            vendor: "Ahara",
            url: "https://ahara.io",
            email: "",
            version: env!("CARGO_PKG_VERSION"),
            category: PluginCategory::Instrument,
            class_id,
        }
    }
}
