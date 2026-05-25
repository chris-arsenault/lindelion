use std::{cell::Cell, ptr, slice};

use crate::{ParameterInfo as ShellParameterInfo, ProcessSetup as ShellProcessSetup};
use vst3::{ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::{PluginMessage, PluginMessageType, TypedPluginMessage, copy_wstring, len_wstring};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vst3ParameterChange {
    pub id: u32,
    pub sample_offset: i32,
    pub normalized_value: f64,
}

/// Visit the last point in each VST3 parameter queue.
///
/// VST3 parameter queues can contain sample-accurate automation points. Lindelion's current
/// plugin runtime consumes one normalized value per block, so this helper centralizes the
/// existing last-point policy while keeping plugin-specific parameter handling in the caller.
///
/// # Safety
/// `changes` must be either null or a valid VST3 `IParameterChanges` pointer for the duration of
/// the call.
pub unsafe fn for_each_vst3_parameter_change(
    changes: *mut IParameterChanges,
    mut apply: impl FnMut(Vst3ParameterChange),
) {
    let Some(changes) = ComRef::from_raw(changes) else {
        return;
    };

    for index in 0..changes.getParameterCount() {
        let Some(queue) = ComRef::from_raw(changes.getParameterData(index)) else {
            continue;
        };
        let point_count = queue.getPointCount();
        if point_count <= 0 {
            continue;
        }

        let mut sample_offset = 0;
        let mut value = 0.0;
        if queue.getPoint(point_count - 1, &mut sample_offset, &mut value) == kResultTrue {
            apply(Vst3ParameterChange {
                id: queue.getParameterId(),
                sample_offset,
                normalized_value: value,
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vst3ParameterInfo {
    pub id: u32,
    pub title: &'static str,
    pub short_title: &'static str,
    pub units: &'static str,
    pub step_count: i32,
    pub default_normalized_value: f64,
    pub flags: i32,
}

impl Vst3ParameterInfo {
    pub fn from_parameter(parameter: ShellParameterInfo) -> Self {
        Self {
            id: parameter.id.0,
            title: parameter.name,
            short_title: parameter.name,
            units: parameter.units,
            step_count: parameter.step_count.map_or(0, |steps| steps as i32),
            default_normalized_value: f64::from(parameter.range.normalize(parameter.range.default)),
            flags: ParameterInfo_::ParameterFlags_::kCanAutomate,
        }
    }

    pub const fn hidden(mut self) -> Self {
        self.flags |= ParameterInfo_::ParameterFlags_::kIsHidden;
        self
    }
}

/// Fill a VST3 `ParameterInfo` from shared parameter metadata.
///
/// # Safety
/// `info` must be either null or a valid writable VST3 `ParameterInfo` pointer.
pub unsafe fn fill_vst3_parameter_info(
    spec: Vst3ParameterInfo,
    info: *mut ParameterInfo,
) -> tresult {
    if info.is_null() {
        return kInvalidArgument;
    }

    let info = &mut *info;
    info.id = spec.id;
    copy_wstring(spec.title, &mut info.title);
    copy_wstring(spec.short_title, &mut info.shortTitle);
    copy_wstring(spec.units, &mut info.units);
    info.stepCount = spec.step_count;
    info.defaultNormalizedValue = spec.default_normalized_value;
    info.unitId = 0;
    info.flags = spec.flags;
    kResultOk
}

/// Parse a VST3 UTF-16 parameter string as a plain f32 value.
///
/// # Safety
/// `string` must be either null or point to readable memory containing a null terminator.
pub unsafe fn parse_vst3_plain_value_string(string: *mut TChar) -> Option<f32> {
    if string.is_null() {
        return None;
    }
    let len = len_wstring(string as *const TChar);
    String::from_utf16(slice::from_raw_parts(string as *const u16, len))
        .ok()?
        .trim()
        .parse::<f32>()
        .ok()
}

/// Write a VST3 UTF-16 parameter display string.
///
/// # Safety
/// `string` must be either null or a valid writable VST3 `String128` pointer.
pub unsafe fn write_vst3_parameter_string(value: &str, string: *mut String128) -> tresult {
    if string.is_null() {
        return kInvalidArgument;
    }
    copy_wstring(value, &mut *string);
    kResultOk
}

pub fn sanitize_normalized_f64(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[derive(Debug)]
pub struct Vst3ParameterMirror<const N: usize> {
    values: Cell<[f64; N]>,
}

impl<const N: usize> Vst3ParameterMirror<N> {
    pub const fn new(values: [f64; N]) -> Self {
        Self {
            values: Cell::new(values),
        }
    }

    pub fn values(&self) -> [f64; N] {
        self.values.get()
    }

    pub fn replace(&self, values: [f64; N]) {
        self.values.set(values);
    }

    pub fn value(&self, index: usize) -> Option<f64> {
        self.values.get().get(index).copied()
    }

    pub fn set_normalized(&self, index: usize, normalized: f64, fallback: f64) -> Option<f64> {
        let mut values = self.values.get();
        let value = values.get_mut(index)?;
        *value = sanitize_normalized_f64(normalized, fallback);
        let sanitized = *value;
        self.values.set(values);
        Some(sanitized)
    }
}

/// Notify a VST3 component handler that parameter values changed.
///
/// # Safety
/// `handler` must be either null or a valid VST3 `IComponentHandler` pointer for the duration of
/// the call.
pub unsafe fn restart_vst3_parameter_values_changed(handler: *mut IComponentHandler) {
    let Some(handler) = ComRef::from_raw(handler) else {
        return;
    };
    handler.restartComponent(RestartFlags_::kParamValuesChanged);
}

pub fn process_setup_from_vst(setup: &ProcessSetup) -> ShellProcessSetup {
    let mode = if setup.processMode as ProcessModes == ProcessModes_::kOffline {
        crate::ProcessMode::Offline
    } else {
        crate::ProcessMode::Realtime
    };
    ShellProcessSetup {
        sample_rate: setup.sampleRate,
        max_block_size: setup.maxSamplesPerBlock.max(1) as usize,
        mode,
    }
}

pub fn can_process_32_bit_sample_size(symbolic_sample_size: i32) -> tresult {
    match symbolic_sample_size as SymbolicSampleSizes {
        SymbolicSampleSizes_::kSample32 => kResultOk,
        SymbolicSampleSizes_::kSample64 => kNotImplemented,
        _ => kInvalidArgument,
    }
}

pub fn mono_or_stereo_speaker_arrangement_supported(arrangement: SpeakerArrangement) -> bool {
    matches!(arrangement, SpeakerArr::kMono | SpeakerArr::kStereo)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vst3BusInfo {
    pub media_type: MediaTypes,
    pub direction: BusDirections,
    pub channel_count: i32,
    pub name: &'static str,
    pub bus_type: BusTypes,
    pub flags: u32,
}

impl Vst3BusInfo {
    pub const fn audio_input(channel_count: i32, name: &'static str) -> Self {
        Self {
            media_type: MediaTypes_::kAudio,
            direction: BusDirections_::kInput,
            channel_count,
            name,
            bus_type: BusTypes_::kMain,
            flags: BusInfo_::BusFlags_::kDefaultActive,
        }
    }

    pub const fn optional_audio_input(channel_count: i32, name: &'static str) -> Self {
        Self {
            media_type: MediaTypes_::kAudio,
            direction: BusDirections_::kInput,
            channel_count,
            name,
            bus_type: BusTypes_::kAux,
            flags: 0,
        }
    }

    pub const fn audio_output(channel_count: i32, name: &'static str) -> Self {
        Self {
            media_type: MediaTypes_::kAudio,
            direction: BusDirections_::kOutput,
            channel_count,
            name,
            bus_type: BusTypes_::kMain,
            flags: BusInfo_::BusFlags_::kDefaultActive,
        }
    }

    pub const fn event_input(channel_count: i32, name: &'static str) -> Self {
        Self {
            media_type: MediaTypes_::kEvent,
            direction: BusDirections_::kInput,
            channel_count,
            name,
            bus_type: BusTypes_::kMain,
            flags: BusInfo_::BusFlags_::kDefaultActive,
        }
    }

    pub const fn event_output(channel_count: i32, name: &'static str) -> Self {
        Self {
            media_type: MediaTypes_::kEvent,
            direction: BusDirections_::kOutput,
            channel_count,
            name,
            bus_type: BusTypes_::kMain,
            flags: BusInfo_::BusFlags_::kDefaultActive,
        }
    }
}

pub fn vst3_bus_count(
    buses: &[Vst3BusInfo],
    media_type: MediaType,
    direction: BusDirection,
) -> i32 {
    let media_type = media_type as MediaTypes;
    let direction = direction as BusDirections;
    buses
        .iter()
        .filter(|bus| bus.media_type == media_type && bus.direction == direction)
        .count()
        .min(i32::MAX as usize) as i32
}

/// Fill a VST3 `BusInfo` from a local bus table.
///
/// # Safety
/// `bus` must be either null or a valid writable `BusInfo` pointer.
pub unsafe fn fill_vst3_bus_info(
    buses: &[Vst3BusInfo],
    media_type: MediaType,
    direction: BusDirection,
    index: i32,
    bus: *mut BusInfo,
) -> tresult {
    if bus.is_null() || index < 0 {
        return kInvalidArgument;
    }

    let media_type_kind = media_type as MediaTypes;
    let direction_kind = direction as BusDirections;
    let Some(spec) = buses
        .iter()
        .filter(|spec| spec.media_type == media_type_kind && spec.direction == direction_kind)
        .nth(index as usize)
    else {
        return kInvalidArgument;
    };

    let bus = &mut *bus;
    bus.mediaType = media_type;
    bus.direction = direction;
    bus.channelCount = spec.channel_count;
    copy_wstring(spec.name, &mut bus.name);
    bus.busType = spec.bus_type as BusType;
    bus.flags = spec.flags;
    kResultOk
}

#[derive(Debug, Default)]
pub struct Vst3PeerConnection {
    peer: Cell<*mut IConnectionPoint>,
}

impl Vst3PeerConnection {
    pub const fn new() -> Self {
        Self {
            peer: Cell::new(ptr::null_mut()),
        }
    }

    pub fn peer(&self) -> *mut IConnectionPoint {
        self.peer.get()
    }

    pub fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.set(other);
        kResultOk
    }

    pub fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        if self.peer.get() == other {
            self.peer.set(ptr::null_mut());
        }
        kResultOk
    }

    pub fn notify(&self, message: ComWrapper<PluginMessage>) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultFalse;
        };
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }

    pub fn notify_if_connected(&self, message: ComWrapper<PluginMessage>) -> tresult {
        if self.peer.get().is_null() {
            return kResultOk;
        }
        self.notify(message)
    }

    pub fn notify_typed<M: PluginMessageType>(&self, message: TypedPluginMessage<M>) -> tresult {
        self.notify(PluginMessage::from_typed(message))
    }
}

pub fn notify_vst3_patch_update<P, E>(
    peer: &Vst3PeerConnection,
    patch: &P,
    encode_patch: impl FnOnce(&P) -> Result<String, E>,
    message: impl FnOnce(Vec<u8>) -> ComWrapper<PluginMessage>,
) -> tresult {
    let Ok(payload) = encode_patch(patch) else {
        return kResultFalse;
    };
    peer.notify(message(payload.into_bytes()))
}
