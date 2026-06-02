pub(crate) mod constants;
pub(crate) mod energy_follower;
pub(crate) mod engine;
mod excitation;
pub(crate) mod master_stage;
pub(crate) mod modal;
mod shared_body;
mod sympathetic_chamber;
mod voice;
pub(crate) mod waveguide;

#[cfg(test)]
mod comparison_tests;
#[cfg(test)]
pub(crate) mod render_metrics;

pub use engine::SynthEngine;
pub use excitation::SelectedExcitations;
pub(crate) use excitation::{
    ExcitationSelector, LiveExcitationBlock, LiveExcitationLatchCapture, LiveExcitationPreRoll,
    MAX_EXCITATION_LAYERS, RuntimeExcitationSlot,
};
pub(crate) use master_stage::MasterStage;
pub(crate) use shared_body::{BodyStrike, SharedBody};
pub(crate) use sympathetic_chamber::SympatheticChamber;
pub use voice::Oversampler2x;
pub(crate) use voice::RESONATOR_OVERSAMPLING_LATENCY_SAMPLES;
pub(crate) use voice::ResonatorStack;
pub(crate) use voice::VoiceExpression;
pub use voice::VoiceTrigger;
pub(crate) use voice::velocity_to_gain;
pub use waveguide::WaveguideStyle;
