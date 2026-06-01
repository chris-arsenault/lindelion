//! WASAPI duplex stream setup — resolve a [`DeviceRef`] to an `IAudioClient`, negotiate
//! exclusive→shared mode + format, initialize event-driven, and expose the capture/render service.
//! The stream is configured but **not started** (the engine starts it, M2 Step 8).

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE, AUDCLNT_SHAREMODE_EXCLUSIVE,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioCaptureClient, IAudioClient,
    IAudioRenderClient, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoTaskMemFree};
use windows::Win32::System::Threading::CreateEventW;
use windows::core::PCWSTR;

use super::devices::{AudioDirection, AudioError, device_enumerator};
use crate::audio::format::{SampleFormat, StreamFormat};
use crate::audio::negotiation::{ShareMode, choose};
use crate::session::DeviceRef;

// `WAVEFORMATEX::wFormatTag` values (avoid pulling extra `windows` feature modules for constants).
const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// 100-ns units per second (WASAPI `REFERENCE_TIME`).
const REFTIMES_PER_SEC: i64 = 10_000_000;

/// The render or capture service for a stream.
enum Service {
    Capture(IAudioCaptureClient),
    Render(IAudioRenderClient),
}

/// A configured (not started) WASAPI stream: client, service, event handle, and negotiated format.
pub struct WasapiStream {
    client: IAudioClient,
    service: Service,
    event: HANDLE,
    format: StreamFormat,
    buffer_frames: u32,
}

impl WasapiStream {
    /// Open and configure the capture (input) stream for `device`.
    pub fn open_capture(device: &DeviceRef) -> Result<Self, AudioError> {
        Self::open(device, AudioDirection::Input)
    }

    /// Open and configure the render (output) stream for `device`.
    pub fn open_render(device: &DeviceRef) -> Result<Self, AudioError> {
        Self::open(device, AudioDirection::Output)
    }

    /// The negotiated stream format.
    pub fn format(&self) -> StreamFormat {
        self.format
    }

    /// The buffer size in frames the device granted.
    pub fn buffer_frames(&self) -> u32 {
        self.buffer_frames
    }

    /// The event handle WASAPI signals when the buffer is ready.
    pub fn event(&self) -> HANDLE {
        self.event
    }

    /// The underlying audio client (for Start/Stop/GetCurrentPadding).
    pub fn client(&self) -> &IAudioClient {
        &self.client
    }

    /// The capture service, if this is an input stream.
    pub fn capture_client(&self) -> Option<&IAudioCaptureClient> {
        match &self.service {
            Service::Capture(client) => Some(client),
            Service::Render(_) => None,
        }
    }

    /// The render service, if this is an output stream.
    pub fn render_client(&self) -> Option<&IAudioRenderClient> {
        match &self.service {
            Service::Render(client) => Some(client),
            Service::Capture(_) => None,
        }
    }

    fn open(device: &DeviceRef, direction: AudioDirection) -> Result<Self, AudioError> {
        unsafe {
            let enumerator = device_enumerator()?;
            let wide: Vec<u16> = device.id.encode_utf16().chain(std::iter::once(0)).collect();
            let imm = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
            let client: IAudioClient = imm.Activate(CLSCTX_ALL, None)?;

            // The shared mix format is also the exclusive-mode probe candidate.
            let mix = client.GetMixFormat()?;
            let wave = *mix; // copy out of the packed COM allocation before referencing fields
            let exclusive_supported = client
                .IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, mix, None)
                .is_ok();

            let mut default_period: i64 = 0;
            let mut min_period: i64 = 0;
            client.GetDevicePeriod(Some(&mut default_period), Some(&mut min_period))?;

            let format = stream_format_from_wave(wave).ok_or(AudioError::UnsupportedFormat)?;
            let config = choose(
                exclusive_supported,
                format,
                hns_to_frames(min_period, format.sample_rate),
                format,
                hns_to_frames(default_period, format.sample_rate),
            );
            let share_mode = match config.share_mode {
                ShareMode::Exclusive => AUDCLNT_SHAREMODE_EXCLUSIVE,
                ShareMode::Shared => AUDCLNT_SHAREMODE_SHARED,
            };

            initialize_event_driven(
                &client,
                share_mode,
                config.buffer_frames,
                format.sample_rate,
                mix,
            )?;
            CoTaskMemFree(Some(mix as *const core::ffi::c_void));

            let buffer_frames = client.GetBufferSize()?;
            let event = CreateEventW(None, false, false, PCWSTR::null())?;
            client.SetEventHandle(event)?;

            let service = match direction {
                AudioDirection::Input => Service::Capture(client.GetService()?),
                AudioDirection::Output => Service::Render(client.GetService()?),
            };

            Ok(WasapiStream {
                client,
                service,
                event,
                format,
                buffer_frames,
            })
        }
    }
}

