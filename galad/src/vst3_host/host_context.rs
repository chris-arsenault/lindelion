//! Host context — the host-side `IHostApplication` + `IComponentHandler` a plugin receives in its
//! `initialize` call. Minimal for the M1 spike: it names the host and stubs the component handler.

use std::ffi::c_void;

use vst3::{Class, ComWrapper, Steinberg::Vst::*, Steinberg::*};

/// The host application context handed to a plugin (as `*mut FUnknown`) at `initialize`.
pub struct HostContext;

impl HostContext {
    /// Host name advertised to plugins via `IHostApplication::getName`.
    const NAME: &'static str = "Galad";

    /// Construct a reference-counted host context.
    pub fn new() -> ComWrapper<HostContext> {
        ComWrapper::new(HostContext)
    }
}

impl Class for HostContext {
    type Interfaces = (IHostApplication, IComponentHandler);
}

impl IHostApplicationTrait for HostContext {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        fill_string128(&mut *name, Self::NAME);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        _obj: *mut *mut c_void,
    ) -> tresult {
        // The host does not vend objects to plugins in the M1 spike (plugins needing host-created
        // IMessage/IAttributeList are out of scope here).
        kNotImplemented
    }
}

impl IComponentHandlerTrait for HostContext {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, _id: ParamID, _value_normalized: ParamValue) -> tresult {
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        kResultOk
    }
}

/// Write `text` as NUL-terminated UTF-16 into a `String128` (`[TChar; 128]`), truncating to fit.
fn fill_string128(dst: &mut String128, text: &str) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    for (slot, unit) in dst.iter_mut().zip(text.encode_utf16().take(max)) {
        *slot = unit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_application_reports_galad_name() {
        let ctx = HostContext::new();
        let app = ctx
            .to_com_ptr::<IHostApplication>()
            .expect("host context exposes IHostApplication");

        let mut name: String128 = [0; 128];
        let result = unsafe { app.getName(&mut name) };

        assert_eq!(result, kResultOk);
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        let decoded = String::from_utf16(&name[..end]).expect("valid UTF-16");
        assert_eq!(decoded, "Galad");
    }

    #[test]
    fn host_create_instance_is_not_implemented() {
        let ctx = HostContext::new();
        let app = ctx
            .to_com_ptr::<IHostApplication>()
            .expect("host context exposes IHostApplication");

        let mut cid: TUID = [0; 16];
        let mut iid: TUID = [0; 16];
        let mut obj: *mut c_void = std::ptr::null_mut();
        let result = unsafe { app.createInstance(&mut cid, &mut iid, &mut obj) };

        assert_eq!(result, kNotImplemented);
    }
}
