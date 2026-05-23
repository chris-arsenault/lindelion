pub(crate) mod constants;
pub mod engine;
pub mod excitation;
pub mod modal;
pub mod voice;
pub mod waveguide;

pub use engine::{SynthEngine, VoiceSlotState};
pub use excitation::{
    ExcitationLayer, ExcitationPlayback, ExcitationSelector, MAX_EXCITATION_LAYERS,
    RuntimeExcitationSlot, SelectedExcitations, VoiceExcitation,
};
pub use modal::{ModalBank, ModalBankParams, ModalMode};
pub use voice::{Voice, VoiceExpression, VoiceTrigger};
pub use waveguide::{WaveguideParams, WaveguideResonator, WaveguideStyle};
