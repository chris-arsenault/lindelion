//! Process driver — run the canonical offline host sequence against a [`PluginInstance`]:
//! `setBusArrangements` → `canProcessSampleSize` → `setupProcessing` → `activateBus`/`setActive`/
//! `setProcessing`, then build a `ProcessData` (mirroring the `channelBuffers32` layout in
//! `crates/lindelion-plugin-shell/src/vst3_process.rs`) and call `process()`.
//!
//! This is an **offline** spike driver: it allocates freely per block. The allocation-free realtime
//! discipline (ADR-0001) applies to the M3 audio callback, not here.

use std::{mem, ptr};

use vst3::{ComPtr, Steinberg::Vst::*, Steinberg::*};

use crate::midi::MidiMessage;

use super::events::HostEventList;
use super::instance::{HostError, PluginInstance};
use super::parameters::{HostParameterChanges, ParameterEdit};

const MAIN_BUS_INDEX: i32 = 0;
const ALL_CHANNELS_SILENT: u64 = u64::MAX;

/// Drive one `process()` call over caller-owned stereo planar buffers. **Allocation-free**: the
/// channel-pointer arrays and the `AudioBusBuffers`/`ProcessData` live on the stack, so the pointers
/// handed to the plugin stay valid for the call. Mirrors the `channelBuffers32` layout read in
/// `crates/lindelion-plugin-shell/src/vst3_process.rs`.
///
/// # Safety
/// `processor` must be a prepared `IAudioProcessor` (see [`ProcessDriver::prepare`]); `process()` is
/// driven for `min(input, output)` frames.
pub(super) unsafe fn drive_process(
    processor: &ComPtr<IAudioProcessor>,
    input: [&[f32]; 2],
    output: [&mut [f32]; 2],
) -> tresult {
    let frames = output[0].len().min(input[0].len());
    let [out_left, out_right] = output;

    let mut in_ptrs: [*mut Sample32; 2] = [
        input[0].as_ptr() as *mut Sample32,
        input[1].as_ptr() as *mut Sample32,
    ];
    let mut out_ptrs: [*mut Sample32; 2] = [out_left.as_mut_ptr(), out_right.as_mut_ptr()];

    let mut input_bus = AudioBusBuffers {
        numChannels: 2,
        silenceFlags: 0,
        __field0: AudioBusBuffers__type0 {
            channelBuffers32: in_ptrs.as_mut_ptr(),
        },
    };
    let mut output_bus = AudioBusBuffers {
        numChannels: 2,
        silenceFlags: 0,
        __field0: AudioBusBuffers__type0 {
            channelBuffers32: out_ptrs.as_mut_ptr(),
        },
    };

    let mut data = ProcessData {
        processMode: ProcessModes_::kRealtime as i32,
        symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
        numSamples: frames as i32,
        numInputs: 1,
        numOutputs: 1,
        inputs: &mut input_bus,
        outputs: &mut output_bus,
        inputParameterChanges: ptr::null_mut(),
        outputParameterChanges: ptr::null_mut(),
        inputEvents: ptr::null_mut(),
        outputEvents: ptr::null_mut(),
        processContext: ptr::null_mut(),
    };

    processor.process(&mut data)
}

/// Preallocated process-buffer layout for one prepared plugin instance.
///
/// VST3 requires `ProcessData.inputs`/`outputs` to contain an `AudioBusBuffers` entry for every
/// audio bus the component declares, including inactive sidechain/aux busses. Galad maps its stereo
/// chain signal to main bus 0 and gives all other channels valid silent/sink buffers so commercial
/// plugins that inspect the complete bus topology see a host-shaped process call. Instruments may
/// declare no audio input buses at all; Galad still drives them with `numInputs = 0` and uses their
/// main output as the next chain signal.
pub(super) struct ProcessBusScratch {
    label: String,
    input_buses: Vec<AudioBusBuffers>,
    output_buses: Vec<AudioBusBuffers>,
    input_ptrs: Vec<Vec<*mut Sample32>>,
    output_ptrs: Vec<Vec<*mut Sample32>>,
    silence: Vec<f32>,
    sink: Vec<f32>,
    context: ProcessContext,
    input_events: vst3::ComWrapper<HostEventList>,
    input_events_ptr: ComPtr<IEventList>,
    input_parameter_changes: vst3::ComWrapper<HostParameterChanges>,
    input_parameter_changes_ptr: ComPtr<IParameterChanges>,
    sample_rate: f64,
    project_time_samples: i64,
    debug: ProcessDebugState,
}

