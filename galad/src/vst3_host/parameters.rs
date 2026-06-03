//! Host-side VST3 parameter-edit plumbing.
//!
//! Editor/controller `performEdit` calls arrive on the UI/control side. The audio thread consumes
//! them as normal VST3 `IParameterChanges` on the next process block.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use vst3::{Class, ComPtr, ComWrapper, Steinberg::Vst::*, Steinberg::*};

pub(super) const PARAMETER_EDIT_QUEUE_CAPACITY: usize = 2048;
pub(super) const MAX_BLOCK_PARAMETER_EDITS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ParameterEdit {
    pub id: ParamID,
    pub normalized_value: ParamValue,
}

impl ParameterEdit {
    pub(super) const fn new(id: ParamID, normalized_value: ParamValue) -> Self {
        Self {
            id,
            normalized_value,
        }
    }
}

impl Default for ParameterEdit {
    fn default() -> Self {
        Self::new(0, 0.0)
    }
}

/// Fixed-capacity SPSC ring for controller/UI parameter edits.
pub(super) struct ParameterEditQueue<const CAP: usize = PARAMETER_EDIT_QUEUE_CAPACITY> {
    read: AtomicUsize,
    write: AtomicUsize,
    dropped: AtomicUsize,
    ids: [AtomicU32; CAP],
    values: [AtomicU64; CAP],
}

impl<const CAP: usize> ParameterEditQueue<CAP> {
    pub(super) fn new() -> Self {
        assert!(CAP > 1);
        Self {
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            ids: std::array::from_fn(|_| AtomicU32::new(0)),
            values: std::array::from_fn(|_| AtomicU64::new(0.0f64.to_bits())),
        }
    }

    pub(super) fn push(&self, edit: ParameterEdit) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        let next = (write + 1) % CAP;
        if next == self.read.load(Ordering::Acquire) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.ids[write].store(edit.id, Ordering::Relaxed);
        self.values[write].store(
            sanitize_normalized(edit.normalized_value).to_bits(),
            Ordering::Relaxed,
        );
        self.write.store(next, Ordering::Release);
        true
    }

    pub(super) fn drain(&self, target: &mut [ParameterEdit]) -> usize {
        let mut read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        let mut count = 0usize;
        while read != write && count < target.len() {
            target[count] = ParameterEdit {
                id: self.ids[read].load(Ordering::Relaxed),
                normalized_value: f64::from_bits(self.values[read].load(Ordering::Relaxed)),
            };
            count += 1;
            read = (read + 1) % CAP;
        }
        self.read.store(read, Ordering::Release);
        count
    }

    #[cfg(test)]
    pub(super) fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl<const CAP: usize> Default for ParameterEditQueue<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) struct HostComponentHandler {
    edits: Arc<ParameterEditQueue>,
}

impl HostComponentHandler {
    pub(super) fn new(edits: Arc<ParameterEditQueue>) -> Self {
        Self { edits }
    }
}

impl Class for HostComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for HostComponentHandler {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        crate::diagnostics::log(format!("component-handler: beginEdit id={id}"));
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
        let value = sanitize_normalized(value_normalized);
        let pushed = self.edits.push(ParameterEdit::new(id, value));
        crate::diagnostics::log(format!(
            "component-handler: performEdit id={id} value={value:.7} queued={pushed}"
        ));
        kResultOk
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        crate::diagnostics::log(format!("component-handler: endEdit id={id}"));
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        crate::diagnostics::log(format!(
            "component-handler: restartComponent flags=0x{flags:x}"
        ));
        kResultOk
    }
}

pub(super) struct HostParameterChanges {
    len: Cell<usize>,
    queues: Vec<ComWrapper<HostParamValueQueue>>,
    queue_ptrs: Vec<ComPtr<IParamValueQueue>>,
}

impl HostParameterChanges {
    pub(super) fn new() -> ComWrapper<Self> {
        let mut queues = Vec::with_capacity(MAX_BLOCK_PARAMETER_EDITS);
        let mut queue_ptrs = Vec::with_capacity(MAX_BLOCK_PARAMETER_EDITS);
        for _ in 0..MAX_BLOCK_PARAMETER_EDITS {
            let queue = ComWrapper::new(HostParamValueQueue::default());
            let ptr = queue
                .to_com_ptr::<IParamValueQueue>()
                .expect("HostParamValueQueue exposes IParamValueQueue");
            queues.push(queue);
            queue_ptrs.push(ptr);
        }
        ComWrapper::new(Self {
            len: Cell::new(0),
            queues,
            queue_ptrs,
        })
    }

