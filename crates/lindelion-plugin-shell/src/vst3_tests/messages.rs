use std::{
    cell::RefCell,
    ffi::{CStr, CString as StdCString},
    ptr,
};

use super::*;
use vst3::{
    Class, ComWrapper,
    Steinberg::{
        FIDString,
        Vst::{IAttributeList, IMessage, IMessageTrait},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestMessage {
    PatchUpdate,
    TelemetryRequest,
}

impl PluginMessageType for TestMessage {
    fn id(self) -> &'static str {
        match self {
            Self::PatchUpdate => "lindelion.test.patch_update",
            Self::TelemetryRequest => "lindelion.test.telemetry_request",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "lindelion.test.patch_update" => Some(Self::PatchUpdate),
            "lindelion.test.telemetry_request" => Some(Self::TelemetryRequest),
            _ => None,
        }
    }
}

#[test]
fn typed_message_roundtrips_payload() {
    let expected = TypedPluginMessage::new(TestMessage::PatchUpdate, b"patch".to_vec());
    let message = PluginMessage::from_typed(expected.clone())
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

    assert_eq!(decoded, Ok(Some(expected)));
}

#[test]
fn unknown_message_ids_are_ignored() {
    let message = PluginMessage::with_payload("lindelion.test.unknown", Vec::new())
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

    assert_eq!(decoded, Ok(None));
}

#[test]
fn malformed_message_payload_returns_error_instead_of_panicking() {
    let message = ComWrapper::new(MessageWithoutAttributes::new(TestMessage::PatchUpdate.id()))
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

    assert_eq!(decoded, Err(PluginMessageDecodeError::MissingPayload));
}

struct MessageWithoutAttributes {
    message_id: RefCell<StdCString>,
}

impl MessageWithoutAttributes {
    fn new(id: &str) -> Self {
        Self {
            message_id: RefCell::new(StdCString::new(id).unwrap()),
        }
    }
}

impl Class for MessageWithoutAttributes {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for MessageWithoutAttributes {
    unsafe fn getMessageID(&self) -> FIDString {
        self.message_id.borrow().as_ptr()
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        if id.is_null() {
            self.message_id.replace(StdCString::default());
        } else {
            self.message_id.replace(CStr::from_ptr(id).to_owned());
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        ptr::null_mut()
    }
}