impl ProcessBusScratch {
    pub(super) fn from_component(
        label: impl Into<String>,
        component: &ComPtr<IComponent>,
        processor: &ComPtr<IAudioProcessor>,
        max_frames: usize,
        sample_rate: f64,
    ) -> Result<Self, HostError> {
        let input_buses = unsafe {
            bus_buffers_for(component, processor, BusDirections_::kInput as BusDirection)?
        };
        let output_buses = unsafe {
            bus_buffers_for(
                component,
                processor,
                BusDirections_::kOutput as BusDirection,
            )?
        };
        Ok(Self::new(
            label.into(),
            input_buses,
            output_buses,
            max_frames,
            sample_rate,
        ))
    }

    fn new(
        label: String,
        input_buses: Vec<AudioBusBuffers>,
        output_buses: Vec<AudioBusBuffers>,
        max_frames: usize,
        sample_rate: f64,
    ) -> Self {
        let input_ptrs = input_buses
            .iter()
            .map(|bus| vec![ptr::null_mut(); bus.numChannels.max(0) as usize])
            .collect();
        let output_ptrs = output_buses
            .iter()
            .map(|bus| vec![ptr::null_mut(); bus.numChannels.max(0) as usize])
            .collect();
        let input_events = HostEventList::new();
        let input_events_ptr = input_events
            .to_com_ptr::<IEventList>()
            .expect("HostEventList exposes IEventList");
        let input_parameter_changes = HostParameterChanges::new();
        let input_parameter_changes_ptr = input_parameter_changes
            .to_com_ptr::<IParameterChanges>()
            .expect("HostParameterChanges exposes IParameterChanges");
        crate::diagnostics::log(format!(
            "vst3-process: scratch label={label} max_frames={} sample_rate={} inputs={} outputs={} input_buses={} output_buses={}",
            max_frames,
            sample_rate,
            input_buses.len(),
            output_buses.len(),
            bus_layout_summary(&input_buses),
            bus_layout_summary(&output_buses)
        ));
        Self {
            label,
            input_buses,
            output_buses,
            input_ptrs,
            output_ptrs,
            silence: vec![0.0; max_frames.max(1)],
            sink: vec![0.0; max_frames.max(1)],
            context: unsafe { mem::zeroed() },
            input_events,
            input_events_ptr,
            input_parameter_changes,
            input_parameter_changes_ptr,
            sample_rate: sample_rate.max(1.0),
            project_time_samples: 0,
            debug: ProcessDebugState::default(),
        }
    }

    pub(super) unsafe fn drive_stereo(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
    ) -> tresult {
        self.drive_stereo_with_midi(processor, input, output, &[])
    }

    pub(super) unsafe fn drive_stereo_with_midi(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
        midi: &[MidiMessage],
    ) -> tresult {
        self.drive_stereo_with_events(processor, input, output, midi, &[])
    }

