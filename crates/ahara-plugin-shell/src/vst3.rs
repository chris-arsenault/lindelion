#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString as StdCString, c_char, c_void},
    ptr, slice,
};

use crate::PluginDescriptor;
use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

const MESSAGE_ATTRIBUTE_PAYLOAD: &[u8] = b"payload\0";

pub trait PluginMessageType: Copy + Eq {
    fn id(self) -> &'static str;
    fn from_id(id: &str) -> Option<Self>;
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

pub unsafe fn message_id(message: *mut IMessage) -> Option<String> {
    let message = ComRef::from_raw(message)?;
    let id = message.getMessageID();
    if id.is_null() {
        return None;
    }
    Some(CStr::from_ptr(id).to_string_lossy().into_owned())
}

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

pub type Vst3CreateInstance = fn() -> ComPtr<FUnknown>;

#[derive(Debug, Clone, Copy)]
pub struct Vst3ClassRegistration {
    pub cid: TUID,
    pub category: &'static str,
    pub name: &'static str,
    pub class_flags: u32,
    pub subcategories: &'static str,
    pub create: Vst3CreateInstance,
}

impl Vst3ClassRegistration {
    pub const fn audio_processor(
        cid: TUID,
        name: &'static str,
        subcategories: &'static str,
        create: Vst3CreateInstance,
    ) -> Self {
        Self {
            cid,
            category: "Audio Module Class",
            name,
            class_flags: ComponentFlags_::kDistributable,
            subcategories,
            create,
        }
    }

    pub const fn edit_controller(
        cid: TUID,
        name: &'static str,
        create: Vst3CreateInstance,
    ) -> Self {
        Self {
            cid,
            category: "Component Controller Class",
            name,
            class_flags: 0,
            subcategories: "",
            create,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vst3PluginFactory {
    descriptor: &'static PluginDescriptor,
    classes: &'static [Vst3ClassRegistration],
    sdk_version: &'static str,
}

impl Vst3PluginFactory {
    pub const fn new(
        descriptor: &'static PluginDescriptor,
        classes: &'static [Vst3ClassRegistration],
    ) -> Self {
        Self {
            descriptor,
            classes,
            sdk_version: "VST 3.8.0",
        }
    }

    pub const fn class_count(&self) -> usize {
        self.classes.len()
    }

    pub fn class(&self, index: i32) -> Option<Vst3ClassRegistration> {
        let index = usize::try_from(index).ok()?;
        self.classes.get(index).copied()
    }
}

impl Class for Vst3PluginFactory {
    type Interfaces = (IPluginFactory, IPluginFactory2, IPluginFactory3);
}

impl IPluginFactoryTrait for Vst3PluginFactory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        copy_cstring(self.descriptor.vendor, &mut info.vendor);
        copy_cstring(self.descriptor.url, &mut info.url);
        copy_cstring(self.descriptor.email, &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as i32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        self.classes.len().min(i32::MAX as usize) as i32
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info(&mut *info);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();

        let requested_cid = *(cid as *const TUID);
        let Some(class) = self
            .classes
            .iter()
            .copied()
            .find(|class| class.cid == requested_cid)
        else {
            return kInvalidArgument;
        };

        let instance = (class.create)();
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

impl IPluginFactory2Trait for Vst3PluginFactory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info2(self.descriptor, self.sdk_version, &mut *info);
        kResultOk
    }
}

impl IPluginFactory3Trait for Vst3PluginFactory {
    unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info_w(self.descriptor, self.sdk_version, &mut *info);
        kResultOk
    }

    unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
}

impl Vst3ClassRegistration {
    fn fill_class_info(self, info: &mut PClassInfo) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_cstring(self.name, &mut info.name);
    }

    fn fill_class_info2(
        self,
        descriptor: &PluginDescriptor,
        sdk_version: &str,
        info: &mut PClassInfo2,
    ) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_cstring(self.name, &mut info.name);
        info.classFlags = self.class_flags;
        copy_cstring(self.subcategories, &mut info.subCategories);
        copy_cstring(descriptor.vendor, &mut info.vendor);
        copy_cstring(descriptor.version, &mut info.version);
        copy_cstring(sdk_version, &mut info.sdkVersion);
    }

    fn fill_class_info_w(
        self,
        descriptor: &PluginDescriptor,
        sdk_version: &str,
        info: &mut PClassInfoW,
    ) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_wstring(self.name, &mut info.name);
        info.classFlags = self.class_flags;
        copy_cstring(self.subcategories, &mut info.subCategories);
        copy_wstring(descriptor.vendor, &mut info.vendor);
        copy_wstring(descriptor.version, &mut info.version);
        copy_wstring(sdk_version, &mut info.sdkVersion);
    }
}

