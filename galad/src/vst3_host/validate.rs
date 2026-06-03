//! Load-time plugin validation (M7). Probe a plugin end-to-end — instantiate the audio class,
//! prepare it, and process one silent stereo block — so an incompatible or mid-process-failing
//! plugin is rejected *before* it is added to a chain, instead of failing later on the audio thread.
//! Drives the same host path the live chain uses (M1 instantiate/setup, M3 `drive_process`).

use std::ffi::c_char;
use std::path::Path;

use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;
use vst3::Steinberg::{
    IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactory3, IPluginFactory3Trait,
    IPluginFactoryTrait, PClassInfo2, PClassInfoW, PFactoryInfo,
};
use vst3::Steinberg::{kResultOk, kResultTrue};

use super::editor_controller::EditorController;
use super::host_context::HostContext;
use super::instance::{HostError, PluginInstance};
use super::module::load_module;
use super::processing::{ProcessBusScratch, ProcessDriver};

/// Probe block geometry — one short silent block is enough to exercise instantiate→setup→process.
const VALIDATE_SAMPLE_RATE: f64 = 48_000.0;
const VALIDATE_BLOCK: usize = 512;
const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

/// A full scan probe result for one plugin path.
#[derive(Debug)]
pub struct PluginProbe {
    /// Vendor reported by the VST3 factory/class metadata, if present.
    pub vendor: Option<String>,
    /// Existing compatibility validation result.
    pub validation: Result<(), HostError>,
}

/// Probe the `.vst3` at `path`: load it, then validate its factory. Returns the first error and never
/// panics on a misbehaving plugin (the failure modes the host can contain — incompatible class or an
/// error returned from `process`; foreign-code aborts/hangs are out of in-process reach, see Step 5).
pub fn validate_plugin(path: &Path) -> Result<(), HostError> {
    probe_plugin(path).validation
}

/// Probe the `.vst3` at `path` for catalog metadata and compatibility. The module is loaded once:
/// class/factory metadata is read first, then the existing end-to-end validation runs over the same
/// factory.
pub fn probe_plugin(path: &Path) -> PluginProbe {
    let module = match load_module(path) {
        Ok(module) => module,
        Err(error) => {
            return PluginProbe {
                vendor: None,
                validation: Err(error),
            };
        }
    };
    let vendor = factory_vendor(module.factory());
    let host = HostContext::new()
        .to_com_ptr::<IHostApplication>()
        .expect("host exposes IHostApplication");
    let validation = validate_factory(module.factory(), &host);
    PluginProbe { vendor, validation }
}

/// Read the most specific VST3 vendor metadata available for the audio class. `PClassInfo2.vendor`
/// can override the factory vendor, so prefer it when the factory exposes `IPluginFactory2`; fall
/// back to `PFactoryInfo.vendor`.
fn factory_vendor(factory: &ComPtr<IPluginFactory>) -> Option<String> {
    unicode_class_vendor(factory)
        .or_else(|| class_vendor(factory))
        .or_else(|| factory_info_vendor(factory))
}

fn unicode_class_vendor(factory: &ComPtr<IPluginFactory>) -> Option<String> {
    let factory3 = factory.cast::<IPluginFactory3>()?;
    let count = unsafe { factory.countClasses() };
    for index in 0..count {
        let mut info = unsafe { std::mem::zeroed::<PClassInfoW>() };
        if unsafe { factory3.getClassInfoUnicode(index, &mut info) } != kResultOk {
            continue;
        }
        if c_array_eq(&info.category, AUDIO_MODULE_CLASS) {
            return wide_array_string(&info.vendor);
        }
    }
    None
}

fn class_vendor(factory: &ComPtr<IPluginFactory>) -> Option<String> {
    let factory2 = factory.cast::<IPluginFactory2>()?;
    let count = unsafe { factory.countClasses() };
    for index in 0..count {
        let mut info = unsafe { std::mem::zeroed::<PClassInfo2>() };
        if unsafe { factory2.getClassInfo2(index, &mut info) } != kResultOk {
            continue;
        }
        if c_array_eq(&info.category, AUDIO_MODULE_CLASS) {
            return c_array_string(&info.vendor);
        }
    }
    None
}

fn factory_info_vendor(factory: &ComPtr<IPluginFactory>) -> Option<String> {
    let mut info = unsafe { std::mem::zeroed::<PFactoryInfo>() };
    if unsafe { factory.getFactoryInfo(&mut info) } != kResultOk {
        return None;
    }
    c_array_string(&info.vendor)
}

