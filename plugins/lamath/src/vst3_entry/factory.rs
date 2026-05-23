use lindelion_plugin_shell::vst3::{Vst3ClassRegistration, Vst3PluginFactory};
use vst3::{ComPtr, ComWrapper, Steinberg::*};

use crate::DESCRIPTOR;

use super::{ResonatorVst3Controller, ResonatorVst3Processor, SUBCATEGORY};

const CONTROLLER_NAME: &str = "Lamath Controller";

const RESONATOR_VST3_CLASSES: &[Vst3ClassRegistration] = &[
    Vst3ClassRegistration::audio_processor(
        ResonatorVst3Processor::CID,
        DESCRIPTOR.name,
        SUBCATEGORY,
        create_processor,
    ),
    Vst3ClassRegistration::edit_controller(
        ResonatorVst3Controller::CID,
        CONTROLLER_NAME,
        create_controller,
    ),
];

fn resonator_vst3_factory() -> Vst3PluginFactory {
    Vst3PluginFactory::new(&DESCRIPTOR, RESONATOR_VST3_CLASSES)
}

fn create_processor() -> ComPtr<FUnknown> {
    ComWrapper::new(ResonatorVst3Processor::new())
        .to_com_ptr::<FUnknown>()
        .expect("processor must expose FUnknown")
}

fn create_controller() -> ComPtr<FUnknown> {
    ComWrapper::new(ResonatorVst3Controller::new())
        .to_com_ptr::<FUnknown>()
        .expect("controller must expose FUnknown")
}

lindelion_plugin_shell::export_vst3_entrypoints!(resonator_vst3_factory());

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use vst3::Steinberg::*;

    use super::*;

    #[test]
    fn resonator_registers_processor_and_controller_with_shared_factory() {
        let factory = resonator_vst3_factory();

        assert_eq!(factory.class_count(), 2);
        assert_eq!(unsafe { factory.countClasses() }, 2);

        let mut processor = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(0, &mut processor) },
            kResultOk
        );
        assert_eq!(processor.cid, ResonatorVst3Processor::CID);
        assert_eq!(c_string(&processor.name), DESCRIPTOR.name);

        let mut controller = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(1, &mut controller) },
            kResultOk
        );
        assert_eq!(controller.cid, ResonatorVst3Controller::CID);
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
