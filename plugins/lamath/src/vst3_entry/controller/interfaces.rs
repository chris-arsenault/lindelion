impl IPluginBaseTrait for ResonatorVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for ResonatorVst3Controller {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        let Ok(payload) = std::str::from_utf8(&plugin_state.payload) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_toml_str(payload) else {
            return kResultFalse;
        };

        self.replace_patch_mirror(patch);
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        VST3_PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        if param_index as usize == PITCH_BEND_PARAMETER_INDEX {
            return fill_vst3_parameter_info(
                Vst3ParameterInfo {
                    id: PITCH_BEND_PARAMETER_ID,
                    title: "Pitch Bend",
                    short_title: "Pitch",
                    units: "st",
                    step_count: 0,
                    default_normalized_value: 0.5,
                    flags: ParameterInfo_::ParameterFlags_::kCanAutomate,
                }
                .hidden(),
                info,
            );
        }

        let Some(binding) = parameter_binding_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        fill_vst3_parameter_info(Vst3ParameterInfo::from_parameter(binding.info()), info)
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
        if id == PITCH_BEND_PARAMETER_ID {
            return write_vst3_parameter_string(
                &lindelion_plugin_shell::format_plain_value(
                    pitch_bend_plain_from_normalized(value_normalized) as f32,
                ),
                string,
            );
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        write_vst3_parameter_string(&format_parameter_plain_value(id, plain), string)
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
        let Some(value) = parse_vst3_plain_value_string(string) else {
            return kInvalidArgument;
        };
        if id == PITCH_BEND_PARAMETER_ID {
            *value_normalized = pitch_bend_normalized_from_plain(value);
            return kResultOk;
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        *value_normalized = parameter.range.normalize(value) as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_plain_from_normalized(value_normalized);
        }

        parameter_by_id(id)
            .map(|parameter| parameter.range.denormalize(value_normalized as f32) as f64)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_normalized_from_plain(plain_value as f32);
        }

        normalized_parameter_value(id, plain_value as f32)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        let Some(index) = parameter_index(id) else {
            return 0.0;
        };
        self.values.value(index).unwrap_or_default()
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

impl IMidiMappingTrait for ResonatorVst3Controller {
    unsafe fn getMidiControllerAssignment(
        &self,
        busIndex: i32,
        channel: i16,
        midiControllerNumber: CtrlNumber,
        id: *mut u32,
    ) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }

        if busIndex == 0
            && (0..=15).contains(&channel)
            && midiControllerNumber == ControllerNumbers_::kPitchBend as CtrlNumber
        {
            *id = PITCH_BEND_PARAMETER_ID;
            return kResultTrue;
        }

        kResultFalse
    }
}