/// Probe a factory: instantiate the audio class, prepare it, and process one silent stereo block,
/// surfacing the first failure as a `HostError`.
fn validate_factory(
    factory: &ComPtr<IPluginFactory>,
    host: &ComPtr<IHostApplication>,
) -> Result<(), HostError> {
    let instance = PluginInstance::from_factory(factory, host)?;
    let _controller = EditorController::new(factory, &instance, host).ok();
    let driver = ProcessDriver::new(VALIDATE_SAMPLE_RATE, VALIDATE_BLOCK);
    driver.prepare(&instance)?;

    let silent = vec![0.0f32; VALIDATE_BLOCK];
    let mut out_left = vec![0.0f32; VALIDATE_BLOCK];
    let mut out_right = vec![0.0f32; VALIDATE_BLOCK];
    let mut buses = ProcessBusScratch::from_component(
        instance.debug_name(),
        instance.component(),
        instance.processor(),
        VALIDATE_BLOCK,
        VALIDATE_SAMPLE_RATE,
    )?;
    let result = unsafe {
        buses.drive_stereo(
            instance.processor(),
            [&silent, &silent],
            [&mut out_left, &mut out_right],
        )
    };
    if result != kResultOk && result != kResultTrue {
        return Err(HostError::ProcessFailed(result));
    }
    Ok(())
}

/// Compare a NUL-terminated VST3 string field to an expected `&str`.
fn c_array_eq(buf: &[c_char], expected: &str) -> bool {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    let bytes = c_char_bytes(&buf[..len]);
    bytes == expected.as_bytes()
}