    pub(super) fn set_parameter_edits(&self, edits: &[ParameterEdit]) -> usize {
        let mut len = 0usize;
        for edit in edits.iter().take(self.queues.len()) {
            self.queues[len].set_edit(*edit, 0);
            len += 1;
        }
        self.len.set(len);
        len
    }
}

impl Class for HostParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for HostParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.len.get().min(i32::MAX as usize) as i32
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        if index < 0 {
            return std::ptr::null_mut();
        }
        self.queue_ptrs
            .get(index as usize)
            .filter(|_| (index as usize) < self.len.get())
            .map_or(std::ptr::null_mut(), ComPtr::as_ptr)
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut i32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        let len = self.len.get();
        if len >= self.queues.len() {
            return std::ptr::null_mut();
        }
        self.queues[len].set_empty(*id);
        self.len.set(len + 1);
        if !index.is_null() {
            *index = len as i32;
        }
        self.queue_ptrs[len].as_ptr()
    }
}

#[derive(Default)]
struct HostParamValueQueue {
    id: Cell<ParamID>,
    point_count: Cell<i32>,
    sample_offset: Cell<i32>,
    value: Cell<ParamValue>,
}

impl HostParamValueQueue {
    fn set_edit(&self, edit: ParameterEdit, sample_offset: i32) {
        self.id.set(edit.id);
        self.point_count.set(1);
        self.sample_offset.set(sample_offset);
        self.value.set(sanitize_normalized(edit.normalized_value));
    }

    fn set_empty(&self, id: ParamID) {
        self.id.set(id);
        self.point_count.set(0);
        self.sample_offset.set(0);
        self.value.set(0.0);
    }
}

impl Class for HostParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for HostParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> i32 {
        self.point_count.get()
    }

    unsafe fn getPoint(
        &self,
        index: i32,
        sample_offset: *mut i32,
        value: *mut ParamValue,
    ) -> tresult {
        if index != 0 || sample_offset.is_null() || value.is_null() || self.point_count.get() <= 0 {
            return kResultFalse;
        }
        *sample_offset = self.sample_offset.get();
        *value = self.value.get();
        kResultTrue
    }

    unsafe fn addPoint(&self, sample_offset: i32, value: ParamValue, index: *mut i32) -> tresult {
        self.point_count.set(1);
        self.sample_offset.set(sample_offset);
        self.value.set(sanitize_normalized(value));
        if !index.is_null() {
            *index = 0;
        }
        kResultOk
    }
}

fn sanitize_normalized(value: ParamValue) -> ParamValue {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_test_allocator::assert_no_allocations;
    use vst3::Steinberg::Vst::{IParamValueQueueTrait, IParameterChangesTrait};

    #[test]
    fn edit_queue_drains_in_order_and_drops_when_full() {
        let queue = ParameterEditQueue::<3>::new();
        assert!(queue.push(ParameterEdit::new(1, 0.25)));
        assert!(queue.push(ParameterEdit::new(2, 0.75)));
        assert!(!queue.push(ParameterEdit::new(3, 1.0)));
        assert_eq!(queue.dropped(), 1);

        let mut out = [ParameterEdit::default(); 4];
        let count = queue.drain(&mut out);
        assert_eq!(count, 2);
        assert_eq!(out[0], ParameterEdit::new(1, 0.25));
        assert_eq!(out[1], ParameterEdit::new(2, 0.75));
        assert_eq!(queue.drain(&mut out), 0);
    }

    #[test]
    fn edit_queue_drain_is_allocation_free() {
        let queue = ParameterEditQueue::<16>::new();
        assert!(queue.push(ParameterEdit::new(7, 0.5)));
        let mut out = [ParameterEdit::default(); 8];
        assert_no_allocations("parameter edit queue drain", || {
            assert_eq!(queue.drain(&mut out), 1);
        });
    }

    #[test]
    fn parameter_changes_exposes_vst3_queues() {
        let changes = HostParameterChanges::new();
        let iface = changes
            .to_com_ptr::<IParameterChanges>()
            .expect("IParameterChanges");
        let edits = [ParameterEdit::new(10, 0.2), ParameterEdit::new(11, 0.8)];

        assert_eq!(changes.set_parameter_edits(&edits), 2);
        assert_eq!(unsafe { iface.getParameterCount() }, 2);

        let queue = unsafe { vst3::ComRef::from_raw(iface.getParameterData(1)) }.expect("queue");
        assert_eq!(unsafe { queue.getParameterId() }, 11);
        assert_eq!(unsafe { queue.getPointCount() }, 1);
        let mut offset = -1;
        let mut value = 0.0;
        assert_eq!(
            unsafe { queue.getPoint(0, &mut offset, &mut value) },
            kResultTrue
        );
        assert_eq!(offset, 0);
        assert_eq!(value, 0.8);
    }
}
