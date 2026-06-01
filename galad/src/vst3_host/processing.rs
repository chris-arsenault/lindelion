//! Process driver — run the canonical offline host sequence against a [`PluginInstance`]:
//! `setBusArrangements` → `canProcessSampleSize` → `setupProcessing` → `activateBus`/`setActive`/
//! `setProcessing`, then build a `ProcessData` (mirroring the `channelBuffers32` layout in
//! `crates/lindelion-plugin-shell/src/vst3_process.rs`) and call `process()`.
//!
//! This is an **offline** spike driver: it allocates freely per block. The allocation-free realtime
//! discipline (ADR-0001) applies to the M3 audio callback, not here.

use std::{mem, ptr};

use vst3::{ComPtr, Steinberg::Vst::*, Steinberg::*};

use super::instance::{HostError, PluginInstance};

const MAIN_BUS_INDEX: i32 = 0;

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
        unsafe {
            // Quiesce first so re-preparation is legal (`setupProcessing` requires an inactive
            // component); harmless on a fresh, already-inactive instance.
            processor.setProcessing(0);
            component.setActive(0);
            configure_stereo_main_buses(component, processor)?;
            if processor.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as i32) != kResultOk {
                return Err(HostError::SetupFailed("canProcessSampleSize"));
            }
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: self.max_block_size as i32,
                sampleRate: self.sample_rate,
            };
            if processor.setupProcessing(&mut setup) != kResultOk {
                return Err(HostError::SetupFailed("setupProcessing"));
            }
            let audio = MediaTypes_::kAudio as MediaType;
            activate_main_bus_only(component, audio, BusDirections_::kInput as BusDirection)?;
            activate_main_bus_only(component, audio, BusDirections_::kOutput as BusDirection)?;
            component.setActive(1);
            processor.setProcessing(1);
        }
        Ok(())
    }

    /// Process one block of `input` (one `Vec<f32>` per channel) and return the output channels.
    ///
    /// All backing storage, the channel-pointer arrays, and the `AudioBusBuffers` live on the stack
    /// for the duration of the `process()` call, so the pointers handed to the plugin stay valid.
    pub fn process_block(&self, instance: &PluginInstance, input: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let channels = input.len();
        let frames = input.first().map_or(0, Vec::len);
        let mut out_storage: Vec<Vec<f32>> = vec![vec![0.0f32; frames]; channels];

        if channels == 2 {
            let (left, right) = out_storage.split_at_mut(1);
            unsafe {
                drive_process(
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
    let input_count = audio_bus_count(component, audio, BusDirections_::kInput as BusDirection)?;
    let output_count = audio_bus_count(component, audio, BusDirections_::kOutput as BusDirection)?;
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

    inputs[MAIN_BUS_INDEX as usize] = SpeakerArr::kStereo;
    outputs[MAIN_BUS_INDEX as usize] = SpeakerArr::kStereo;

    let result = processor.setBusArrangements(
        inputs.as_mut_ptr(),
        input_count as i32,
        outputs.as_mut_ptr(),
        output_count as i32,
    );
    if vst_ok(result) {
        return Ok(());
    }

    // Some correct plugins reject the host's requested arrangement and expect the host to use the
    // arrangement they report back. Accept that path when the reported main I/O is already stereo,
    // because Galad's internal chain is currently stereo.
    if reported_main_bus_is_stereo(component, processor, BusDirections_::kInput as BusDirection)
        && reported_main_bus_is_stereo(
            component,
            processor,
            BusDirections_::kOutput as BusDirection,
        )
    {
        return Ok(());
    }

    Err(HostError::SetupFailed("setBusArrangements"))
}

unsafe fn audio_bus_count(
    component: &ComPtr<IComponent>,
    audio: MediaType,
    direction: BusDirection,
) -> Result<usize, HostError> {
    let count = component.getBusCount(audio, direction);
    if count <= 0 {
        return Err(HostError::SetupFailed(
            if direction == BusDirections_::kInput as BusDirection {
                "main audio input bus"
            } else {
                "main audio output bus"
            },
        ));
    }
    Ok(count as usize)
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

unsafe fn activate_main_bus_only(
    component: &ComPtr<IComponent>,
    audio: MediaType,
    direction: BusDirection,
) -> Result<(), HostError> {
    let count = audio_bus_count(component, audio, direction)?;
    for index in 0..count {
        let active = if index as i32 == MAIN_BUS_INDEX { 1 } else { 0 };
        let result = component.activateBus(audio, direction, index as i32, active);
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

fn vst_ok(result: tresult) -> bool {
    result == kResultOk || result == kResultTrue
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::{
        fixed_stereo_rejects_arrangement_factory, fixture_factory, sidechain_fixture_factory,
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