/// Read a device's mix-format sample rate without opening/starting a stream. `open` initializes both
/// exclusive and shared modes with this same mix format (it only negotiates share-mode + buffer
/// size, never the rate), so this is exactly the rate the engine will run at — the chain can be
/// prepared at it before starting. The host declares the true device rate to plugins; it never
/// resamples.
pub fn device_sample_rate(device: &DeviceRef) -> Result<u32, AudioError> {
    unsafe {
        let enumerator = device_enumerator()?;
        let wide: Vec<u16> = device.id.encode_utf16().chain(std::iter::once(0)).collect();
        let imm = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
        let client: IAudioClient = imm.Activate(CLSCTX_ALL, None)?;
        let mix = client.GetMixFormat()?;
        let rate = (*mix).nSamplesPerSec;
        CoTaskMemFree(Some(mix as *const core::ffi::c_void));
        Ok(rate)
    }
}

/// `Initialize` the client event-driven, retrying once with the device-aligned size on
/// `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` (the documented exclusive-mode retry).
unsafe fn initialize_event_driven(
    client: &IAudioClient,
    share_mode: AUDCLNT_SHAREMODE,
    buffer_frames: u32,
    sample_rate: u32,
    format: *const WAVEFORMATEX,
) -> Result<(), AudioError> {
    let periodicity = |hns: i64| -> i64 {
        if share_mode == AUDCLNT_SHAREMODE_EXCLUSIVE {
            hns
        } else {
            0
        }
    };
    let hns = frames_to_hns(buffer_frames, sample_rate);
    match client.Initialize(
        share_mode,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        hns,
        periodicity(hns),
        format,
        None,
    ) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            let aligned = client.GetBufferSize()?;
            let aligned_hns = frames_to_hns(aligned, sample_rate);
            client.Initialize(
                share_mode,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                aligned_hns,
                periodicity(aligned_hns),
                format,
                None,
            )?;
            Ok(())
        }
        Err(error) => Err(AudioError::Com(error)),
    }
}

fn hns_to_frames(hns: i64, sample_rate: u32) -> u32 {
    ((hns as i128 * sample_rate as i128) / REFTIMES_PER_SEC as i128) as u32
}

fn frames_to_hns(frames: u32, sample_rate: u32) -> i64 {
    ((frames as i128 * REFTIMES_PER_SEC as i128) / sample_rate.max(1) as i128) as i64
}

fn stream_format_from_wave(wave: WAVEFORMATEX) -> Option<StreamFormat> {
    let sample = match (wave.wFormatTag, wave.wBitsPerSample) {
        (WAVE_FORMAT_IEEE_FLOAT, 32) | (WAVE_FORMAT_EXTENSIBLE, 32) => SampleFormat::F32,
        (WAVE_FORMAT_PCM, 16) | (WAVE_FORMAT_EXTENSIBLE, 16) => SampleFormat::I16,
        _ => return None,
    };
    Some(StreamFormat {
        sample_rate: wave.nSamplesPerSec,
        channels: wave.nChannels,
        sample,
    })
}
