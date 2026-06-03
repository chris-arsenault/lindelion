use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{LamathStringedVst3Processor, SUBCATEGORY};

const CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    LamathStringedVst3Processor::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_component,
)];

fn factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, CLASSES)
}

fn create_component() -> ComPtr<FUnknown> {
    ComWrapper::new(LamathStringedVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("component must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(factory());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_registers_single_component() {
        assert_eq!(factory().class_count(), 1);
    }
}
