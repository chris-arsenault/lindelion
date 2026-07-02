//! Realtime duplex engine — open capture + render streams sharing one [`Transport`], run an
//! event-driven RT thread (capture → ring → render passthrough), and report measured latency.
//!
//! Only WASAPI `GetBuffer`/`ReleaseBuffer` and the platform-neutral `Transport` run on the audio
//! thread; the scratch buffers are preallocated, so the per-callback path is allocation-free and
//! lock-free (ADR-0001). The plugin chain joins this path in M3.

use std::slice;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_SILENT;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::Threading::WaitForMultipleObjects;

use std::ptr;

use super::devices::AudioError;
use super::stream::WasapiStream;
use crate::audio::EngineStatus;
use crate::audio::format::{StreamFormat, bytes_to_f32, f32_to_bytes};
use crate::audio::meter::{
    MeterPublisher, MeterReader, MeterSnapshot, meter_channel, stereo_levels,
};
use crate::audio::transport::Transport;
use crate::midi::{MAX_BLOCK_MIDI_EVENTS, MidiEventQueue, MidiMessage};
use crate::session::DeviceRef;
use crate::vst3_host::{ChainProcessor, Handoff};

use super::midi_input::MidiInputs;

/// 100-ns units per millisecond.
const HNS_PER_MS: f64 = 10_000.0;

/// Post-chain master settings.
#[derive(Debug, Clone, Copy)]
pub struct MasterSettings {
    pub gain_db: f32,
    pub muted: bool,
}

impl Default for MasterSettings {
    fn default() -> Self {
        MasterSettings {
            gain_db: 0.0,
            muted: false,
        }
    }
}

struct MasterControl {
    gain_bits: AtomicU32,
    muted: AtomicBool,
}

impl MasterControl {
    fn new(settings: MasterSettings) -> Self {
        let control = MasterControl {
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            muted: AtomicBool::new(false),
        };
        control.set(settings);
        control
    }

    fn set(&self, settings: MasterSettings) {
        let linear = if settings.muted {
            0.0
        } else {
            db_to_linear(settings.gain_db)
        };
        self.gain_bits.store(linear.to_bits(), Ordering::Release);
        self.muted.store(settings.muted, Ordering::Release);
    }

    fn gain_linear(&self) -> f32 {
        if self.muted.load(Ordering::Acquire) {
            0.0
        } else {
            f32::from_bits(self.gain_bits.load(Ordering::Acquire))
        }
    }
}

fn db_to_linear(gain_db: f32) -> f32 {
    10.0f32.powf(gain_db.clamp(-60.0, 12.0) / 20.0)
}

/// Measured round-trip latency components, in milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct MeasuredLatency {
    pub capture_buffer_ms: f64,
    pub render_buffer_ms: f64,
    pub capture_stream_ms: f64,
    pub render_stream_ms: f64,
    /// Aggregate reported latency of the (initial) plugin chain.
    pub chain_latency_ms: f64,
    pub total_ms: f64,
}

/// A running engine: mic → (optional VST3 chain) → output. Stops (and joins its thread) on `stop`
/// or drop.
pub struct AudioEngine {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    handoff: Arc<Handoff<ChainProcessor>>,
    meter: MeterReader,
    /// Run status published by the realtime thread (`EngineStatus` encoded); read off-thread so the
    /// control side can detect a runtime fault (e.g. device invalidation).
    status: Arc<AtomicU8>,
    master: Arc<MasterControl>,
    latency: MeasuredLatency,
}

impl AudioEngine {
    /// Start live passthrough from `input` to `output` (no plugin chain).
    pub fn start(input: DeviceRef, output: DeviceRef) -> Result<Self, AudioError> {
        Self::start_internal(input, output, None, MasterSettings::default())
    }

    /// Start live processing through `initial` from `input` to `output`. Returns once the stream is
    /// running, carrying the measured latency (including the chain), or the setup error.
    pub fn start_with_chain(
        input: DeviceRef,
        output: DeviceRef,
        initial: Box<ChainProcessor>,
    ) -> Result<Self, AudioError> {
        Self::start_internal(input, output, Some(initial), MasterSettings::default())
    }

