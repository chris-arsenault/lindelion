use std::{ffi::c_void, ptr};

use crate::PluginDescriptor;
use vst3::{Class, ComPtr, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::{copy_cstring, copy_wstring};

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
