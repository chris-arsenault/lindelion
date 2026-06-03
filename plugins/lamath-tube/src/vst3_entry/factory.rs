use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{LamathTubeVst3Processor, SUBCATEGORY};

const LAMATH_TUBE_CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    LamathTubeVst3Processor::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_component,
)];

fn lamath_tube_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, LAMATH_TUBE_CLASSES)
}

fn create_component() -> ComPtr<FUnknown> {
    ComWrapper::new(LamathTubeVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("component must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(lamath_tube_vst3_factory());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_registers_single_component() {
        let factory = lamath_tube_vst3_factory();
        assert_eq!(factory.class_count(), 1);
    }
}