    /// Start live processing with post-chain master settings.
    pub fn start_with_chain_and_master(
        input: DeviceRef,
        output: DeviceRef,
        initial: Box<ChainProcessor>,
        master: MasterSettings,
    ) -> Result<Self, AudioError> {
        Self::start_internal(input, output, Some(initial), master)
    }

    /// Publish a new chain to the running engine; the audio thread swaps it in without dropouts.
    pub fn publish_chain(&self, chain: Box<ChainProcessor>) {
        self.handoff.publish(chain);
    }

    /// Drop any graph the audio thread has already swapped away from. This runs on the control/UI
    /// thread so VST teardown never happens in the realtime callback.
    pub fn reclaim_retired_chains(&self) {
        self.handoff.reclaim();
    }

    /// Update post-chain master settings without restarting the engine.
    pub fn set_master(&self, master: MasterSettings) {
        self.master.set(master);
    }

    fn start_internal(
        input: DeviceRef,
        output: DeviceRef,
        initial: Option<Box<ChainProcessor>>,
        master_settings: MasterSettings,
    ) -> Result<Self, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handoff: Arc<Handoff<ChainProcessor>> = Arc::new(Handoff::new());
        let handoff_thread = handoff.clone();
        let status = Arc::new(AtomicU8::new(EngineStatus::Running.as_u8()));
        let status_thread = status.clone();
        let master = Arc::new(MasterControl::new(master_settings));
        let master_thread = master.clone();
        let (meter_publisher, meter_reader) = meter_channel();
        let (tx, rx) = channel();
        let thread = std::thread::spawn(move || {
            run_audio_thread(
                input,
                output,
                initial,
                handoff_thread,
                meter_publisher,
                stop_thread,
                status_thread,
                master_thread,
                tx,
            )
        });

        match rx.recv() {
            Ok(Ok(latency)) => Ok(AudioEngine {
                stop,
                thread: Some(thread),
                handoff,
                meter: meter_reader,
                status,
                master,
                latency,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(AudioError::ThreadSetup)
            }
        }
    }

    /// Signal the audio thread to stop and join it (the final chain is dropped on the control thread).
    pub fn stop(&mut self) {
        let _ = self.stop_and_take();
    }