    pub(super) unsafe fn drive_stereo_with_events(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
        midi: &[MidiMessage],
        parameters: &[ParameterEdit],
    ) -> tresult {
        let frames = output[0]
            .len()
            .min(output[1].len())
            .min(input[0].len())
            .min(input[1].len())
            .min(self.silence.len())
            .min(self.sink.len());
        self.map_default_buffers(frames);

        let [out_left, out_right] = output;
        self.map_main_input_channel(0, input[0].as_ptr() as *mut Sample32);
        self.map_main_input_channel(1, input[1].as_ptr() as *mut Sample32);
        self.map_main_output_channel(0, out_left.as_mut_ptr());
        self.map_main_output_channel(1, out_right.as_mut_ptr());
        self.update_process_context(frames);
        let midi_event_count = self
            .input_events
            .set_midi_messages(midi, self.context.projectTimeMusic);
        let parameter_edit_count = self.input_parameter_changes.set_parameter_edits(parameters);

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: frames as i32,
            numInputs: self.input_buses.len() as i32,
            numOutputs: self.output_buses.len() as i32,
            inputs: if self.input_buses.is_empty() {
                ptr::null_mut()
            } else {
                self.input_buses.as_mut_ptr()
            },
            outputs: if self.output_buses.is_empty() {
                ptr::null_mut()
            } else {
                self.output_buses.as_mut_ptr()
            },
            inputParameterChanges: self.input_parameter_changes_ptr.as_ptr(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: self.input_events_ptr.as_ptr(),
            outputEvents: ptr::null_mut(),
            processContext: &mut self.context,
        };

        let result = processor.process(&mut data);
        #[cfg(test)]
        let _ = (midi_event_count, parameter_edit_count);
        #[cfg(not(test))]
        self.maybe_log_process_result(
            result,
            input,
            [&*out_left, &*out_right],
            frames,
            midi_event_count,
            parameter_edit_count,
        );
        self.project_time_samples = self.project_time_samples.saturating_add(frames as i64);
        result
    }

    fn map_default_buffers(&mut self, frames: usize) {
        self.silence[..frames].fill(0.0);
        let silent_ptr = self.silence.as_mut_ptr();
        let sink_ptr = self.sink.as_mut_ptr();

        for (bus, ptrs) in self.input_buses.iter_mut().zip(&mut self.input_ptrs) {
            for ptr in ptrs.iter_mut() {
                *ptr = silent_ptr;
            }
            bus.silenceFlags = silence_flags_for_channels(bus.numChannels);
            bus.__field0.channelBuffers32 = if ptrs.is_empty() {
                ptr::null_mut()
            } else {
                ptrs.as_mut_ptr()
            };
        }

        for (bus, ptrs) in self.output_buses.iter_mut().zip(&mut self.output_ptrs) {
            for ptr in ptrs.iter_mut() {
                *ptr = sink_ptr;
            }
            bus.silenceFlags = silence_flags_for_channels(bus.numChannels);
            bus.__field0.channelBuffers32 = if ptrs.is_empty() {
                ptr::null_mut()
            } else {
                ptrs.as_mut_ptr()
            };
        }
    }

    fn map_main_input_channel(&mut self, channel: usize, ptr: *mut Sample32) {
        if let Some(bus) = self.input_buses.get_mut(MAIN_BUS_INDEX as usize) {
            bus.silenceFlags = 0;
        }
        if let Some(ptrs) = self.input_ptrs.get_mut(MAIN_BUS_INDEX as usize)
            && let Some(slot) = ptrs.get_mut(channel)
        {
            *slot = ptr;
        }
    }

    fn map_main_output_channel(&mut self, channel: usize, ptr: *mut Sample32) {
        if let Some(bus) = self.output_buses.get_mut(MAIN_BUS_INDEX as usize) {
            bus.silenceFlags = 0;
        }
        if let Some(ptrs) = self.output_ptrs.get_mut(MAIN_BUS_INDEX as usize)
            && let Some(slot) = ptrs.get_mut(channel)
        {
            *slot = ptr;
        }
    }

    fn update_process_context(&mut self, _frames: usize) {
        const TEMPO: f64 = 120.0;
        self.context.state = ProcessContext_::StatesAndFlags_::kPlaying
            | ProcessContext_::StatesAndFlags_::kContTimeValid
            | ProcessContext_::StatesAndFlags_::kProjectTimeMusicValid
            | ProcessContext_::StatesAndFlags_::kTempoValid
            | ProcessContext_::StatesAndFlags_::kTimeSigValid;
        self.context.sampleRate = self.sample_rate;
        self.context.projectTimeSamples = self.project_time_samples;
        self.context.continousTimeSamples = self.project_time_samples;
        self.context.tempo = TEMPO;
        self.context.projectTimeMusic =
            self.project_time_samples as f64 / self.sample_rate * TEMPO / 60.0;
        self.context.timeSigNumerator = 4;
        self.context.timeSigDenominator = 4;
    }
}

