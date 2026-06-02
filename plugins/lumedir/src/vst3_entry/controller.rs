use std::ffi::c_char;

use vst3::{Class, Steinberg::Vst::*, Steinberg::*, uid};

/// Minimal edit controller: Lúmedir exposes no parameters in M0. `createView` returns the
/// `FixedSizePlugView`; the actual Vizia editor attaches to the host child window on Windows
/// (ADR-0023) and is a no-op on other platforms.
pub(super) struct LumedirVst3Controller;

impl Class for LumedirVst3Controller {
    type Interfaces = (IEditController,);
}

impl LumedirVst3Controller {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.controller_cid[0],
        crate::VST3_BUNDLE_METADATA.controller_cid[1],
        crate::VST3_BUNDLE_METADATA.controller_cid[2],
        crate::VST3_BUNDLE_METADATA.controller_cid[3],
    );

    pub(super) fn new() -> Self {
        Self
    }
}

impl IPluginBaseTrait for LumedirVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for LumedirVst3Controller {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        0
    }

    unsafe fn getParameterInfo(&self, _param_index: i32, _info: *mut ParameterInfo) -> tresult {
        kInvalidArgument
    }

    unsafe fn getParamStringByValue(
        &self,
        _id: u32,
        _value_normalized: f64,
        _string: *mut String128,
    ) -> tresult {
        kInvalidArgument
    }

    unsafe fn getParamValueByString(
        &self,
        _id: u32,
        _string: *mut TChar,
        _value_normalized: *mut f64,
    ) -> tresult {
        kInvalidArgument
    }

    unsafe fn normalizedParamToPlain(&self, _id: u32, value_normalized: f64) -> f64 {
        value_normalized
    }

    unsafe fn plainParamToNormalized(&self, _id: u32, plain_value: f64) -> f64 {
        plain_value
    }

    unsafe fn getParamNormalized(&self, _id: u32) -> f64 {
        0.0
    }

    unsafe fn setParamNormalized(&self, _id: u32, _value: f64) -> tresult {
        kResultOk
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        super::editor::create_editor_view(self)
    }
}