    /// Stop and join the audio thread, returning the final chain (still initialized) so the caller
    /// can capture its state before it is dropped.
    pub fn stop_and_take(&mut self) -> Option<Box<ChainProcessor>> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.handoff.take_retired()
    }

    /// The measured round-trip latency at startup.
    pub fn measured_latency(&self) -> MeasuredLatency {
        self.latency
    }

    /// The engine's current run status (read off the audio thread). Becomes `Faulted` if the realtime
    /// loop exited on its own (e.g. the device was invalidated), `StoppedByUser` after a `stop`.
    pub fn status(&self) -> EngineStatus {
        EngineStatus::from_u8(self.status.load(Ordering::Acquire))
    }

    /// Read the latest input/output meter snapshot (off the audio thread).
    pub fn meter_reader(&self) -> &MeterReader {
        &self.meter
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Open streams, fold the initial chain's latency into the report, then run the event-driven loop.
fn run_audio_thread(
    input: DeviceRef,
    output: DeviceRef,
    initial: Option<Box<ChainProcessor>>,
    handoff: Arc<Handoff<ChainProcessor>>,
    meter_publisher: MeterPublisher,
    stop: Arc<AtomicBool>,
    status: Arc<AtomicU8>,
    master: Arc<MasterControl>,
    setup: Sender<Result<MeasuredLatency, AudioError>>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    match setup_streams(&input, &output, meter_publisher, master) {
        Ok(mut state) => {
            let chain_latency = initial
                .as_ref()
                .map_or(0, |chain| chain.aggregate_latency());
            let chain_ms =
                chain_latency as f64 / state.capture.format().sample_rate.max(1) as f64 * 1000.0;
            state.latency.chain_latency_ms = chain_ms;
            state.latency.total_ms += chain_ms;
            let _ = setup.send(Ok(state.latency));

            let current = initial.map_or(ptr::null_mut(), Box::into_raw);
            run_loop(state, current, handoff, stop, status);
        }
        Err(error) => {
            let _ = setup.send(Err(error));
        }
    }
}

struct RunningState {
    capture: WasapiStream,
    render: WasapiStream,
    transport: Transport,
    midi_inputs: MidiInputs,
    midi_queue: Arc<MidiEventQueue>,
    midi_scratch: [MidiMessage; MAX_BLOCK_MIDI_EVENTS],
    capture_scratch: Vec<f32>,
    render_scratch: Vec<f32>,
    meter_publisher: MeterPublisher,
    master: Arc<MasterControl>,
    meter: MeterSnapshot,
    latency: MeasuredLatency,
}

fn setup_streams(
    input: &DeviceRef,
    output: &DeviceRef,
    meter_publisher: MeterPublisher,
    master: Arc<MasterControl>,
) -> Result<RunningState, AudioError> {
    let capture = WasapiStream::open_capture(input)?;
    let render = WasapiStream::open_render(output)?;
    let in_fmt = capture.format();
    let out_fmt = render.format();
    // The transport moves samples 1:1 between the streams (no resampler): mismatched device rates
    // would play pitch-shifted and chronically under-run the ring. Refuse to start instead.
    if in_fmt.sample_rate != out_fmt.sample_rate {
        return Err(AudioError::SampleRateMismatch {
            input_hz: in_fmt.sample_rate,
            output_hz: out_fmt.sample_rate,
        });
    }

    let max_frames = capture.buffer_frames().max(render.buffer_frames()) as usize;
    // Generous ring (several buffers of stereo) so a late callback does not under-run.
    let ring_capacity = max_frames * 2 * 4;
    let transport = Transport::new(in_fmt.channels, out_fmt.channels, max_frames, ring_capacity);

    let capture_scratch = vec![0.0f32; max_frames * in_fmt.channels.max(1) as usize];
    let render_scratch = vec![0.0f32; max_frames * out_fmt.channels.max(1) as usize];

    let latency = measure_latency(&capture, &render);
    let midi_queue = Arc::new(MidiEventQueue::new());
    let midi_inputs = MidiInputs::open_all(midi_queue.clone());
    crate::diagnostics::log(format!("midi: engine input count={}", midi_inputs.len()));

    unsafe {
        capture.client().Start()?;
        render.client().Start()?;
    }

    Ok(RunningState {
        capture,
        render,
        transport,
        midi_inputs,
        midi_queue,
        midi_scratch: [MidiMessage::default(); MAX_BLOCK_MIDI_EVENTS],
        capture_scratch,
        render_scratch,
        meter_publisher,
        master,
        meter: MeterSnapshot::default(),
        latency,
    })
}

fn run_loop(
    mut state: RunningState,
    mut current: *mut ChainProcessor,
    handoff: Arc<Handoff<ChainProcessor>>,
    stop: Arc<AtomicBool>,
    status: Arc<AtomicU8>,
) {
    let handles = [state.capture.event(), state.render.event()];
    let mut faulted = false;
    while !stop.load(Ordering::Acquire) {
        let wait = unsafe { WaitForMultipleObjects(&handles, false, 200) };
        let result = if wait == WAIT_OBJECT_0 {
            pump_capture(
                &state.capture,
                &mut state.transport,
                &mut state.capture_scratch,
                &mut state.meter,
            )
        } else if wait.0 == WAIT_OBJECT_0.0 + 1 {
            // Apply any published chain edit (lock-free; the old graph is reclaimed off this thread).
            if let Some(next) = handoff.try_take() {
                if !current.is_null() {
                    handoff.retire(current);
                }
                current = next;
            }
            let midi_count = state.midi_queue.drain(&mut state.midi_scratch);
            let result = pump_render(
                &state.render,
                &mut state.transport,
                &mut state.render_scratch,
                current,
                &state.midi_scratch[..midi_count],
                &mut state.meter,
                &state.master,
            );
            // Publish the latest input+output levels for the UI (off-thread reader).
            state.meter_publisher.publish(state.meter);
            result
        } else {
            Ok(()) // timeout / abandoned
        };
        if result.is_err() {
            // A runtime fault (commonly a device-invalidated `Com` error) — exit and report it so
            // the control thread can react (M7 recovery), distinct from a control-thread stop.
            faulted = true;
            break;
        }
    }
    // Publish why the loop exited (non-blocking; ADR-0001): a fault vs. a requested stop.
    let exit_status = if faulted {
        EngineStatus::Faulted
    } else {
        EngineStatus::StoppedByUser
    };
    status.store(exit_status.as_u8(), Ordering::Release);
    // Park the current chain for the control thread to reclaim (never freed on the audio thread).
    if !current.is_null() {
        handoff.retire(current);
    }
    unsafe {
        let _ = state.capture.client().Stop();
        let _ = state.render.client().Stop();
    }
}

/// Drain all ready capture packets into the transport, updating the input meter.
fn pump_capture(
    stream: &WasapiStream,
    transport: &mut Transport,
    scratch: &mut [f32],
    meter: &mut MeterSnapshot,
) -> Result<(), AudioError> {
    let format = stream.format();
    let client = stream.capture_client().expect("capture stream");
    unsafe {
        loop {
            if client.GetNextPacketSize()? == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;
            client.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let samples = frames as usize * format.channels.max(1) as usize;
            // A packet flagged silent carries undefined data (WASAPI contract): feed zeros of the
            // packet's length so the stream stays continuous without leaking buffer garbage.
            let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
            let n = if silent {
                scratch[..samples].fill(0.0);
                samples
            } else if data.is_null() {
                0
            } else {
                let bytes = samples * format.sample.bytes_per_sample();
                let byte_slice = slice::from_raw_parts(data, bytes);
                bytes_to_f32(format.sample, byte_slice, &mut scratch[..samples])
            };
            let levels = transport.capture(&scratch[..n]);
            meter.set_input_stereo(levels);
            client.ReleaseBuffer(frames)?;
        }
    }
    Ok(())
}

/// Fill the available render space, running the plugin chain (if any) and updating the output meter.
fn pump_render(
    stream: &WasapiStream,
    transport: &mut Transport,
    scratch: &mut [f32],
    chain: *mut ChainProcessor,
    midi: &[MidiMessage],
    meter: &mut MeterSnapshot,
    master: &MasterControl,
) -> Result<(), AudioError> {
    let format = stream.format();
    let render = stream.render_client().expect("render stream");
    let audio = stream.client();
    unsafe {
        let padding = audio.GetCurrentPadding()?;
        let available = stream.buffer_frames().saturating_sub(padding);
        if available == 0 {
            return Ok(());
        }
        let samples = available as usize * format.channels.max(1) as usize;
        transport.render_through(&mut scratch[..samples], |stereo| {
            if !chain.is_null() {
                (*chain).process_in_place_with_midi(stereo, midi)
            }
            apply_master(stereo, master.gain_linear());
            meter.set_output_stereo(stereo_levels(stereo));
        });

        let data = render.GetBuffer(available)?;
        let bytes = samples * format.sample.bytes_per_sample();
        let byte_slice = slice::from_raw_parts_mut(data, bytes);
        f32_to_bytes(format.sample, &scratch[..samples], byte_slice);
        render.ReleaseBuffer(available, 0)?;
    }
    Ok(())
}

fn apply_master(stereo: &mut [f32], gain: f32) {
    for sample in stereo {
        *sample *= gain;
    }
}

fn measure_latency(capture: &WasapiStream, render: &WasapiStream) -> MeasuredLatency {
    let cap_fmt = capture.format();
    let ren_fmt = render.format();
    let capture_buffer_ms =
        capture.buffer_frames() as f64 / cap_fmt.sample_rate.max(1) as f64 * 1000.0;
    let render_buffer_ms =
        render.buffer_frames() as f64 / ren_fmt.sample_rate.max(1) as f64 * 1000.0;
    let capture_stream_ms =
        unsafe { capture.client().GetStreamLatency().unwrap_or(0) } as f64 / HNS_PER_MS;
    let render_stream_ms =
        unsafe { render.client().GetStreamLatency().unwrap_or(0) } as f64 / HNS_PER_MS;
    MeasuredLatency {
        capture_buffer_ms,
        render_buffer_ms,
        capture_stream_ms,
        render_stream_ms,
        chain_latency_ms: 0.0, // folded in by run_audio_thread once the initial chain is known
        total_ms: capture_buffer_ms + render_buffer_ms + capture_stream_ms + render_stream_ms,
    }
}
