use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{CenedrilVst3Processor, SUBCATEGORY};

// Single-component: one Audio Module Class that also implements `IEditController`. The host queries
// the controller interface on this object, so there is no separate controller class registration.
const CENEDRIL_VST3_CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    CenedrilVst3Processor::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_component,
)];

fn cenedril_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, CENEDRIL_VST3_CLASSES)
}

fn create_component() -> ComPtr<FUnknown> {
    ComWrapper::new(CenedrilVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("component must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(cenedril_vst3_factory());

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use vst3::Steinberg::*;

    use super::*;

    #[test]
    fn cenedril_registers_a_single_component_class() {
        let factory = cenedril_vst3_factory();

        assert_eq!(factory.class_count(), 1);
        assert_eq!(unsafe { factory.countClasses() }, 1);

        let mut component = unsafe { std::mem::zeroed::<PClassInfo2>() };
        assert_eq!(
            unsafe { factory.getClassInfo2(0, &mut component) },
            kResultOk
        );
        assert_eq!(component.cid, CenedrilVst3Processor::CID);
        assert_eq!(c_string(&component.name), DESCRIPTOR.name);
        assert_eq!(c_string(&component.subCategories), SUBCATEGORY);
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}
