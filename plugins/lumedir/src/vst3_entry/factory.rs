use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{LumedirVst3Processor, SUBCATEGORY};

// Single-component: one Audio Module Class that also implements `IEditController`. The host queries
// the controller interface on this object, so there is no separate controller class registration.
const LUMEDIR_VST3_CLASSES: &[Vst3ClassRegistration] = &[Vst3ClassRegistration::audio_processor(
    LumedirVst3Processor::CID,
    DESCRIPTOR.name,
    SUBCATEGORY,
    create_component,
)];

fn lumedir_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, LUMEDIR_VST3_CLASSES)
}

fn create_component() -> ComPtr<FUnknown> {
    ComWrapper::new(LumedirVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("component must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(lumedir_vst3_factory());

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use vst3::Steinberg::*;

    use super::*;

    #[test]
    fn lumedir_registers_a_single_component_class() {
        let factory = lumedir_vst3_factory();

        assert_eq!(factory.class_count(), 1);
        assert_eq!(unsafe { factory.countClasses() }, 1);

        let mut component = unsafe { std::mem::zeroed::<PClassInfo2>() };
        assert_eq!(
            unsafe { factory.getClassInfo2(0, &mut component) },
            kResultOk
        );
        assert_eq!(component.cid, LumedirVst3Processor::CID);
        assert_eq!(c_string(&component.name), DESCRIPTOR.name);
        assert_eq!(c_string(&component.subCategories), SUBCATEGORY);
        assert_eq!(c_string(&component.vendor), DESCRIPTOR.vendor);

        let mut factory_info = unsafe { std::mem::zeroed::<PFactoryInfo>() };
        assert_eq!(
            unsafe { factory.getFactoryInfo(&mut factory_info) },
            kResultOk
        );
        assert_eq!(c_string(&factory_info.vendor), DESCRIPTOR.vendor);
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}