pub fn plugin_factory_ptr(factory: Vst3PluginFactory) -> *mut IPluginFactory {
    ComWrapper::new(factory)
        .to_com_ptr::<IPluginFactory>()
        .expect("Vst3PluginFactory must expose IPluginFactory")
        .into_raw()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedSizePlugViewSize {
    pub width: i32,
    pub height: i32,
}

impl FixedSizePlugViewSize {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    pub fn view_rect(self) -> ViewRect {
        ViewRect {
            left: 0,
            top: 0,
            right: self.width.max(0),
            bottom: self.height.max(0),
        }
    }

    fn clamp_rect(self, rect: ViewRect) -> ViewRect {
        ViewRect {
            left: rect.left,
            top: rect.top,
            right: rect.left + self.width.max(0),
            bottom: rect.top + self.height.max(0),
        }
    }
}

pub trait FixedSizePlugViewDelegate {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult;
    unsafe fn removed(&self) -> tresult {
        kResultOk
    }
}

pub struct FixedSizePlugView<D> {
    delegate: D,
    frame: Cell<*mut IPlugFrame>,
    size: Cell<ViewRect>,
    fixed_size: FixedSizePlugViewSize,
}

impl<D> FixedSizePlugView<D> {
    pub fn new(delegate: D, fixed_size: FixedSizePlugViewSize) -> Self {
        Self {
            delegate,
            frame: Cell::new(ptr::null_mut()),
            size: Cell::new(fixed_size.view_rect()),
            fixed_size,
        }
    }
}

impl<D: 'static> Class for FixedSizePlugView<D> {
    type Interfaces = (IPlugView,);
}

