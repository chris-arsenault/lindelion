//! `EditorController` — instantiate a plugin's `IEditController`, connect it to the component, sync
//! its state, and create the editor `IPlugView`. Platform-neutral COM (the window/`HWND` attach is
//! Step 5).

use std::ffi::c_void;
use std::ptr;

use vst3::Steinberg::Vst::{
    IComponent, IComponentTrait, IConnectionPoint, IConnectionPointTrait, IEditController,
    IEditControllerTrait, IHostApplication, ViewType,
};
use vst3::Steinberg::{
    FUnknown, IPlugView, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, TUID, kResultOk,
};
use vst3::{ComPtr, ComWrapper, Interface};

use super::bstream::MemoryStream;
use super::instance::HostError;
use super::state::capture_state;

/// A plugin's edit controller, connected to its component and ready to vend an editor view.
pub struct EditorController {
    controller: ComPtr<IEditController>,
}

impl EditorController {
    /// Instantiate the controller for `component` from `factory`, initialize + connect + sync it.
    pub fn new(
        factory: &ComPtr<IPluginFactory>,
        component: &ComPtr<IComponent>,
        host: &ComPtr<IHostApplication>,
    ) -> Result<Self, HostError> {
        unsafe {
            let mut cid: TUID = [0; 16];
            if component.getControllerClassId(&mut cid) != kResultOk {
                return Err(HostError::NoController);
            }

            let mut obj: *mut c_void = ptr::null_mut();
            let result = factory.createInstance(
                cid.as_ptr(),
                IEditController::IID.as_ptr().cast(),
                &mut obj,
            );
            if result != kResultOk || obj.is_null() {
                return Err(HostError::NoController);
            }
            let controller =
                ComPtr::from_raw(obj.cast::<IEditController>()).ok_or(HostError::NoController)?;

            controller.initialize(host.as_ptr() as *mut FUnknown);

            // Best-effort component↔controller connection (so editor edits reach the processor).
            if let (Some(component_cp), Some(controller_cp)) = (
                component.cast::<IConnectionPoint>(),
                controller.cast::<IConnectionPoint>(),
            ) {
                component_cp.connect(controller_cp.as_ptr());
                controller_cp.connect(component_cp.as_ptr());
            }

            // Sync the controller to the component's current state.
            let state = capture_state(component);
            if !state.is_empty() {
                let stream = ComWrapper::new(MemoryStream::from_bytes(state));
                if let Some(iface) = stream.to_com_ptr::<vst3::Steinberg::IBStream>() {
                    controller.setComponentState(iface.as_ptr());
                }
            }

            Ok(EditorController { controller })
        }
    }

    /// Create the plugin's editor view, if it has one.
    pub fn create_view(&self) -> Option<ComPtr<IPlugView>> {
        unsafe {
            let view = self.controller.createView(ViewType::kEditor);
            ComPtr::from_raw(view)
        }
    }
}

impl Drop for EditorController {
    fn drop(&mut self) {
        unsafe {
            self.controller.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::PluginInstance;
    use crate::vst3_host::fixture::fixture_factory;

    #[test]
    fn editor_controller_creates_a_view() {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");

        let editor = EditorController::new(&factory, instance.component(), &host)
            .expect("editor controller");

        assert!(editor.create_view().is_some());
    }
}
