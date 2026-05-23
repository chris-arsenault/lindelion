pub mod events;
pub mod parameters;
pub mod patch_io;
pub mod process;
pub mod state;
pub mod voices;
pub mod vst3;

pub use events::{
    ControlEvent, ExpressionSource, ExpressionStream, HostMidiEvent, MIDI_CHANNEL_COUNT,
    ManualExpressionSource, MidiControllerRoute, MidiEvent, MidiEventNormalizer,
    MidiExpressionControl, MidiExpressionControlRoute, MidiExpressionMapping, MidiExpressionSource,
    MidiExpressionUpdate, MidiVoiceExpression, NoteEvent,
};
pub use parameters::{
    AtomicParameter, ParameterFlags, ParameterId, ParameterInfo, ParameterRange,
    PlainToSmoothedValue, SmoothedAtomicParam, SmoothedAtomicParamSpec,
};
pub use patch_io::{NoPatchMigration, TomlPatchError, TomlPatchFormat, TomlPatchMigration};
pub use process::{AudioBuffer, AudioPlugin, ProcessContext, ProcessMode, ProcessSetup};
pub use state::{PluginState, StateError};
pub use voices::{
    ManagedVoiceExpression, VoiceLike, VoiceManager, VoiceRenderStatus, VoiceSlotState,
};

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
