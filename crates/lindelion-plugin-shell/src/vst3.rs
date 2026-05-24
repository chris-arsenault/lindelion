#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::{CString as StdCString, c_char};

use vst3::Steinberg::Vst::TChar;

#[path = "vst3_component.rs"]
mod component;
#[path = "vst3_factory.rs"]
mod factory;
#[path = "vst3_message.rs"]
mod message;
#[path = "vst3_process.rs"]
mod process;
#[path = "vst3_view.rs"]
mod view;

pub use component::{
    Vst3BusInfo, Vst3ParameterChange, Vst3PeerConnection, can_process_32_bit_sample_size,
    fill_vst3_bus_info, for_each_vst3_parameter_change,
    mono_or_stereo_speaker_arrangement_supported, process_setup_from_vst, vst3_bus_count,
};
pub use factory::{
    Vst3ClassRegistration, Vst3CreateInstance, Vst3PluginFactory, plugin_factory_ptr,
};
pub use message::{
    PluginAttributes, PluginMessage, PluginMessageDecodeError, PluginMessagePayload,
    PluginMessageType, TypedPluginMessage, decode_typed_message, message_id, message_payload,
};
pub use process::{
    audio_input_buffer_from_vst_process_data, clear_vst_outputs, read_plugin_state_from_stream,
    stereo_output_buffers_from_vst_process_data, transport_context_from_vst_process_context,
    vst_event_to_host_midi, vst_event_to_midi, write_plugin_state_to_stream,
};
pub use view::{FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize};

pub fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let c_string = StdCString::new(src).unwrap_or_default();
    let bytes = c_string.as_bytes_with_nul();

    for (src, dst) in bytes.iter().zip(dst.iter_mut()) {
        *dst = *src as c_char;
    }

    if bytes.len() > dst.len()
        && let Some(last) = dst.last_mut()
    {
        *last = 0;
    }
}

pub fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let mut len = 0;
    for (src, dst) in src.encode_utf16().zip(dst.iter_mut()) {
        *dst = src as TChar;
        len += 1;
    }

    if len < dst.len() {
        dst[len] = 0;
    } else if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

/// Return the length of a null-terminated VST3 UTF-16 string.
///
/// # Safety
/// `string` must be either null or point to readable memory containing a null terminator.
pub unsafe fn len_wstring(string: *const TChar) -> usize {
    if string.is_null() {
        return 0;
    }

    let mut len = 0;
    while *string.add(len) != 0 {
        len += 1;
    }
    len
}

#[macro_export]
macro_rules! export_vst3_entrypoints {
    ($factory:expr) => {
        #[cfg(target_os = "windows")]
        #[unsafe(no_mangle)]
        pub extern "system" fn InitDll() -> bool {
            true
        }

        #[cfg(target_os = "windows")]
        #[unsafe(no_mangle)]
        pub extern "system" fn ExitDll() -> bool {
            true
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn bundleEntry(_bundle_ref: *mut ::std::ffi::c_void) -> bool {
            true
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn bundleExit() -> bool {
            true
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn BundleEntry(bundle_ref: *mut ::std::ffi::c_void) -> bool {
            bundleEntry(bundle_ref)
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn BundleExit() -> bool {
            bundleExit()
        }

        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        pub extern "system" fn ModuleEntry(_library_handle: *mut ::std::ffi::c_void) -> bool {
            true
        }

        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        pub extern "system" fn ModuleExit() -> bool {
            true
        }

        #[unsafe(no_mangle)]
        pub extern "system" fn GetPluginFactory() -> *mut ::vst3::Steinberg::IPluginFactory {
            $crate::vst3::plugin_factory_ptr($factory)
        }
    };
}

#[cfg(test)]
#[path = "vst3_tests.rs"]
mod tests;
