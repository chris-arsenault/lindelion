//! `EditorController` — resolve a plugin's `IEditController`, connect it to the component, sync its
//! state, and create the editor `IPlugView`. Platform-neutral COM (the window/`HWND` attach is Step
//! 5).

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
use super::instance::{HostError, PluginInstance};
use super::state::capture_state;

/// A plugin's edit controller, connected to its component and ready to vend an editor view.
pub struct EditorController {
    controller: ComPtr<IEditController>,
    terminate_on_drop: bool,
}

impl EditorController {
    /// Resolve the controller for `instance` from `factory`, initialize + connect + sync it when it
    /// is a separate controller object.
    pub fn new(
        factory: &ComPtr<IPluginFactory>,
        instance: &PluginInstance,
        host: &ComPtr<IHostApplication>,
    ) -> Result<Self, HostError> {
        let component = instance.component();
        unsafe {
            let mut cid: TUID = [0; 16];
            let controller_class_result = component.getControllerClassId(&mut cid);
            crate::diagnostics::log(format!(
                "editor-controller: getControllerClassId result={controller_class_result} single_component={}",
                controller_class_result != kResultOk || cid == instance.class_id()
            ));
            if controller_class_result != kResultOk || cid == instance.class_id() {
                crate::diagnostics::log("editor-controller: direct IEditController cast begin");
                let Some(controller) = component.cast::<IEditController>() else {
                    crate::diagnostics::log(
                        "editor-controller: direct IEditController cast failed",
                    );
                    return Err(HostError::NoController);
                };
                crate::diagnostics::log("editor-controller: direct IEditController cast done");
                return Ok(EditorController {
                    controller,
                    terminate_on_drop: false,
                });
            }

            crate::diagnostics::log("editor-controller: create separate controller begin");
            let mut obj: *mut c_void = ptr::null_mut();
            let result = factory.createInstance(
                cid.as_ptr(),
                IEditController::IID.as_ptr().cast(),
                &mut obj,
            );
            if result != kResultOk || obj.is_null() {
                crate::diagnostics::log(format!(
                    "editor-controller: create separate controller failed result={result} null={}",
                    obj.is_null()
                ));
                return Err(HostError::NoController);
            }
            let controller =
                ComPtr::from_raw(obj.cast::<IEditController>()).ok_or(HostError::NoController)?;
            crate::diagnostics::log("editor-controller: create separate controller done");

            crate::diagnostics::log("editor-controller: initialize begin");
            controller.initialize(host.as_ptr() as *mut FUnknown);
            crate::diagnostics::log("editor-controller: initialize done");

            // Best-effort component↔controller connection (so editor edits reach the processor).
            if let (Some(component_cp), Some(controller_cp)) = (
                component.cast::<IConnectionPoint>(),
                controller.cast::<IConnectionPoint>(),
            ) {
                crate::diagnostics::log("editor-controller: connect begin");
                component_cp.connect(controller_cp.as_ptr());
                controller_cp.connect(component_cp.as_ptr());
                crate::diagnostics::log("editor-controller: connect done");
            }

            // Sync the controller to the component's current state.
            crate::diagnostics::log("editor-controller: capture component state begin");
            let state = capture_state(component);
            crate::diagnostics::log(format!(
                "editor-controller: capture component state done bytes={}",
                state.len()
            ));
            if !state.is_empty() {
                let stream = ComWrapper::new(MemoryStream::from_bytes(state));
                if let Some(iface) = stream.to_com_ptr::<vst3::Steinberg::IBStream>() {
                    crate::diagnostics::log("editor-controller: setComponentState begin");
                    controller.setComponentState(iface.as_ptr());
                    crate::diagnostics::log("editor-controller: setComponentState done");
                }
            }

            Ok(EditorController {
                controller,
                terminate_on_drop: true,
            })
        }
    }

    /// Create the plugin's editor view, if it has one.
    pub fn create_view(&self) -> Option<ComPtr<IPlugView>> {
        unsafe {
            crate::diagnostics::log("editor-controller: createView(kEditor) call begin");
            let view = self.controller.createView(ViewType::kEditor);
            let view = ComPtr::from_raw(view);
            crate::diagnostics::log(format!(
                "editor-controller: createView(kEditor) call done has_view={}",
                view.is_some()
            ));
            view
        }
    }
}

impl Drop for EditorController {
    fn drop(&mut self) {
        if !self.terminate_on_drop {
            return;
        }
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
    use crate::vst3_host::fixture::{
        fixture_factory, own_cid_single_component_fixture_factory, single_component_fixture_factory,
    };

    #[test]
    fn editor_controller_creates_a_view() {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");

        let editor = EditorController::new(&factory, &instance, &host).expect("editor controller");

        assert!(editor.create_view().is_some());
    }

    #[test]
    fn editor_controller_uses_single_component_controller_when_no_controller_class_is_reported() {
        let factory = single_component_fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");

        let editor = EditorController::new(&factory, &instance, &host).expect("editor controller");

        assert!(editor.create_view().is_some());
    }

    #[test]
    fn editor_controller_uses_single_component_controller_when_controller_cid_is_audio_cid() {
        let factory = own_cid_single_component_fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");

        let editor = EditorController::new(&factory, &instance, &host).expect("editor controller");

        assert!(editor.create_view().is_some());
    }
}
