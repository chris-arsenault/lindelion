pub(crate) mod constants;
pub(crate) mod engine;
mod excitation;
pub(crate) mod modal;
mod voice;
pub(crate) mod waveguide;

pub use engine::SynthEngine;
pub use excitation::SelectedExcitations;
pub(crate) use excitation::{
    ExcitationSelector, LiveExcitationBlock, LiveExcitationLatchCapture, LiveExcitationPreRoll,
    MAX_EXCITATION_LAYERS, RuntimeExcitationSlot,
};
pub(crate) use voice::VoiceExpression;
pub use voice::VoiceTrigger;
pub use waveguide::WaveguideStyle;