#[derive(Default)]
struct ProcessDebugState {
    total_blocks: u64,
    first_blocks_logged: u32,
    nonsilent_blocks_logged: u32,
    failure_blocks_logged: u32,
}

#[cfg(not(test))]
impl ProcessBusScratch {
    #[allow(clippy::too_many_arguments)] // diagnostic logger: each slice is a distinct bus tap
    fn maybe_log_process_result(
        &mut self,
        result: tresult,
        input: [&[f32]; 2],
        output: [&[f32]; 2],
        frames: usize,
        midi_event_count: usize,
        parameter_edit_count: usize,
    ) {
        let failed = !vst_ok(result);
        let first_block = self.debug.first_blocks_logged < 2;
        let should_measure = first_block || self.debug.nonsilent_blocks_logged < 16 || failed;
        if !should_measure {
            self.debug.total_blocks = self.debug.total_blocks.saturating_add(1);
            return;
        }

        let input_stats = stereo_stats(input[0], input[1], frames);
        let output_stats = stereo_stats(output[0], output[1], frames);
        let nonsilent = input_stats.peak > 0.000_001;
        let should_log = first_block
            || (nonsilent && self.debug.nonsilent_blocks_logged < 16)
            || (failed && self.debug.failure_blocks_logged < 16);

        if !should_log {
            self.debug.total_blocks = self.debug.total_blocks.saturating_add(1);
            return;
        }

        if first_block {
            self.debug.first_blocks_logged += 1;
        }
        if nonsilent {
            self.debug.nonsilent_blocks_logged += 1;
        }
        if failed {
            self.debug.failure_blocks_logged += 1;
        }

        crate::diagnostics::log(format!(
            "vst3-process: label={} block={} frames={} result={} in_peak={:.7} in_rms={:.7} out_peak={:.7} out_rms={:.7} midi_events={} parameter_edits={} ctx_state=0x{:x} project_samples={} sample_rate={} buses_in={} buses_out={}",
            self.label,
            self.debug.total_blocks,
            frames,
            result,
            input_stats.peak,
            input_stats.rms,
            output_stats.peak,
            output_stats.rms,
            midi_event_count,
            parameter_edit_count,
            self.context.state,
            self.context.projectTimeSamples,
            self.context.sampleRate,
            self.input_buses.len(),
            self.output_buses.len()
        ));
        self.debug.total_blocks = self.debug.total_blocks.saturating_add(1);
    }
}

#[cfg(not(test))]
#[derive(Clone, Copy)]
struct SignalStats {
    peak: f32,
    rms: f32,
}

#[cfg(not(test))]
fn stereo_stats(left: &[f32], right: &[f32], frames: usize) -> SignalStats {
    let mut peak = 0.0f32;
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for (&l, &r) in left.iter().zip(right).take(frames) {
        let l_abs = l.abs();
        let r_abs = r.abs();
        peak = peak.max(l_abs).max(r_abs);
        sum += (l as f64 * l as f64) + (r as f64 * r as f64);
        count += 2;
    }
    SignalStats {
        peak,
        rms: if count == 0 {
            0.0
        } else {
            (sum / count as f64).sqrt() as f32
        },
    }
}

/// Drives a prepared plugin over stereo `f32` blocks at a fixed sample rate / max block size.
pub struct ProcessDriver {
    sample_rate: f64,
    max_block_size: usize,
}

impl ProcessDriver {
    /// A driver for the given sample rate and maximum block size.
    pub fn new(sample_rate: f64, max_block_size: usize) -> Self {
        Self {
            sample_rate,
            max_block_size,
        }
    }

