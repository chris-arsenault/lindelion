//! WASAPI device enumeration — list active capture/render endpoints as serializable [`DeviceRef`]s.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    STGM_READ,
};
use windows::core::PWSTR;

use crate::session::DeviceRef;

/// Capture (input) or render (output) endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection {
    Input,
    Output,
}

impl AudioDirection {
    fn data_flow(self) -> EDataFlow {
        match self {
            AudioDirection::Input => eCapture,
            AudioDirection::Output => eRender,
        }
    }
}

/// Errors from the WASAPI layer.
#[derive(Debug)]
pub enum AudioError {
    Com(windows::core::Error),
    /// The device's stream format is not one the engine handles (f32 / i16).
    UnsupportedFormat,
    /// The realtime audio thread failed to report its setup result.
    ThreadSetup,
}

impl From<windows::core::Error> for AudioError {
    fn from(error: windows::core::Error) -> Self {
        AudioError::Com(error)
    }
}

/// Ensure COM is initialized on this thread. Device enumeration also runs on Galad's UI thread
/// before winit creates the window, so use STA-compatible COM rather than poisoning the thread for
/// winit's OLE drag/drop initialization. The audio thread initializes MTA before it opens streams.
fn ensure_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

pub(super) fn device_enumerator() -> Result<IMMDeviceEnumerator, AudioError> {
    ensure_com();
    let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    Ok(enumerator)
}

/// Enumerate active endpoints in `direction` as serializable [`DeviceRef`]s.
pub fn enumerate(direction: AudioDirection) -> Result<Vec<DeviceRef>, AudioError> {
    let enumerator = device_enumerator()?;
    unsafe {
        let collection =
            enumerator.EnumAudioEndpoints(direction.data_flow(), DEVICE_STATE_ACTIVE)?;
        let count = collection.GetCount()?;
        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let device = collection.Item(index)?;
            devices.push(device_ref(&device)?);
        }
        Ok(devices)
    }
}

/// The default endpoint for `direction` (console role).
pub fn default_device(direction: AudioDirection) -> Result<DeviceRef, AudioError> {
    let enumerator = device_enumerator()?;
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(direction.data_flow(), eConsole)?;
        device_ref(&device)
    }
}

unsafe fn device_ref(device: &IMMDevice) -> Result<DeviceRef, AudioError> {
    let id = pwstr_into_string(device.GetId()?);
    let store = device.OpenPropertyStore(STGM_READ)?;
    let property = store.GetValue(&PKEY_Device_FriendlyName)?;
    let name = pwstr_into_string(PropVariantToStringAlloc(&property)?);
    Ok(DeviceRef { id, name })
}

/// Decode a COM-owned `PWSTR` (CoTaskMem-allocated) into a `String`, then free it.
unsafe fn pwstr_into_string(pwstr: PWSTR) -> String {
    if pwstr.is_null() {
        return String::new();
    }
    let string = pwstr.to_string().unwrap_or_default();
    CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
    string
}
