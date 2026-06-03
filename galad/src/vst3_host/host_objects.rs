//! Host-created VST3 utility objects.
//!
//! Some third-party plugins ask `IHostApplication::createInstance` for VST3 SDK utility objects,
//! especially `IMessage`, during controller/component messaging. These objects live on the
//! controller/UI side of the host, not on Galad's realtime audio path.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::{CStr, CString, c_void},
    ptr, slice,
};

use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

#[derive(Debug, Clone, PartialEq)]
enum HostAttribute {
    Int(int64),
    Float(f64),
    String(Vec<TChar>),
    Binary(Vec<u8>),
}

#[derive(Default)]
pub(super) struct HostAttributeList {
    attributes: RefCell<BTreeMap<String, HostAttribute>>,
}

impl HostAttributeList {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

impl Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: IAttrID, value: int64) -> tresult {
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        self.attributes
            .borrow_mut()
            .insert(key, HostAttribute::Int(value));
        kResultOk
    }

    unsafe fn getInt(&self, id: IAttrID, value: *mut int64) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        let attributes = self.attributes.borrow();
        let Some(HostAttribute::Int(stored)) = attributes.get(&key) else {
            return kResultFalse;
        };
        *value = *stored;
        kResultOk
    }

    unsafe fn setFloat(&self, id: IAttrID, value: f64) -> tresult {
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        self.attributes
            .borrow_mut()
            .insert(key, HostAttribute::Float(value));
        kResultOk
    }

    unsafe fn getFloat(&self, id: IAttrID, value: *mut f64) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        let attributes = self.attributes.borrow();
        let Some(HostAttribute::Float(stored)) = attributes.get(&key) else {
            return kResultFalse;
        };
        *value = *stored;
        kResultOk
    }

    unsafe fn setString(&self, id: IAttrID, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        self.attributes.borrow_mut().insert(
            key,
            HostAttribute::String(copy_tchar_nul_terminated(string)),
        );
        kResultOk
    }

    unsafe fn getString(&self, id: IAttrID, string: *mut TChar, sizeInBytes: uint32) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let char_capacity = (sizeInBytes as usize) / std::mem::size_of::<TChar>();
        if char_capacity == 0 {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        let attributes = self.attributes.borrow();
        let Some(HostAttribute::String(stored)) = attributes.get(&key) else {
            return kResultFalse;
        };

        let copy_len = stored.len().min(char_capacity.saturating_sub(1));
        if copy_len > 0 {
            ptr::copy_nonoverlapping(stored.as_ptr(), string, copy_len);
        }
        *string.add(copy_len) = 0;
        kResultOk
    }

    unsafe fn setBinary(&self, id: IAttrID, data: *const c_void, sizeInBytes: uint32) -> tresult {
        if data.is_null() && sizeInBytes > 0 {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        let bytes = if sizeInBytes == 0 {
            Vec::new()
        } else {
            slice::from_raw_parts(data.cast::<u8>(), sizeInBytes as usize).to_vec()
        };
        self.attributes
            .borrow_mut()
            .insert(key, HostAttribute::Binary(bytes));
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: IAttrID,
        data: *mut *const c_void,
        sizeInBytes: *mut uint32,
    ) -> tresult {
        if data.is_null() || sizeInBytes.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = attr_key(id) else {
            return kInvalidArgument;
        };
        let attributes = self.attributes.borrow();
        let Some(HostAttribute::Binary(stored)) = attributes.get(&key) else {
            return kResultFalse;
        };
        *data = stored.as_ptr().cast::<c_void>();
        *sizeInBytes = stored.len().min(u32::MAX as usize) as uint32;
        kResultOk
    }
}

pub(super) struct HostMessage {
    message_id: RefCell<CString>,
    attributes: ComPtr<IAttributeList>,
}