    /// Run the full prepare sequence (stereo main bus arrangement, 32-bit setup, activate, start).
    ///
    /// Safe to re-run on an already-prepared instance (it quiesces first), so a pooled instance can
    /// be re-prepared at a new sample rate without being destroyed — its state survives, since
    /// `setActive`/`setupProcessing` do not clear the component's parameters.
    pub fn prepare(&self, instance: &PluginInstance) -> Result<(), HostError> {
        let processor = instance.processor();
        let component = instance.component();
        let label = instance.debug_name();
        unsafe {
            crate::diagnostics::log(format!(
                "vst3-prepare: begin label={label} sample_rate={} max_block={}",
                self.sample_rate, self.max_block_size
            ));
            // Quiesce first so re-preparation is legal (`setupProcessing` requires an inactive
            // component); harmless on a fresh, already-inactive instance.
            let set_processing_off = processor.setProcessing(0);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setProcessing(0) result={set_processing_off}"
            ));
            let set_active_off = component.setActive(0);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setActive(0) result={set_active_off}"
            ));
            let io_mode = component.setIoMode(IoModes_::kSimple as IoMode);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setIoMode(kSimple) result={io_mode}"
            ));
            configure_stereo_main_buses(component, processor)?;
            let can_process =
                processor.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as i32);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} canProcessSampleSize(kSample32) result={can_process}"
            ));
            if can_process != kResultOk {
                return Err(HostError::SetupFailed("canProcessSampleSize"));
            }
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: self.max_block_size as i32,
                sampleRate: self.sample_rate,
            };
            let setup_result = processor.setupProcessing(&mut setup);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setupProcessing result={setup_result}"
            ));
            if setup_result != kResultOk {
                return Err(HostError::SetupFailed("setupProcessing"));
            }
            let audio = MediaTypes_::kAudio as MediaType;
            activate_main_bus_only(
                label,
                component,
                audio,
                BusDirections_::kInput as BusDirection,
            )?;
            activate_main_bus_only(
                label,
                component,
                audio,
                BusDirections_::kOutput as BusDirection,
            )?;
            activate_event_buses(label, component, BusDirections_::kInput as BusDirection)?;
            activate_event_buses(label, component, BusDirections_::kOutput as BusDirection)?;
            if let Some(requirements) = processor.cast::<IProcessContextRequirements>() {
                let flags = requirements.getProcessContextRequirements();
                crate::diagnostics::log(format!(
                    "vst3-prepare: label={label} processContextRequirements flags=0x{flags:x}"
                ));
            } else {
                crate::diagnostics::log(format!(
                    "vst3-prepare: label={label} processContextRequirements unsupported"
                ));
            }
            let set_active_on = component.setActive(1);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setActive(1) result={set_active_on}"
            ));
            let set_processing_on = processor.setProcessing(1);
            crate::diagnostics::log(format!(
                "vst3-prepare: label={label} setProcessing(1) result={set_processing_on}"
            ));
            crate::diagnostics::log(format!("vst3-prepare: done label={label}"));
        }
        Ok(())
    }

    /// Process one block of `input` (one `Vec<f32>` per channel) and return the output channels.
    ///
    /// This offline helper allocates the bus scratch for each call; the realtime chain owns the same
    /// shape per slot and reuses it.
    pub fn process_block(&self, instance: &PluginInstance, input: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let channels = input.len();
        let frames = input.first().map_or(0, Vec::len);
        let mut out_storage: Vec<Vec<f32>> = vec![vec![0.0f32; frames]; channels];

        if channels == 2 {
            let (left, right) = out_storage.split_at_mut(1);
            let mut buses = ProcessBusScratch::from_component(
                instance.debug_name(),
                instance.component(),
                instance.processor(),
                frames,
                self.sample_rate,
            )
            .expect("prepared instance has process buses");
            unsafe {
                buses.drive_stereo(
                    instance.processor(),
                    [input[0].as_slice(), input[1].as_slice()],
                    [left[0].as_mut_slice(), right[0].as_mut_slice()],
                );
            }
        }

        out_storage
    }
}

