//! Process driver — run the canonical offline host sequence against a [`PluginInstance`]:
//! `setBusArrangements` → `canProcessSampleSize` → `setupProcessing` → `activateBus`/`setActive`/
//! `setProcessing`, then build a `ProcessData` (mirroring the `channelBuffers32` layout in
//! `crates/lindelion-plugin-shell/src/vst3_process.rs`) and call `process()`.
//!
//! This is an **offline** spike driver: it allocates freely per block. The allocation-free realtime
//! discipline (ADR-0001) applies to the M3 audio callback, not here.

use std::ptr;

use vst3::{ComPtr, Steinberg::Vst::*, Steinberg::*};

use super::instance::{HostError, PluginInstance};

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

    /// Run the full prepare sequence (stereo bus arrangement, 32-bit setup, activate, start).
    pub fn prepare(&self, instance: &PluginInstance) -> Result<(), HostError> {
        let processor = instance.processor();
        let component = instance.component();
        unsafe {
            let mut stereo_in: SpeakerArrangement = SpeakerArr::kStereo;
            let mut stereo_out: SpeakerArrangement = SpeakerArr::kStereo;
            if processor.setBusArrangements(&mut stereo_in, 1, &mut stereo_out, 1) != kResultTrue {
                return Err(HostError::SetupFailed("setBusArrangements"));
            }
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
            component.activateBus(audio, BusDirections_::kInput as BusDirection, 0, 1);
            component.activateBus(audio, BusDirections_::kOutput as BusDirection, 0, 1);
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

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::fixture_factory;

    fn fixture_instance() -> PluginInstance {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        PluginInstance::from_factory(&factory, &host).expect("instance")
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