impl HostMessage {
    pub(super) fn new() -> Self {
        let attributes = ComWrapper::new(HostAttributeList::new())
            .to_com_ptr::<IAttributeList>()
            .expect("HostAttributeList exposes IAttributeList");
        Self {
            message_id: RefCell::new(CString::default()),
            attributes,
        }
    }
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        self.message_id.borrow().as_ptr()
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        if id.is_null() {
            self.message_id.replace(CString::default());
        } else {
            self.message_id.replace(CStr::from_ptr(id).to_owned());
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes.as_ptr()
    }
}

unsafe fn attr_key(id: IAttrID) -> Option<String> {
    if id.is_null() {
        return None;
    }
    Some(CStr::from_ptr(id).to_string_lossy().into_owned())
}

unsafe fn copy_tchar_nul_terminated(string: *const TChar) -> Vec<TChar> {
    let mut len = 0usize;
    while *string.add(len) != 0 {
        len += 1;
    }
    let mut out = slice::from_raw_parts(string, len).to_vec();
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_char;

    const INT_KEY: &[u8] = b"int\0";
    const FLOAT_KEY: &[u8] = b"float\0";
    const STRING_KEY: &[u8] = b"string\0";
    const BINARY_KEY: &[u8] = b"binary\0";

    #[test]
    fn attribute_list_round_trips_supported_value_types() {
        let attributes = ComWrapper::new(HostAttributeList::new())
            .to_com_ptr::<IAttributeList>()
            .expect("IAttributeList");

        unsafe {
            assert_eq!(attributes.setInt(attr(INT_KEY), -42), kResultOk);
            let mut int_value = 0;
            assert_eq!(attributes.getInt(attr(INT_KEY), &mut int_value), kResultOk);
            assert_eq!(int_value, -42);

            assert_eq!(attributes.setFloat(attr(FLOAT_KEY), 0.25), kResultOk);
            let mut float_value = 0.0;
            assert_eq!(
                attributes.getFloat(attr(FLOAT_KEY), &mut float_value),
                kResultOk
            );
            assert_eq!(float_value, 0.25);

            let input: Vec<TChar> = "hello".encode_utf16().chain([0]).collect();
            assert_eq!(
                attributes.setString(attr(STRING_KEY), input.as_ptr()),
                kResultOk
            );
            let mut output: [TChar; 8] = [0; 8];
            assert_eq!(
                attributes.getString(
                    attr(STRING_KEY),
                    output.as_mut_ptr(),
                    (output.len() * std::mem::size_of::<TChar>()) as uint32,
                ),
                kResultOk
            );
            assert_eq!(String::from_utf16_lossy(&output[..5]), "hello");

            let binary = [1u8, 2, 3, 4];
            assert_eq!(
                attributes.setBinary(
                    attr(BINARY_KEY),
                    binary.as_ptr().cast::<c_void>(),
                    binary.len() as uint32,
                ),
                kResultOk
            );
            let mut data = ptr::null::<c_void>();
            let mut size = 0;
            assert_eq!(
                attributes.getBinary(attr(BINARY_KEY), &mut data, &mut size),
                kResultOk
            );
            assert_eq!(size, binary.len() as uint32);
            assert_eq!(
                slice::from_raw_parts(data.cast::<u8>(), size as usize),
                binary
            );
        }
    }

    #[test]
    fn message_owns_general_attribute_list() {
        let message = ComWrapper::new(HostMessage::new())
            .to_com_ptr::<IMessage>()
            .expect("IMessage");
        let id = CString::new("meter").unwrap();

        unsafe {
            message.setMessageID(id.as_ptr());
            assert_eq!(CStr::from_ptr(message.getMessageID()).to_str(), Ok("meter"));

            let attributes = ComRef::from_raw(message.getAttributes()).expect("attributes");
            assert_eq!(attributes.setInt(attr(INT_KEY), 7), kResultOk);
            let mut value = 0;
            assert_eq!(attributes.getInt(attr(INT_KEY), &mut value), kResultOk);
            assert_eq!(value, 7);
        }
    }

    fn attr(bytes: &[u8]) -> IAttrID {
        bytes.as_ptr().cast::<c_char>()
    }
}
