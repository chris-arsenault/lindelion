//! Host context — the host-side `IHostApplication` + `IComponentHandler` a plugin receives in its
//! `initialize` call. Minimal for the M1 spike: it names the host and stubs the component handler.

use std::{ffi::c_void, ptr};

use vst3::{Class, ComPtr, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::host_objects::{HostAttributeList, HostMessage};

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
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();
        crate::diagnostics::log(format!(
            "host-context: createInstance cid={} iid={}",
            tuid_ptr_hex(cid),
            tuid_ptr_hex(iid)
        ));
        let requested = *cid;
        let object = if requested == IMessage_iid {
            crate::diagnostics::log("host-context: createInstance class=IMessage");
            ComWrapper::new(HostMessage::new())
                .to_com_ptr::<FUnknown>()
                .expect("HostMessage exposes FUnknown")
        } else if requested == IAttributeList_iid {
            crate::diagnostics::log("host-context: createInstance class=IAttributeList");
            ComWrapper::new(HostAttributeList::new())
                .to_com_ptr::<FUnknown>()
                .expect("HostAttributeList exposes FUnknown")
        } else {
            crate::diagnostics::log("host-context: createInstance unsupported");
            return kNotImplemented;
        };

        let ptr = object.as_ptr();
        let result = ((*(*ptr).vtbl).queryInterface)(ptr, iid, obj);
        crate::diagnostics::log(format!(
            "host-context: createInstance result={result} null={}",
            (*obj).is_null()
        ));
        result
    }
}

impl IComponentHandlerTrait for HostContext {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        crate::diagnostics::log(format!("host-context: beginEdit id={id}"));
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
        crate::diagnostics::log(format!(
            "host-context: performEdit id={id} value={value_normalized:.7}"
        ));
        kResultOk
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        crate::diagnostics::log(format!("host-context: endEdit id={id}"));
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        crate::diagnostics::log(format!("host-context: restartComponent flags=0x{flags:x}"));
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

fn tuid_ptr_hex(tuid: *mut TUID) -> String {
    if tuid.is_null() {
        return "<null>".to_string();
    }
    let tuid = unsafe { &*tuid };
    tuid.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
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
    fn host_create_instance_rejects_unknown_class() {
        let ctx = HostContext::new();
        let app = ctx
            .to_com_ptr::<IHostApplication>()
            .expect("host context exposes IHostApplication");

        let mut cid: TUID = [0; 16];
        let mut iid: TUID = [0; 16];
        let mut obj: *mut c_void = std::ptr::null_mut();
        let result = unsafe { app.createInstance(&mut cid, &mut iid, &mut obj) };

        assert_eq!(result, kNotImplemented);
        assert!(obj.is_null());
    }

    #[test]
    fn host_create_instance_vends_vst_message() {
        let ctx = HostContext::new();
        let app = ctx
            .to_com_ptr::<IHostApplication>()
            .expect("host context exposes IHostApplication");

        let mut cid = IMessage_iid;
        let mut iid = IMessage_iid;
        let mut obj: *mut c_void = std::ptr::null_mut();
        let result = unsafe { app.createInstance(&mut cid, &mut iid, &mut obj) };

        assert_eq!(result, kResultOk);
        assert!(!obj.is_null());
        let message = unsafe { ComPtr::from_raw(obj.cast::<IMessage>()) }.expect("IMessage");
        let id = std::ffi::CString::new("galad.test").unwrap();
        unsafe {
            message.setMessageID(id.as_ptr());
            let returned = std::ffi::CStr::from_ptr(message.getMessageID())
                .to_str()
                .unwrap();
            assert_eq!(returned, "galad.test");
            assert!(!message.getAttributes().is_null());
        }
    }

    #[test]
    fn host_create_instance_vends_vst_attribute_list() {
        let ctx = HostContext::new();
        let app = ctx
            .to_com_ptr::<IHostApplication>()
            .expect("host context exposes IHostApplication");

        let mut cid = IAttributeList_iid;
        let mut iid = IAttributeList_iid;
        let mut obj: *mut c_void = std::ptr::null_mut();
        let result = unsafe { app.createInstance(&mut cid, &mut iid, &mut obj) };

        assert_eq!(result, kResultOk);
        assert!(!obj.is_null());
        let attributes =
            unsafe { ComPtr::from_raw(obj.cast::<IAttributeList>()) }.expect("IAttributeList");
        let key = b"answer\0";
        unsafe {
            assert_eq!(
                attributes.setInt(key.as_ptr().cast::<std::ffi::c_char>(), 42),
                kResultOk
            );
            let mut value = 0;
            assert_eq!(
                attributes.getInt(key.as_ptr().cast::<std::ffi::c_char>(), &mut value),
                kResultOk
            );
            assert_eq!(value, 42);
        }
    }
}
