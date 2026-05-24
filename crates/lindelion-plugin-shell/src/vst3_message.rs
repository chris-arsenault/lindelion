use std::{
    cell::RefCell,
    ffi::{CStr, CString as StdCString, c_char, c_void},
    ptr, slice,
};

use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

const MESSAGE_ATTRIBUTE_PAYLOAD: &[u8] = b"payload\0";

pub trait PluginMessageType: Copy + Eq {
    fn id(self) -> &'static str;
    fn from_id(id: &str) -> Option<Self>;
}

pub trait PluginMessagePayload: Sized {
    fn into_payload(self) -> Vec<u8>;
    fn from_payload(payload: Vec<u8>) -> Result<Self, PluginMessageDecodeError>;
}

impl PluginMessagePayload for Vec<u8> {
    fn into_payload(self) -> Vec<u8> {
        self
    }

    fn from_payload(payload: Vec<u8>) -> Result<Self, PluginMessageDecodeError> {
        Ok(payload)
    }
}

impl PluginMessagePayload for () {
    fn into_payload(self) -> Vec<u8> {
        Vec::new()
    }

    fn from_payload(payload: Vec<u8>) -> Result<Self, PluginMessageDecodeError> {
        if payload.is_empty() {
            Ok(())
        } else {
            Err(PluginMessageDecodeError::MalformedPayload)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPluginMessage<M> {
    pub kind: M,
    pub payload: Vec<u8>,
}

impl<M: PluginMessageType> TypedPluginMessage<M> {
    pub fn new(kind: M, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    pub fn empty(kind: M) -> Self {
        Self {
            kind,
            payload: Vec::new(),
        }
    }

    pub fn id(&self) -> &'static str {
        self.kind.id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginMessageDecodeError {
    MissingMessageId,
    MissingPayload,
    MalformedPayload,
}

/// Decode a VST3 message into a typed Lindelion message.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn decode_typed_message<M: PluginMessageType>(
    message: *mut IMessage,
) -> Result<Option<TypedPluginMessage<M>>, PluginMessageDecodeError> {
    let id = message_id(message).ok_or(PluginMessageDecodeError::MissingMessageId)?;
    let Some(kind) = M::from_id(&id) else {
        return Ok(None);
    };
    let payload = message_payload(message).ok_or(PluginMessageDecodeError::MissingPayload)?;
    Ok(Some(TypedPluginMessage::new(kind, payload)))
}

pub struct PluginAttributes {
    payload: RefCell<Vec<u8>>,
}

impl PluginAttributes {
    pub fn new(payload: Vec<u8>) -> Self {
        Self {
            payload: RefCell::new(payload),
        }
    }
}

impl Class for PluginAttributes {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for PluginAttributes {
    unsafe fn setInt(&self, _id: IAttrID, _value: int64) -> tresult {
        kNotImplemented
    }

    unsafe fn getInt(&self, _id: IAttrID, _value: *mut int64) -> tresult {
        kNotImplemented
    }

    unsafe fn setFloat(&self, _id: IAttrID, _value: f64) -> tresult {
        kNotImplemented
    }

    unsafe fn getFloat(&self, _id: IAttrID, _value: *mut f64) -> tresult {
        kNotImplemented
    }

    unsafe fn setString(&self, _id: IAttrID, _string: *const TChar) -> tresult {
        kNotImplemented
    }

    unsafe fn getString(&self, _id: IAttrID, _string: *mut TChar, _sizeInBytes: uint32) -> tresult {
        kNotImplemented
    }

    unsafe fn setBinary(&self, id: IAttrID, data: *const c_void, sizeInBytes: uint32) -> tresult {
        if !is_payload_attribute(id) || (data.is_null() && sizeInBytes > 0) {
            return kResultFalse;
        }
        let bytes = if sizeInBytes == 0 {
            Vec::new()
        } else {
            slice::from_raw_parts(data.cast::<u8>(), sizeInBytes as usize).to_vec()
        };
        self.payload.replace(bytes);
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: IAttrID,
        data: *mut *const c_void,
        sizeInBytes: *mut uint32,
    ) -> tresult {
        if !is_payload_attribute(id) || data.is_null() || sizeInBytes.is_null() {
            return kResultFalse;
        }
        let payload = self.payload.borrow();
        *data = payload.as_ptr().cast::<c_void>();
        *sizeInBytes = payload.len().min(u32::MAX as usize) as uint32;
        kResultOk
    }
}

pub struct PluginMessage {
    message_id: RefCell<StdCString>,
    attributes: ComPtr<IAttributeList>,
}

impl PluginMessage {
    pub fn with_payload(id: &str, payload: Vec<u8>) -> ComWrapper<Self> {
        let attributes = ComWrapper::new(PluginAttributes::new(payload))
            .to_com_ptr::<IAttributeList>()
            .expect("PluginAttributes must expose IAttributeList");
        ComWrapper::new(Self {
            message_id: RefCell::new(StdCString::new(id).unwrap_or_default()),
            attributes,
        })
    }

    pub fn from_typed<M: PluginMessageType>(message: TypedPluginMessage<M>) -> ComWrapper<Self> {
        Self::with_payload(message.id(), message.payload)
    }
}

impl Class for PluginMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for PluginMessage {
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
        self.attributes.as_ptr()
    }
}

unsafe fn is_payload_attribute(id: IAttrID) -> bool {
    !id.is_null() && CStr::from_ptr(id).to_bytes_with_nul() == MESSAGE_ATTRIBUTE_PAYLOAD
}

/// Read a VST3 message id.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn message_id(message: *mut IMessage) -> Option<String> {
    let message = ComRef::from_raw(message)?;
    let id = message.getMessageID();
    if id.is_null() {
        return None;
    }
    Some(CStr::from_ptr(id).to_string_lossy().into_owned())
}

/// Read the Lindelion binary payload attribute from a VST3 message.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn message_payload(message: *mut IMessage) -> Option<Vec<u8>> {
    let message = ComRef::from_raw(message)?;
    let attributes = ComRef::from_raw(message.getAttributes())?;
    let mut data = ptr::null::<c_void>();
    let mut size = 0;
    if attributes.getBinary(
        MESSAGE_ATTRIBUTE_PAYLOAD.as_ptr().cast::<c_char>(),
        &mut data,
        &mut size,
    ) != kResultOk
        || (data.is_null() && size > 0)
    {
        return None;
    }
    Some(slice::from_raw_parts(data.cast::<u8>(), size as usize).to_vec())
}

#[macro_export]
macro_rules! define_vst3_plugin_messages {
    (
        $kind_vis:vis enum $kind:ident;
        $message_vis:vis enum $message:ident;
        prefix $prefix:literal;
        messages {
            empty {
                $($empty_variant:ident => $empty_id:literal),* $(,)?
            }
            payload {
                $($payload_variant:ident($payload_ty:ty) => $payload_id:literal),* $(,)?
            }
        }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $kind_vis enum $kind {
            $($empty_variant,)*
            $($payload_variant,)*
        }

        impl $crate::vst3::PluginMessageType for $kind {
            fn id(self) -> &'static str {
                match self {
                    $(Self::$empty_variant => concat!($prefix, $empty_id),)*
                    $(Self::$payload_variant => concat!($prefix, $payload_id),)*
                }
            }

            fn from_id(id: &str) -> Option<Self> {
                match id {
                    $(concat!($prefix, $empty_id) => Some(Self::$empty_variant),)*
                    $(concat!($prefix, $payload_id) => Some(Self::$payload_variant),)*
                    _ => None,
                }
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        $message_vis enum $message {
            $($empty_variant,)*
            $($payload_variant($payload_ty),)*
        }

        impl $message {
            $message_vis fn into_com_message(self) -> ::vst3::ComWrapper<$crate::vst3::PluginMessage> {
                let message = match self {
                    $(
                        Self::$empty_variant => {
                            $crate::vst3::TypedPluginMessage::empty($kind::$empty_variant)
                        },
                    )*
                    $(
                        Self::$payload_variant(payload) => {
                            $crate::vst3::TypedPluginMessage::new(
                                $kind::$payload_variant,
                                $crate::vst3::PluginMessagePayload::into_payload(payload),
                            )
                        },
                    )*
                };
                $crate::vst3::PluginMessage::from_typed(message)
            }

            /// Decode a Lindelion VST3 message.
            ///
            /// # Safety
            /// `message` must be either null or a valid VST3 `IMessage` pointer for the duration
            /// of the call.
            $message_vis unsafe fn decode(
                message: *mut ::vst3::Steinberg::Vst::IMessage,
            ) -> Result<Option<Self>, $crate::vst3::PluginMessageDecodeError> {
                let Some(message) = $crate::vst3::decode_typed_message::<$kind>(message)? else {
                    return Ok(None);
                };
                match message.kind {
                    $(
                        $kind::$empty_variant => {
                            <() as $crate::vst3::PluginMessagePayload>::from_payload(message.payload)
                                .map(|()| Some(Self::$empty_variant))
                        },
                    )*
                    $(
                        $kind::$payload_variant => {
                            <$payload_ty as $crate::vst3::PluginMessagePayload>::from_payload(
                                message.payload,
                            )
                            .map(|payload| Some(Self::$payload_variant(payload)))
                        },
                    )*
                }
            }
        }
    };
}
