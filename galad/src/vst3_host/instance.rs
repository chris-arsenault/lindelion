//! Factory driver — enumerate a plugin factory's classes, instantiate the audio-processor class as
//! `IComponent`, initialize it with the host context, and query `IAudioProcessor`. Mirrors the
//! host-style `createInstance` call in `crates/lindelion-plugin-shell/src/vst3_tests.rs`.

use std::ffi::{CStr, c_char, c_void};
use std::ptr;

use vst3::{ComPtr, Interface, Steinberg::Vst::*, Steinberg::*};

/// Errors raised while loading/driving a plugin host-side.
#[derive(Debug)]
pub enum HostError {
    /// The factory exposes no class with category `"Audio Module Class"`.
    NoAudioClass,
    /// `IPluginFactory::createInstance` failed (carries the returned `tresult`).
    CreateInstanceFailed(tresult),
    /// The instantiated component does not expose `IAudioProcessor`.
    MissingAudioProcessor,
    /// A step of the processing-setup sequence was rejected by the plugin (names the step).
    SetupFailed(&'static str),
    /// The plugin returned an error from `process` (carries the returned `tresult`).
    ProcessFailed(tresult),
    /// Failed to load a `.vst3` module or resolve its entry point.
    ModuleLoad(String),
    /// The plugin has no edit controller (or it failed to instantiate).
    NoController,
    /// The plugin's editor view does not support the host's window platform (`HWND`).
    EditorUnsupported,
    /// Failed to create or attach the editor window.
    EditorWindow(String),
}

/// A live plugin: its `IComponent` and `IAudioProcessor`, plus the host context kept alive for the
/// plugin's lifetime. Torn down (deactivate + terminate) on drop.
pub struct PluginInstance {
    class_id: TUID,
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    _host: ComPtr<IHostApplication>,
}

impl PluginInstance {
    /// Instantiate the factory's audio-processor class and initialize it with `host`.
    pub fn from_factory(
        factory: &ComPtr<IPluginFactory>,
        host: &ComPtr<IHostApplication>,
    ) -> Result<Self, HostError> {
        unsafe {
            let cid = find_audio_class_cid(factory).ok_or(HostError::NoAudioClass)?;

            let mut obj: *mut c_void = ptr::null_mut();
            let result =
                factory.createInstance(cid.as_ptr(), IComponent::IID.as_ptr().cast(), &mut obj);
            if result != kResultOk || obj.is_null() {
                return Err(HostError::CreateInstanceFailed(result));
            }
            let component = ComPtr::from_raw(obj.cast::<IComponent>())
                .ok_or(HostError::CreateInstanceFailed(result))?;

            component.initialize(host.as_ptr() as *mut FUnknown);

            let processor = component
                .cast::<IAudioProcessor>()
                .ok_or(HostError::MissingAudioProcessor)?;

            Ok(PluginInstance {
                class_id: cid,
                component,
                processor,
                _host: host.clone(),
            })
        }
    }

    /// The factory class id used to instantiate this plugin's audio component.
    pub fn class_id(&self) -> TUID {
        self.class_id
    }

    /// The plugin's audio processor.
    pub fn processor(&self) -> &ComPtr<IAudioProcessor> {
        &self.processor
    }

    /// The plugin's component.
    pub fn component(&self) -> &ComPtr<IComponent> {
        &self.component
    }
}

impl Drop for PluginInstance {
    fn drop(&mut self) {
        crate::diagnostics::log("vst3: PluginInstance drop begin");
        unsafe {
            self.processor.setProcessing(0);
            crate::diagnostics::log("vst3: PluginInstance setProcessing(0) done");
            self.component.setActive(0);
            crate::diagnostics::log("vst3: PluginInstance setActive(0) done");
            self.component.terminate();
            crate::diagnostics::log("vst3: PluginInstance terminate done");
        }
        crate::diagnostics::log("vst3: PluginInstance drop end");
    }
}

/// Find the cid of the first class whose category is `"Audio Module Class"`.
unsafe fn find_audio_class_cid(factory: &ComPtr<IPluginFactory>) -> Option<TUID> {
    let count = factory.countClasses();
    for index in 0..count {
        let mut info = std::mem::zeroed::<PClassInfo>();
        if factory.getClassInfo(index, &mut info) != kResultOk {
            continue;
        }
        if c_array_eq(&info.category, "Audio Module Class") {
            return Some(info.cid);
        }
    }
    None
}

/// Compare a NUL-terminated C-string field to an expected `&str`.
fn c_array_eq(buf: &[c_char], expected: &str) -> bool {
    let bytes = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_bytes();
    bytes == expected.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::fixture_factory;
    use vst3::{Class, ComWrapper};

    #[test]
    fn from_factory_instantiates_fixture_audio_processor() {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");

        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");

        assert_eq!(unsafe { instance.processor().getLatencySamples() }, 0);
    }

    struct EmptyFactory;

    impl Class for EmptyFactory {
        type Interfaces = (IPluginFactory,);
    }

    impl IPluginFactoryTrait for EmptyFactory {
        unsafe fn getFactoryInfo(&self, _info: *mut PFactoryInfo) -> tresult {
            kResultOk
        }
        unsafe fn countClasses(&self) -> i32 {
            0
        }
        unsafe fn getClassInfo(&self, _index: i32, _info: *mut PClassInfo) -> tresult {
            kInvalidArgument
        }
        unsafe fn createInstance(
            &self,
            _cid: FIDString,
            _iid: FIDString,
            _obj: *mut *mut c_void,
        ) -> tresult {
            kInvalidArgument
        }
    }

    #[test]
    fn from_factory_without_audio_class_errors() {
        let factory = ComWrapper::new(EmptyFactory)
            .to_com_ptr::<IPluginFactory>()
            .expect("IPluginFactory");
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");

        let result = PluginInstance::from_factory(&factory, &host);

        assert!(matches!(result, Err(HostError::NoAudioClass)));
    }
}