unsafe fn configure_stereo_main_buses(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
) -> Result<(), HostError> {
    let audio = MediaTypes_::kAudio as MediaType;
    let input_count = audio_bus_count(component, audio, BusDirections_::kInput as BusDirection);
    let output_count =
        required_audio_bus_count(component, audio, BusDirections_::kOutput as BusDirection)?;
    let mut inputs = arrangements_for_buses(
        component,
        processor,
        BusDirections_::kInput as BusDirection,
        input_count,
    );
    let mut outputs = arrangements_for_buses(
        component,
        processor,
        BusDirections_::kOutput as BusDirection,
        output_count,
    );
    crate::diagnostics::log(format!(
        "vst3-prepare: bus counts inputs={} outputs={} input_arrangements={} output_arrangements={}",
        input_count,
        output_count,
        arrangements_summary(&inputs),
        arrangements_summary(&outputs)
    ));

    if let Some(input) = inputs.get_mut(MAIN_BUS_INDEX as usize) {
        *input = SpeakerArr::kStereo;
    }
    outputs[MAIN_BUS_INDEX as usize] = SpeakerArr::kStereo;

    let input_ptr = if inputs.is_empty() {
        ptr::null_mut()
    } else {
        inputs.as_mut_ptr()
    };
    let result = processor.setBusArrangements(
        input_ptr,
        input_count as i32,
        outputs.as_mut_ptr(),
        output_count as i32,
    );
    crate::diagnostics::log(format!(
        "vst3-prepare: setBusArrangements result={result} requested_inputs={} requested_outputs={}",
        arrangements_summary(&inputs),
        arrangements_summary(&outputs)
    ));
    if vst_ok(result) {
        return Ok(());
    }

    // Some correct plugins reject the host's requested arrangement and expect the host to use the
    // arrangement they report back. Accept that path when the reported main I/O is already stereo,
    // because Galad's internal chain is currently stereo.
    let input_ok = input_count == 0
        || reported_main_bus_is_stereo(
            component,
            processor,
            BusDirections_::kInput as BusDirection,
        );
    if input_ok
        && reported_main_bus_is_stereo(
            component,
            processor,
            BusDirections_::kOutput as BusDirection,
        )
    {
        crate::diagnostics::log(
            "vst3-prepare: setBusArrangements rejected; accepting reported stereo main bus",
        );
        return Ok(());
    }

    Err(HostError::SetupFailed("setBusArrangements"))
}

unsafe fn required_audio_bus_count(
    component: &ComPtr<IComponent>,
    audio: MediaType,
    direction: BusDirection,
) -> Result<usize, HostError> {
    let count = audio_bus_count(component, audio, direction);
    if count == 0 {
        return Err(HostError::SetupFailed(
            if direction == BusDirections_::kInput as BusDirection {
                "main audio input bus"
            } else {
                "main audio output bus"
            },
        ));
    }
    Ok(count)
}

unsafe fn audio_bus_count(
    component: &ComPtr<IComponent>,
    audio: MediaType,
    direction: BusDirection,
) -> usize {
    component.getBusCount(audio, direction).max(0) as usize
}

unsafe fn arrangements_for_buses(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    direction: BusDirection,
    count: usize,
) -> Vec<SpeakerArrangement> {
    (0..count)
        .map(|index| {
            reported_bus_arrangement(component, processor, direction, index as i32)
                .unwrap_or(SpeakerArr::kEmpty)
        })
        .collect()
}

unsafe fn reported_main_bus_is_stereo(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    direction: BusDirection,
) -> bool {
    reported_bus_arrangement(component, processor, direction, MAIN_BUS_INDEX)
        .is_some_and(|arrangement| arrangement == SpeakerArr::kStereo)
}

