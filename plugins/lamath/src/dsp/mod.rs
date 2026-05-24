pub(crate) mod constants;
mod engine;
mod excitation;
mod modal;
mod voice;
mod waveguide;

pub(crate) use engine::SynthEngine;
pub(crate) use excitation::{
    ExcitationSelector, MAX_EXCITATION_LAYERS, RuntimeExcitationSlot, SelectedExcitations,
};
pub(crate) use voice::{VoiceExpression, VoiceTrigger};
pub use waveguide::WaveguideStyle;