impl<D: FixedSizePlugViewDelegate> IPlugViewTrait for FixedSizePlugView<D> {
    unsafe fn isPlatformTypeSupported(&self, r#type: FIDString) -> tresult {
        #[cfg(target_os = "macos")]
        {
            if is_ns_view_platform(r#type) {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = r#type;
            kResultFalse
        }
    }

    unsafe fn attached(&self, parent: *mut c_void, r#type: FIDString) -> tresult {
        if parent.is_null() {
            return kInvalidArgument;
        }
        if self.isPlatformTypeSupported(r#type) != kResultTrue {
            return kResultFalse;
        }

        #[cfg(target_os = "macos")]
        {
            self.delegate.attached(parent, self.size.get())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        self.delegate.removed()
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyDown(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyUp(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        *size = self.size.get();
        kResultOk
    }

    unsafe fn onSize(&self, newSize: *mut ViewRect) -> tresult {
        if newSize.is_null() {
            return kInvalidArgument;
        }
        let size = self.fixed_size.clamp_rect(*newSize);
        self.size.set(size);
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        self.frame.set(frame);
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        kResultFalse
    }

    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        if rect.is_null() {
            return kInvalidArgument;
        }
        *rect = self.fixed_size.view_rect();
        kResultOk
    }
}

#[cfg(target_os = "macos")]
unsafe fn is_ns_view_platform(platform: FIDString) -> bool {
    !platform.is_null() && CStr::from_ptr(platform as *const c_char).to_bytes() == b"NSView"
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
mod tests {
    use super::*;
    use vst3::{Interface, uid};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMessage {
        PatchUpdate,
        TelemetryRequest,
    }

    impl PluginMessageType for TestMessage {
        fn id(self) -> &'static str {
            match self {
                Self::PatchUpdate => "ahara.test.patch_update",
                Self::TelemetryRequest => "ahara.test.telemetry_request",
            }
        }

        fn from_id(id: &str) -> Option<Self> {
            match id {
                "ahara.test.patch_update" => Some(Self::PatchUpdate),
                "ahara.test.telemetry_request" => Some(Self::TelemetryRequest),
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
        let message = PluginMessage::with_payload("ahara.test.unknown", Vec::new())
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

    #[test]
    fn string_helpers_null_terminate_truncated_strings() {
        let mut text = [1 as c_char; 4];
        copy_cstring("abcd", &mut text);

        assert_eq!(text[3], 0);

        let mut wide = [1 as TChar; 4];
        copy_wstring("abcd", &mut wide);

        assert_eq!(wide[3], 0);
    }

    #[test]
    fn string_helpers_roundtrip_ascii_and_unicode_without_overflow() {
        let mut text = [1 as c_char; 16];
        copy_cstring("Ahara", &mut text);

        assert_eq!(c_string(&text), "Ahara");
        assert_eq!(text[6], 1);

        let mut wide = [1 as TChar; 16];
        copy_wstring("Résonateur", &mut wide);

        assert_eq!(wide_string(&wide), "Résonateur");
        assert_eq!(wide[10], 0);
        assert_eq!(wide[11], 1);
    }

    #[test]
    fn factory_enumerates_registered_classes_through_ipluginfactory() {
        let factory = unsafe { ComPtr::from_raw(plugin_factory_ptr(test_vst3_factory())).unwrap() };

        assert_eq!(unsafe { factory.countClasses() }, 2);

        let mut processor = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(0, &mut processor) },
            kResultOk
        );
        assert_eq!(processor.cid, TEST_PROCESSOR_CID);
        assert_eq!(c_string(&processor.category), "Audio Module Class");
        assert_eq!(c_string(&processor.name), "Ahara Test Processor");

        let mut controller = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(1, &mut controller) },
            kResultOk
        );
        assert_eq!(controller.cid, TEST_CONTROLLER_CID);
        assert_eq!(c_string(&controller.category), "Component Controller Class");
        assert_eq!(c_string(&controller.name), "Ahara Test Controller");
    }

    #[test]
    fn factory_dispatches_class_creation_by_cid() {
        let factory = test_vst3_factory();
        let mut obj = ptr::null_mut::<c_void>();

        assert_eq!(
            unsafe {
                factory.createInstance(
                    TEST_PROCESSOR_CID.as_ptr(),
                    IPluginBase::IID.as_ptr().cast(),
                    &mut obj,
                )
            },
            kResultOk
        );

        let plugin_base = unsafe { ComPtr::from_raw(obj.cast::<IPluginBase>()).unwrap() };
        assert_eq!(
            unsafe { plugin_base.initialize(ptr::null_mut()) },
            kResultOk
        );

        let mut missing = std::ptr::dangling_mut::<c_void>();
        assert_eq!(
            unsafe {
                factory.createInstance(
                    TEST_UNKNOWN_CID.as_ptr(),
                    IPluginBase::IID.as_ptr().cast(),
                    &mut missing,
                )
            },
            kInvalidArgument
        );
        assert!(missing.is_null());
    }

    #[test]
    fn fixed_size_plug_view_reports_and_enforces_declared_size() {
        let view =
            FixedSizePlugView::new(TestPlugViewDelegate, FixedSizePlugViewSize::new(320, 180));

        let mut size = unsafe { std::mem::zeroed::<ViewRect>() };
        assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
        assert_rect(size, 0, 0, 320, 180);

        let mut requested = rect(12, 24, 640, 480);
        assert_eq!(unsafe { view.onSize(&mut requested) }, kResultOk);
        assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
        assert_rect(size, 12, 24, 332, 204);

        assert_eq!(unsafe { view.canResize() }, kResultFalse);

        let mut constrained = rect(8, 16, 100, 100);
        assert_eq!(
            unsafe { view.checkSizeConstraint(&mut constrained) },
            kResultOk
        );
        assert_rect(constrained, 0, 0, 320, 180);
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

    static TEST_DESCRIPTOR: PluginDescriptor =
        PluginDescriptor::instrument("Ahara Test", *b"ahara_test_plug!");

    const TEST_PROCESSOR_CID: TUID = uid(0x98E5D65D, 0x3B32489D, 0x89498A31, 0x4544F110);
    const TEST_CONTROLLER_CID: TUID = uid(0x2B77C756, 0x2E144A2A, 0xB05B702D, 0x797DD064);
    const TEST_UNKNOWN_CID: TUID = uid(0x530C977B, 0xB1004DB7, 0xB24EF0FE, 0xF7F1A040);

    const TEST_CLASSES: &[Vst3ClassRegistration] = &[
        Vst3ClassRegistration::audio_processor(
            TEST_PROCESSOR_CID,
            "Ahara Test Processor",
            "Instrument|Synth",
            create_test_component,
        ),
        Vst3ClassRegistration::edit_controller(
            TEST_CONTROLLER_CID,
            "Ahara Test Controller",
            create_test_component,
        ),
    ];

    fn test_vst3_factory() -> Vst3PluginFactory {
        Vst3PluginFactory::new(&TEST_DESCRIPTOR, TEST_CLASSES)
    }

    fn create_test_component() -> ComPtr<FUnknown> {
        ComWrapper::new(TestPluginBase)
            .to_com_ptr::<FUnknown>()
            .expect("TestPluginBase must expose FUnknown")
    }

    struct TestPluginBase;

    impl Class for TestPluginBase {
        type Interfaces = (IPluginBase,);
    }

    impl IPluginBaseTrait for TestPluginBase {
        unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
            kResultOk
        }

        unsafe fn terminate(&self) -> tresult {
            kResultOk
        }
    }

    struct TestPlugViewDelegate;

    impl FixedSizePlugViewDelegate for TestPlugViewDelegate {
        unsafe fn attached(&self, _parent: *mut c_void, _size: ViewRect) -> tresult {
            kResultOk
        }
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }

    fn wide_string(buffer: &[TChar]) -> String {
        let len = unsafe { len_wstring(buffer.as_ptr()) };
        let chars = buffer[..len]
            .iter()
            .map(|value| *value as u16)
            .collect::<Vec<_>>();
        String::from_utf16(&chars).unwrap()
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ViewRect {
        ViewRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn assert_rect(rect: ViewRect, left: i32, top: i32, right: i32, bottom: i32) {
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (left, top, right, bottom)
        );
    }
}