unsafe fn reported_bus_arrangement(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    direction: BusDirection,
    index: i32,
) -> Option<SpeakerArrangement> {
    let mut arrangement = SpeakerArr::kEmpty;
    if processor.getBusArrangement(direction, index, &mut arrangement) == kResultOk
        && arrangement != SpeakerArr::kEmpty
    {
        return Some(arrangement);
    }

    let audio = MediaTypes_::kAudio as MediaType;
    let mut info: BusInfo = mem::zeroed();
    if component.getBusInfo(audio, direction, index, &mut info) != kResultOk {
        return None;
    }
    match info.channelCount {
        1 => Some(SpeakerArr::kMono),
        2 => Some(SpeakerArr::kStereo),
        _ => None,
    }
}

unsafe fn activate_event_buses(
    label: &str,
    component: &ComPtr<IComponent>,
    direction: BusDirection,
) -> Result<(), HostError> {
    let event = MediaTypes_::kEvent as MediaType;
    let count = component.getBusCount(event, direction).max(0) as usize;
    for index in 0..count {
        let result = component.activateBus(event, direction, index as i32, 1);
        crate::diagnostics::log(format!(
            "vst3-prepare: label={label} activateBus media=event dir={} index={index} active=1 result={result}",
            bus_direction_label(direction)
        ));
        if !vst_ok(result) {
            return Err(HostError::SetupFailed("activate event bus"));
        }
    }
    Ok(())
}

unsafe fn activate_main_bus_only(
    label: &str,
    component: &ComPtr<IComponent>,
    audio: MediaType,
    direction: BusDirection,
) -> Result<(), HostError> {
    let count = if direction == BusDirections_::kOutput as BusDirection {
        required_audio_bus_count(component, audio, direction)?
    } else {
        audio_bus_count(component, audio, direction)
    };
    for index in 0..count {
        let active = if index as i32 == MAIN_BUS_INDEX { 1 } else { 0 };
        let result = component.activateBus(audio, direction, index as i32, active);
        crate::diagnostics::log(format!(
            "vst3-prepare: label={label} activateBus dir={} index={index} active={active} result={result}",
            bus_direction_label(direction)
        ));
        if !vst_ok(result) {
            return Err(HostError::SetupFailed(
                if direction == BusDirections_::kInput as BusDirection {
                    "activate input bus"
                } else {
                    "activate output bus"
                },
            ));
        }
    }
    Ok(())
}

unsafe fn bus_buffers_for(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    direction: BusDirection,
) -> Result<Vec<AudioBusBuffers>, HostError> {
    let audio = MediaTypes_::kAudio as MediaType;
    let count = if direction == BusDirections_::kOutput as BusDirection {
        required_audio_bus_count(component, audio, direction)?
    } else {
        audio_bus_count(component, audio, direction)
    };
    let mut buses = Vec::with_capacity(count);
    for index in 0..count {
        let channels = bus_channel_count(component, processor, direction, index as i32);
        buses.push(AudioBusBuffers {
            numChannels: channels as i32,
            silenceFlags: silence_flags_for_channels(channels as i32),
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: ptr::null_mut(),
            },
        });
    }
    Ok(buses)
}

unsafe fn bus_channel_count(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    direction: BusDirection,
    index: i32,
) -> usize {
    let audio = MediaTypes_::kAudio as MediaType;
    let mut info: BusInfo = mem::zeroed();
    if component.getBusInfo(audio, direction, index, &mut info) == kResultOk
        && info.channelCount > 0
    {
        return info.channelCount as usize;
    }
    reported_bus_arrangement(component, processor, direction, index)
        .map_or(0, |arrangement| arrangement.count_ones() as usize)
}

fn silence_flags_for_channels(channels: i32) -> u64 {
    match channels {
        n if n <= 0 => 0,
        n if n >= 64 => ALL_CHANNELS_SILENT,
        n => (1u64 << n) - 1,
    }
}

fn bus_layout_summary(buses: &[AudioBusBuffers]) -> String {
    buses
        .iter()
        .enumerate()
        .map(|(index, bus)| format!("{index}:{}ch", bus.numChannels))
        .collect::<Vec<_>>()
        .join(",")
}

