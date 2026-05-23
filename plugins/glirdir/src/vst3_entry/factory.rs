use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{GlirdirVst3Controller, GlirdirVst3Processor, SUBCATEGORY};

const CONTROLLER_NAME: &str = "Glirdir Controller";

const GLIRDIR_VST3_CLASSES: &[Vst3ClassRegistration] = &[
    Vst3ClassRegistration::audio_processor(
        GlirdirVst3Processor::CID,
        DESCRIPTOR.name,
        SUBCATEGORY,
        create_processor,
    ),
    Vst3ClassRegistration::edit_controller(
        GlirdirVst3Controller::CID,
        CONTROLLER_NAME,
        create_controller,
    ),
];

fn glirdir_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, GLIRDIR_VST3_CLASSES)
}

fn create_processor() -> ComPtr<FUnknown> {
    ComWrapper::new(GlirdirVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("processor must expose FUnknown")
}

fn create_controller() -> ComPtr<FUnknown> {
    ComWrapper::new(GlirdirVst3Controller::new())
        .to_com_ptr::<FUnknown>()
        .expect("controller must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(glirdir_vst3_factory());

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use vst3::Steinberg::*;

    use super::*;

    #[test]
    fn glirdir_registers_processor_and_controller_with_shared_factory() {
        let factory = glirdir_vst3_factory();

        assert_eq!(factory.class_count(), 2);
        assert_eq!(unsafe { factory.countClasses() }, 2);

        let mut processor = unsafe { std::mem::zeroed::<PClassInfo2>() };
        assert_eq!(
            unsafe { factory.getClassInfo2(0, &mut processor) },
            kResultOk
        );
        assert_eq!(processor.cid, GlirdirVst3Processor::CID);
        assert_eq!(c_string(&processor.name), DESCRIPTOR.name);
        assert_eq!(c_string(&processor.subCategories), SUBCATEGORY);

        let mut controller = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(1, &mut controller) },
            kResultOk
        );
        assert_eq!(controller.cid, GlirdirVst3Controller::CID);
        assert_eq!(c_string(&controller.name), CONTROLLER_NAME);
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}
