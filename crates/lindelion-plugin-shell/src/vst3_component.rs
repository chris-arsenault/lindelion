use std::{cell::Cell, ptr};

use crate::ProcessSetup as ShellProcessSetup;
use vst3::{ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::{PluginMessage, PluginMessageType, TypedPluginMessage, copy_wstring};

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
