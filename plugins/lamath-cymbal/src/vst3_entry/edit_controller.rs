//! The single-component object's `IEditController` face (one COM object is processor and
//! controller; the editor reads the processor directly).

#![allow(clippy::wildcard_imports)]

use std::ffi::c_char;

use lindelion_plugin_shell::vst3::{
    Vst3ParameterInfo, fill_vst3_parameter_info, parse_vst3_plain_value_string,
    write_vst3_parameter_string,
};
use vst3::{Steinberg::Vst::*, Steinberg::*};

use super::{editor, processor::LamathCymbalVst3Processor};
use crate::parameters;

impl IEditControllerTrait for LamathCymbalVst3Processor {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::getState(self, state)
    }

    unsafe fn getParameterCount(&self) -> i32 {
        parameters::PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() || param_index < 0 {
            return kInvalidArgument;
        }
        let Some(parameter) = parameters::parameter_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        fill_vst3_parameter_info(Vst3ParameterInfo::from_parameter(parameter), info)
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let Some(parameter) = parameters::parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        write_vst3_parameter_string(&parameters::format_plain_value(id, plain), string)
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        if string.is_null() || value_normalized.is_null() {
            return kInvalidArgument;
        }
        let Some(plain) = parse_vst3_plain_value_string(string) else {
            return kInvalidArgument;
        };
        let Some(parameter) = parameters::parameter_by_id(id) else {
            return kInvalidArgument;
        };
        *value_normalized = f64::from(parameter.range.normalize(plain));
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        parameters::parameter_by_id(id)
            .map(|parameter| f64::from(parameter.range.denormalize(value_normalized as f32)))
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        parameters::parameter_by_id(id)
            .map(|parameter| f64::from(parameter.range.normalize(plain_value as f32)))
            .unwrap_or(0.0)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        parameters::parameter_index(id)
            .and_then(|index| self.values.value(index))
            .unwrap_or(0.0)
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        self.set_value(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.handler.set(handler);
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        editor::create_editor_view(self)
    }
}
