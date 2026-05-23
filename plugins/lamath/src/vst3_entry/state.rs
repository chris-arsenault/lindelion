use std::ffi::c_void;

use lindelion_plugin_shell::PluginState;
use vst3::{ComRef, Steinberg::*};

const STATE_MAGIC: [u8; 4] = *b"AHRS";
const STATE_HEADER_BYTES: usize = 12;
const MAX_STATE_BYTES: usize = 1_048_576;

pub(super) unsafe fn read_plugin_state_from_stream(stream: *mut IBStream) -> Option<PluginState> {
    let stream = ComRef::from_raw(stream)?;
    let mut header = [0; STATE_HEADER_BYTES];
    if !read_exact(&stream, &mut header) {
        return None;
    }
    if header[..4] != STATE_MAGIC {
        return None;
    }

    let format_version = u32::from_le_bytes(header[4..8].try_into().ok()?);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().ok()?) as usize;
    if payload_len > MAX_STATE_BYTES {
        return None;
    }

    let mut payload = vec![0; payload_len];
    if !read_exact(&stream, &mut payload) {
        return None;
    }
    Some(PluginState {
        format_version,
        payload,
    })
}

pub(super) unsafe fn write_plugin_state_to_stream(
    stream: *mut IBStream,
    state: PluginState,
) -> bool {
    let Some(stream) = ComRef::from_raw(stream) else {
        return false;
    };
    if state.payload.len() > u32::MAX as usize {
        return false;
    }

    let mut header = [0; STATE_HEADER_BYTES];
    header[..4].copy_from_slice(&STATE_MAGIC);
    header[4..8].copy_from_slice(&state.format_version.to_le_bytes());
    header[8..12].copy_from_slice(&(state.payload.len() as u32).to_le_bytes());

    write_all(&stream, &header) && write_all(&stream, &state.payload)
}

unsafe fn read_exact(stream: &ComRef<IBStream>, buffer: &mut [u8]) -> bool {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut bytes_read = 0;
        let chunk_len = (buffer.len() - offset).min(i32::MAX as usize) as i32;
        let result = stream.read(
            buffer[offset..].as_mut_ptr().cast::<c_void>(),
            chunk_len,
            &mut bytes_read,
        );
        if result != kResultOk || bytes_read <= 0 {
            return false;
        }
        offset += bytes_read as usize;
    }
    true
}

unsafe fn write_all(stream: &ComRef<IBStream>, buffer: &[u8]) -> bool {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut bytes_written = 0;
        let chunk_len = (buffer.len() - offset).min(i32::MAX as usize) as i32;
        let result = stream.write(
            buffer[offset..].as_ptr().cast::<c_void>() as *mut c_void,
            chunk_len,
            &mut bytes_written,
        );
        if result != kResultOk || bytes_written <= 0 {
            return false;
        }
        offset += bytes_written as usize;
    }
    true
}