fn arrangements_summary(arrangements: &[SpeakerArrangement]) -> String {
    arrangements
        .iter()
        .enumerate()
        .map(|(index, arrangement)| format!("{index}:0x{arrangement:x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn bus_direction_label(direction: BusDirection) -> &'static str {
    if direction == BusDirections_::kInput as BusDirection {
        "input"
    } else if direction == BusDirections_::kOutput as BusDirection {
        "output"
    } else {
        "unknown"
    }
}

fn vst_ok(result: tresult) -> bool {
    result == kResultOk || result == kResultTrue
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::{
        fixed_stereo_rejects_arrangement_factory, fixture_factory,
        output_only_midi_note_fixture_factory, sidechain_fixture_factory,
    };

    fn fixture_instance() -> PluginInstance {
        let factory = fixture_factory();
        instance_from_factory(&factory)
    }

    fn instance_from_factory(factory: &ComPtr<IPluginFactory>) -> PluginInstance {
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        PluginInstance::from_factory(factory, &host).expect("instance")
    }

    #[test]
    fn silence_stays_silent() {
        let instance = fixture_instance();
        let driver = ProcessDriver::new(48_000.0, 512);
        driver.prepare(&instance).expect("prepare");

        let input = vec![vec![0.0f32; 480], vec![0.0f32; 480]];
        let output = driver.process_block(&instance, &input);

        assert!(output.iter().all(|ch| ch.iter().all(|&s| s == 0.0)));
    }

    #[test]
    fn prepare_passes_all_audio_buses_for_sidechain_plugins() {
        let factory = sidechain_fixture_factory();
        let instance = instance_from_factory(&factory);
        let driver = ProcessDriver::new(48_000.0, 512);

        driver
            .prepare(&instance)
            .expect("prepare sidechain fixture");
    }

    #[test]
    fn prepare_accepts_output_only_instruments() {
        let factory = output_only_midi_note_fixture_factory();
        let instance = instance_from_factory(&factory);
        let driver = ProcessDriver::new(48_000.0, 512);

        driver
            .prepare(&instance)
            .expect("prepare output-only fixture");
    }

    #[test]
    fn prepare_accepts_reported_fixed_stereo_when_set_arrangement_fails() {
        let factory = fixed_stereo_rejects_arrangement_factory();
        let instance = instance_from_factory(&factory);
        let driver = ProcessDriver::new(48_000.0, 512);

        driver
            .prepare(&instance)
            .expect("prepare fixed-stereo fixture");
    }

    #[test]
    fn drive_process_is_allocation_free() {
        let instance = fixture_instance();
        let driver = ProcessDriver::new(48_000.0, 512);
        driver.prepare(&instance).expect("prepare");

        let left_in = vec![0.5f32; 128];
        let right_in = vec![0.25f32; 128];
        let mut left_out = vec![0.0f32; 128];
        let mut right_out = vec![0.0f32; 128];

        lindelion_test_allocator::assert_no_allocations("drive_process", || unsafe {
            drive_process(
                instance.processor(),
                [&left_in, &right_in],
                [&mut left_out, &mut right_out],
            );
        });

        // The fixture passes through verbatim, so output matches input.
        assert_eq!(left_out, left_in);
        assert_eq!(right_out, right_in);
    }

    #[test]
    fn sine_passes_through_bit_exact_at_zero_latency() {
        let instance = fixture_instance();
        let driver = ProcessDriver::new(48_000.0, 512);
        driver.prepare(&instance).expect("prepare");

        let frames = 480;
        let sine: Vec<f32> = (0..frames)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / 48_000.0).sin())
            .collect();
        let input = vec![sine.clone(), sine.clone()];

        let output = driver.process_block(&instance, &input);

        assert_eq!(output[0], sine);
        assert_eq!(output[1], sine);
        assert_eq!(unsafe { instance.processor().getLatencySamples() }, 0);
    }
}
