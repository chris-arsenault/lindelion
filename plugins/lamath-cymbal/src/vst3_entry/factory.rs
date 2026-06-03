use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{LamathCymbalVst3Processor, SUBCATEGORY};

const LAMATH_CYMBAL_CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    LamathCymbalVst3Processor::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_component,
)];

fn lamath_cymbal_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, LAMATH_CYMBAL_CLASSES)
}

fn create_component() -> ComPtr<FUnknown> {
    ComWrapper::new(LamathCymbalVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("component must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(lamath_cymbal_vst3_factory());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_registers_single_component() {
        let factory = lamath_cymbal_vst3_factory();
        assert_eq!(factory.class_count(), 1);
    }
}
