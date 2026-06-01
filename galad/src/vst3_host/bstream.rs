//! A host-implemented in-memory `IBStream` — the stream the host hands to a plugin's
//! `IComponent::getState` (the plugin writes its state into it) and `setState` (the plugin reads its
//! state out of it). Backs a `Vec<u8>` with a read/write cursor.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use vst3::{Class, Steinberg::*};

// VST3 `IStreamSeekMode` values (kIBSeekSet/Cur/End).
const SEEK_SET: i32 = 0;
const SEEK_CUR: i32 = 1;
const SEEK_END: i32 = 2;

/// An in-memory `IBStream` over a `Vec<u8>` with a cursor.
pub struct MemoryStream {
    data: RefCell<Vec<u8>>,
    pos: Cell<i64>,
}

impl MemoryStream {
    /// An empty stream (cursor at 0).
    pub fn new() -> Self {
        Self {
            data: RefCell::new(Vec::new()),
            pos: Cell::new(0),
        }
    }

    /// A stream preloaded with `bytes` (cursor at 0).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            data: RefCell::new(bytes),
            pos: Cell::new(0),
        }
    }

    /// A copy of the buffer's current contents.
    pub fn bytes(&self) -> Vec<u8> {
        self.data.borrow().clone()
    }
}

impl Default for MemoryStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: i32,
        num_bytes_read: *mut i32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let data = self.data.borrow();
        let pos = self.pos.get().max(0) as usize;
        let available = data.len().saturating_sub(pos);
        let n = (num_bytes as usize).min(available);
        if n > 0 {
            std::ptr::copy_nonoverlapping(data[pos..].as_ptr(), buffer as *mut u8, n);
        }
        self.pos.set((pos + n) as i64);
        if !num_bytes_read.is_null() {
            *num_bytes_read = n as i32;
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: i32,
        num_bytes_written: *mut i32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let n = num_bytes as usize;
        let pos = self.pos.get().max(0) as usize;
        {
            let mut data = self.data.borrow_mut();
            if pos + n > data.len() {
                data.resize(pos + n, 0);
            }
            std::ptr::copy_nonoverlapping(buffer as *const u8, data[pos..].as_mut_ptr(), n);
        }
        self.pos.set((pos + n) as i64);
        if !num_bytes_written.is_null() {
            *num_bytes_written = n as i32;
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> tresult {
        let len = self.data.borrow().len() as i64;
        let base = match mode {
            SEEK_SET => 0,
            SEEK_CUR => self.pos.get(),
            SEEK_END => len,
            _ => return kInvalidArgument,
        };
        let new_pos = (base + pos).max(0);
        self.pos.set(new_pos);
        if !result.is_null() {
            *result = new_pos;
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut i64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        *pos = self.pos.get();
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use vst3::ComWrapper;

    use super::*;

    #[test]
    fn memory_stream_writes_then_reads_back() {
        let stream = ComWrapper::new(MemoryStream::new());
        let iface = stream.to_com_ptr::<IBStream>().expect("IBStream");

        let first = [1u8, 2, 3];
        let second = [4u8, 5];
        let mut written: i32 = 0;
        unsafe {
            iface.write(first.as_ptr() as *mut c_void, 3, &mut written);
            assert_eq!(written, 3);
            iface.write(second.as_ptr() as *mut c_void, 2, &mut written);
            assert_eq!(written, 2);

            let mut new_pos: i64 = -1;
            iface.seek(0, SEEK_SET, &mut new_pos);
            assert_eq!(new_pos, 0);

            let mut buf = [0u8; 5];
            let mut read: i32 = 0;
            iface.read(buf.as_mut_ptr() as *mut c_void, 5, &mut read);
            assert_eq!(read, 5);
            assert_eq!(buf, [1, 2, 3, 4, 5]);

            let mut tell: i64 = 0;
            iface.tell(&mut tell);
            assert_eq!(tell, 5);
        }

        assert_eq!(stream.bytes(), vec![1, 2, 3, 4, 5]);
    }
}