fn c_array_string(buf: &[c_char]) -> Option<String> {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    let text = String::from_utf8_lossy(&c_char_bytes(&buf[..len]))
        .trim()
        .to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn c_char_bytes(buf: &[c_char]) -> Vec<u8> {
    buf.iter().map(|&ch| ch as u8).collect()
}

fn wide_array_string(buf: &[u16]) -> Option<String> {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..len]).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::fixture::{
        context_fixture_factory, gain_fixture_factory, no_audio_class_factory,
        process_error_factory, single_component_fixture_factory, strict_sidechain_fixture_factory,
    };
    use std::ffi::c_void;
    use std::ptr;
    use vst3::Steinberg::*;
    use vst3::{Class, ComWrapper};

    fn host() -> ComPtr<IHostApplication> {
        HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
    }

    #[test]
    fn well_behaved_plugin_validates() {
        assert!(validate_factory(&gain_fixture_factory(1.0, 0), &host()).is_ok());
    }

    #[test]
    fn single_component_plugin_validates() {
        assert!(validate_factory(&single_component_fixture_factory(), &host()).is_ok());
    }

    #[test]
    fn plugin_with_declared_sidechain_validates() {
        assert!(validate_factory(&strict_sidechain_fixture_factory(), &host()).is_ok());
    }

    #[test]
    fn plugin_requiring_process_context_validates() {
        assert!(validate_factory(&context_fixture_factory(), &host()).is_ok());
    }

    #[test]
    fn incompatible_plugin_is_rejected() {
        assert!(matches!(
            validate_factory(&no_audio_class_factory(), &host()),
            Err(HostError::NoAudioClass)
        ));
    }

    #[test]
    fn process_error_plugin_is_rejected() {
        assert!(matches!(
            validate_factory(&process_error_factory(), &host()),
            Err(HostError::ProcessFailed(_))
        ));
    }

    #[test]
    fn vendor_probe_prefers_audio_class_vendor() {
        let factory = ComWrapper::new(VendorFixtureFactory)
            .to_com_ptr::<IPluginFactory>()
            .expect("IPluginFactory");

        assert_eq!(factory_vendor(&factory).as_deref(), Some("Class Vendor"));
    }

    #[test]
    fn vendor_probe_falls_back_to_factory_vendor() {
        let factory = single_component_fixture_factory();

        assert_eq!(factory_vendor(&factory).as_deref(), Some("Ahara"));
    }

    #[test]
    fn vendor_probe_prefers_unicode_audio_class_vendor() {
        let factory = ComWrapper::new(UnicodeVendorFixtureFactory)
            .to_com_ptr::<IPluginFactory>()
            .expect("IPluginFactory");

        assert_eq!(factory_vendor(&factory).as_deref(), Some("Unicode Vendor"));
    }

    struct VendorFixtureFactory;

    impl Class for VendorFixtureFactory {
        type Interfaces = (IPluginFactory, IPluginFactory2);
    }

    impl IPluginFactoryTrait for VendorFixtureFactory {
        unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
            if info.is_null() {
                return kInvalidArgument;
            }
            let info = &mut *info;
            fill_cstr(&mut info.vendor, "Factory Vendor");
            fill_cstr(&mut info.url, "");
            fill_cstr(&mut info.email, "");
            info.flags = 0;
            kResultOk
        }

        unsafe fn countClasses(&self) -> i32 {
            1
        }

        unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
            if info.is_null() || index != 0 {
                return kInvalidArgument;
            }
            let info = &mut *info;
            info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
            fill_cstr(&mut info.category, AUDIO_MODULE_CLASS);
            fill_cstr(&mut info.name, "Vendor Fixture");
            kResultOk
        }

        unsafe fn createInstance(
            &self,
            _cid: FIDString,
            _iid: FIDString,
            obj: *mut *mut c_void,
        ) -> tresult {
            if !obj.is_null() {
                *obj = ptr::null_mut();
            }
            kInvalidArgument
        }
    }

    impl IPluginFactory2Trait for VendorFixtureFactory {
        unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
            if info.is_null() || index != 0 {
                return kInvalidArgument;
            }
            let info = &mut *info;
            info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
            fill_cstr(&mut info.category, AUDIO_MODULE_CLASS);
            fill_cstr(&mut info.name, "Vendor Fixture");
            fill_cstr(&mut info.subCategories, "Fx");
            fill_cstr(&mut info.vendor, "Class Vendor");
            fill_cstr(&mut info.version, "1.0.0");
            fill_cstr(&mut info.sdkVersion, "VST 3.8.0");
            kResultOk
        }
    }

    struct UnicodeVendorFixtureFactory;

    impl Class for UnicodeVendorFixtureFactory {
        type Interfaces = (IPluginFactory, IPluginFactory2, IPluginFactory3);
    }

    impl IPluginFactoryTrait for UnicodeVendorFixtureFactory {
        unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
            if info.is_null() {
                return kInvalidArgument;
            }
            let info = &mut *info;
            fill_cstr(&mut info.vendor, "Factory Vendor");
            fill_cstr(&mut info.url, "");
            fill_cstr(&mut info.email, "");
            info.flags = 0;
            kResultOk
        }

        unsafe fn countClasses(&self) -> i32 {
            1
        }

        unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
            if info.is_null() || index != 0 {
                return kInvalidArgument;
            }
            let info = &mut *info;
            info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
            fill_cstr(&mut info.category, AUDIO_MODULE_CLASS);
            fill_cstr(&mut info.name, "Unicode Vendor Fixture");
            kResultOk
        }

        unsafe fn createInstance(
            &self,
            _cid: FIDString,
            _iid: FIDString,
            obj: *mut *mut c_void,
        ) -> tresult {
            if !obj.is_null() {
                *obj = ptr::null_mut();
            }
            kInvalidArgument
        }
    }

    impl IPluginFactory2Trait for UnicodeVendorFixtureFactory {
        unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
            if info.is_null() || index != 0 {
                return kInvalidArgument;
            }
            let info = &mut *info;
            info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
            fill_cstr(&mut info.category, AUDIO_MODULE_CLASS);
            fill_cstr(&mut info.name, "Unicode Vendor Fixture");
            fill_cstr(&mut info.subCategories, "Fx");
            fill_cstr(&mut info.vendor, "");
            fill_cstr(&mut info.version, "1.0.0");
            fill_cstr(&mut info.sdkVersion, "VST 3.8.0");
            kResultOk
        }
    }

    impl IPluginFactory3Trait for UnicodeVendorFixtureFactory {
        unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
            if info.is_null() || index != 0 {
                return kInvalidArgument;
            }
            let info = &mut *info;
            info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
            fill_cstr(&mut info.category, AUDIO_MODULE_CLASS);
            fill_utf16(&mut info.name, "Unicode Vendor Fixture");
            fill_cstr(&mut info.subCategories, "Fx");
            fill_utf16(&mut info.vendor, "Unicode Vendor");
            fill_utf16(&mut info.version, "1.0.0");
            fill_utf16(&mut info.sdkVersion, "VST 3.8.0");
            kResultOk
        }

        unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
            kResultOk
        }
    }

    fn fill_cstr(dst: &mut [c_char], s: &str) {
        dst.iter_mut().for_each(|b| *b = 0);
        let n = dst.len().saturating_sub(1);
        for (slot, byte) in dst.iter_mut().zip(s.bytes()).take(n) {
            *slot = byte as c_char;
        }
    }

    fn fill_utf16(dst: &mut [u16], s: &str) {
        dst.iter_mut().for_each(|b| *b = 0);
        let n = dst.len().saturating_sub(1);
        for (slot, unit) in dst.iter_mut().zip(s.encode_utf16()).take(n) {
            *slot = unit;
        }
    }
}
