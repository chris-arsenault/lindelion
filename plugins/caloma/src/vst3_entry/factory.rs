use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{CalomaVst3Plugin, SUBCATEGORY};

/// Single-component: one registered class. The host loads this audio-processor class and
/// `queryInterface`s the controller on the same created object (ADR-0023, self-contained editor).
const CALOMA_VST3_CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    CalomaVst3Plugin::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_plugin,
)];

fn caloma_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, CALOMA_VST3_CLASSES)
}

fn create_plugin() -> ComPtr<FUnknown> {
    ComWrapper::new(CalomaVst3Plugin::new())
        .to_com_ptr::<FUnknown>()
        .expect("plugin object must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(caloma_vst3_factory());

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use vst3::Steinberg::*;

    use super::*;

    #[test]
    fn caloma_registers_exactly_one_single_component_class() {
        let factory = caloma_vst3_factory();

        assert_eq!(factory.class_count(), 1);
        assert_eq!(unsafe { factory.countClasses() }, 1);

        let mut info = unsafe { std::mem::zeroed::<PClassInfo2>() };
        assert_eq!(unsafe { factory.getClassInfo2(0, &mut info) }, kResultOk);
        assert_eq!(info.cid, CalomaVst3Plugin::CID);
        assert_eq!(c_string(&info.name), DESCRIPTOR.name);
        assert_eq!(c_string(&info.subCategories), SUBCATEGORY);
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}
