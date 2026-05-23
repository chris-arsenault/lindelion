use std::{cell::RefCell, ptr, rc::Rc};

use lindelion_midi::MidiClip;
use lindelion_pitch_detect::PitchContour;
use lindelion_plugin_shell::vst3::len_wstring;
use vst3::{Class, ComPtr, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use crate::{
    AUDITION_VOLUME_PARAMETER_ID, AnalysisJob, AnalysisJobResult, AnalysisSequence, AnalysisStatus,
    CaptureState, GlirdirPatch, GlirdirWorkerQueue, GlirdirWorkerResult, RequantizeJob,
    ScratchpadAudio, TIMING_STRENGTH_PARAMETER_ID,
    vst3_entry::{messages::GlirdirPluginMessage, processor::GlirdirVst3Processor},
};

#[test]
fn bus_count_exposes_audio_input_and_output_only() {
    let processor = GlirdirVst3Processor::new();

    assert_eq!(unsafe { processor.getBusCount(audio(), input()) }, 1);
    assert_eq!(unsafe { processor.getBusCount(audio(), output()) }, 1);
    assert_eq!(unsafe { processor.getBusCount(event(), input()) }, 0);
    assert_eq!(unsafe { processor.getBusCount(event(), output()) }, 0);
}

#[test]
fn bus_info_exposes_stereo_input_and_output() {
    let processor = GlirdirVst3Processor::new();
    let mut input_bus = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { processor.getBusInfo(audio(), input(), 0, &mut input_bus) },
        kResultOk
    );
    assert_eq!(input_bus.channelCount, 2);
    assert_eq!(wide_string(&input_bus.name), "Input");

    let mut output_bus = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { processor.getBusInfo(audio(), output(), 0, &mut output_bus) },
        kResultOk
    );
    assert_eq!(output_bus.channelCount, 2);
    assert_eq!(wide_string(&output_bus.name), "Output");
}

#[test]
fn bus_arrangements_accept_mono_or_stereo_input_and_stereo_output() {
    let processor = GlirdirVst3Processor::new();
    let mut mono_input = SpeakerArr::kMono;
    let mut stereo_input = SpeakerArr::kStereo;
    let mut stereo_output = SpeakerArr::kStereo;
    let mut mono_output = SpeakerArr::kMono;

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut mono_input, 1, &mut stereo_output, 1) },
        kResultTrue
    );
    let mut active_input = SpeakerArr::kStereo;
    assert_eq!(
        unsafe { processor.getBusArrangement(input(), 0, &mut active_input) },
        kResultOk
    );
    assert_eq!(active_input, SpeakerArr::kMono);

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut stereo_input, 1, &mut stereo_output, 1) },
        kResultTrue
    );
    assert_eq!(
        unsafe { processor.setBusArrangements(&mut stereo_input, 1, &mut mono_output, 1) },
        kResultFalse
    );
}

#[test]
fn sample_size_support_is_32_bit_float_only() {
    let processor = GlirdirVst3Processor::new();

    assert_eq!(
        unsafe { processor.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as i32) },
        kResultOk
    );
    assert_eq!(
        unsafe { processor.canProcessSampleSize(SymbolicSampleSizes_::kSample64 as i32) },
        kNotImplemented
    );
    assert_eq!(
        unsafe { processor.canProcessSampleSize(999) },
        kInvalidArgument
    );
}

#[test]
fn process_context_requirements_request_musical_transport() {
    let processor = GlirdirVst3Processor::new();
    let requirements = unsafe { processor.getProcessContextRequirements() };

    assert_flag(
        requirements,
        IProcessContextRequirements_::Flags_::kNeedBarPositionMusic,
    );
    assert_flag(
        requirements,
        IProcessContextRequirements_::Flags_::kNeedTempo,
    );
    assert_flag(
        requirements,
        IProcessContextRequirements_::Flags_::kNeedTimeSignature,
    );
    assert_flag(
        requirements,
        IProcessContextRequirements_::Flags_::kNeedTransportState,
    );
}

#[test]
fn process_clears_idle_output_to_finite_samples() {
    let processor = GlirdirVst3Processor::new();
    setup_processor(&processor, 48_000.0, 4);
    let mut left = [f32::NAN, 1.0, f32::INFINITY, -1.0];
    let mut right = [0.5, f32::NEG_INFINITY, 2.0, -2.0];
    let mut output_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
    let mut output_bus = audio_bus(2, output_channels.as_mut_ptr());
    let mut data = process_data(4, None, Some(&mut output_bus));

    assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);

    assert!(
        left.iter()
            .chain(right.iter())
            .all(|sample| sample.is_finite())
    );
    assert_eq!(left, [0.0; 4]);
    assert_eq!(right, [0.0; 4]);
}

#[test]
fn process_projects_input_buffer_into_capture_path() {
    let processor = GlirdirVst3Processor::new();
    setup_processor(&processor, 10.0, 80);
    processor.plugin.borrow_mut().arm_capture();

    let input = vec![0.25_f32; 80];
    let mut input_channels = [input.as_ptr() as *mut Sample32];
    let mut input_bus = audio_bus(1, input_channels.as_mut_ptr());
    let mut left = vec![0.0_f32; 80];
    let mut right = vec![0.0_f32; 80];
    let mut output_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
    let mut output_bus = audio_bus(2, output_channels.as_mut_ptr());
    let mut data = process_data(80, Some(&mut input_bus), Some(&mut output_bus));

    assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);

    let plugin = processor.plugin.borrow();
    assert_eq!(plugin.capture_state(), CaptureState::Captured);
    assert!(plugin.patch().scratchpad.is_none());
    assert_eq!(
        plugin.analysis_status(),
        AnalysisStatus::CapturedPendingAnalysis
    );
}

#[test]
fn parameter_changes_mutate_patch_through_process() {
    let processor = GlirdirVst3Processor::new();
    setup_processor(&processor, 48_000.0, 1);
    let changes = TestParameterChanges::one(AUDITION_VOLUME_PARAMETER_ID, 1.0);
    let mut left = [0.0_f32];
    let mut right = [0.0_f32];
    let mut output_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
    let mut output_bus = audio_bus(2, output_channels.as_mut_ptr());
    let mut data = process_data(1, None, Some(&mut output_bus));
    data.inputParameterChanges = changes.as_ptr();

    assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);

    assert_eq!(processor.plugin.borrow().patch().audition.volume, 1.0);
}

#[test]
fn quantize_parameter_process_defers_midi_rederive_to_worker_notify() {
    let worker = Rc::new(RecordingWorker::default());
    let processor = GlirdirVst3Processor::with_worker(Box::new(Rc::clone(&worker)));
    setup_processor(&processor, 48_000.0, 1);
    seed_ready_analysis(&processor);
    let changes = TestParameterChanges::one(TIMING_STRENGTH_PARAMETER_ID, 0.25);
    let mut left = [0.0_f32];
    let mut right = [0.0_f32];
    let mut output_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
    let mut output_bus = audio_bus(2, output_channels.as_mut_ptr());
    let mut data = process_data(1, None, Some(&mut output_bus));
    data.inputParameterChanges = changes.as_ptr();

    assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);
    assert!(worker.analysis_jobs.borrow().is_empty());
    assert!(worker.requantize_jobs.borrow().is_empty());

    let message = GlirdirPluginMessage::status_request()
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();
    assert_eq!(unsafe { processor.notify(message.as_ptr()) }, kResultOk);

    assert!(worker.analysis_jobs.borrow().is_empty());
    let jobs = worker.requantize_jobs.borrow();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].quantize_settings.timing_strength, 0.25);
    assert_eq!(
        processor.plugin.borrow().analysis_status(),
        AnalysisStatus::Analyzing
    );
}

struct TestParameterChanges {
    queues: Vec<ComPtr<IParamValueQueue>>,
}

impl TestParameterChanges {
    fn one(id: ParamID, value: ParamValue) -> ComPtr<IParameterChanges> {
        ComWrapper::new(Self {
            queues: vec![TestParamValueQueue::com_ptr(id, value)],
        })
        .to_com_ptr::<IParameterChanges>()
        .expect("TestParameterChanges must expose IParameterChanges")
    }
}

impl Class for TestParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for TestParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.queues.len() as i32
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        if index < 0 {
            return ptr::null_mut();
        }
        self.queues
            .get(index as usize)
            .map_or(ptr::null_mut(), ComPtr::as_ptr)
    }

    unsafe fn addParameterData(
        &self,
        _id: *const ParamID,
        _index: *mut i32,
    ) -> *mut IParamValueQueue {
        ptr::null_mut()
    }
}

struct TestParamValueQueue {
    id: ParamID,
    points: RefCell<Vec<(i32, ParamValue)>>,
}

impl TestParamValueQueue {
    fn com_ptr(id: ParamID, value: ParamValue) -> ComPtr<IParamValueQueue> {
        ComWrapper::new(Self {
            id,
            points: RefCell::new(vec![(0, value)]),
        })
        .to_com_ptr::<IParamValueQueue>()
        .expect("TestParamValueQueue must expose IParamValueQueue")
    }
}

impl Class for TestParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for TestParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id
    }

    unsafe fn getPointCount(&self) -> i32 {
        self.points.borrow().len() as i32
    }

    unsafe fn getPoint(
        &self,
        index: i32,
        sample_offset: *mut i32,
        value: *mut ParamValue,
    ) -> tresult {
        if sample_offset.is_null() || value.is_null() {
            return kResultFalse;
        }
        if index < 0 {
            return kResultFalse;
        }
        let points = self.points.borrow();
        let Some((point_offset, point_value)) = points.get(index as usize) else {
            return kResultFalse;
        };
        *sample_offset = *point_offset;
        *value = *point_value;
        kResultTrue
    }

    unsafe fn addPoint(&self, sample_offset: i32, value: ParamValue, index: *mut i32) -> tresult {
        let mut points = self.points.borrow_mut();
        points.push((sample_offset, value));
        if !index.is_null() {
            *index = points.len().saturating_sub(1) as i32;
        }
        kResultOk
    }
}

fn setup_processor(processor: &GlirdirVst3Processor, sample_rate: f64, block_size: i32) {
    let mut setup = ProcessSetup {
        processMode: ProcessModes_::kRealtime as i32,
        symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
        maxSamplesPerBlock: block_size,
        sampleRate: sample_rate,
    };
    assert_eq!(unsafe { processor.setupProcessing(&mut setup) }, kResultOk);
}

fn seed_ready_analysis(processor: &GlirdirVst3Processor) {
    let patch = GlirdirPatch {
        scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.2; 4_800])),
        ..GlirdirPatch::default()
    };
    let mut plugin = processor.plugin.borrow_mut();
    plugin.set_patch(patch);
    let job = plugin.request_analysis_job().expect("analysis job");
    assert!(plugin.publish_analysis_result(AnalysisJobResult::ready(
        job.sequence,
        empty_analysis_result()
    )));
}

fn empty_analysis_result() -> crate::AnalysisResult {
    crate::AnalysisResult {
        pitch_contour: PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: Vec::new(),
        },
        markers: Vec::new(),
        detected_notes: Vec::new(),
        midi_clip: MidiClip::empty(120),
    }
}

#[derive(Default)]
struct RecordingWorker {
    analysis_jobs: RefCell<Vec<AnalysisJob>>,
    requantize_jobs: RefCell<Vec<RequantizeJob>>,
    midi_exports: RefCell<Vec<AnalysisSequence>>,
    results: RefCell<Vec<GlirdirWorkerResult>>,
}

impl GlirdirWorkerQueue for Rc<RecordingWorker> {
    fn schedule_analysis(&self, job: AnalysisJob) -> bool {
        self.analysis_jobs.borrow_mut().push(job);
        true
    }

    fn schedule_requantize(&self, job: RequantizeJob) -> bool {
        self.requantize_jobs.borrow_mut().push(job);
        true
    }

    fn schedule_midi_export(&self, sequence: AnalysisSequence, _clip: MidiClip) -> bool {
        self.midi_exports.borrow_mut().push(sequence);
        true
    }

    fn drain_results(&self, publish: &mut dyn FnMut(GlirdirWorkerResult)) -> usize {
        let mut count = 0;
        while let Some(result) = self.results.borrow_mut().pop() {
            publish(result);
            count += 1;
        }
        count
    }
}

fn process_data(
    sample_count: i32,
    input_bus: Option<&mut AudioBusBuffers>,
    output_bus: Option<&mut AudioBusBuffers>,
) -> ProcessData {
    let (num_inputs, inputs) = input_bus
        .map(|bus| (1, bus as *mut AudioBusBuffers))
        .unwrap_or((0, ptr::null_mut()));
    let (num_outputs, outputs) = output_bus
        .map(|bus| (1, bus as *mut AudioBusBuffers))
        .unwrap_or((0, ptr::null_mut()));
    ProcessData {
        processMode: ProcessModes_::kRealtime as i32,
        symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
        numSamples: sample_count,
        numInputs: num_inputs,
        numOutputs: num_outputs,
        inputs,
        outputs,
        inputParameterChanges: ptr::null_mut(),
        outputParameterChanges: ptr::null_mut(),
        inputEvents: ptr::null_mut(),
        outputEvents: ptr::null_mut(),
        processContext: ptr::null_mut(),
    }
}

fn audio_bus(num_channels: i32, channel_buffers: *mut *mut Sample32) -> AudioBusBuffers {
    AudioBusBuffers {
        numChannels: num_channels,
        silenceFlags: 0,
        __field0: AudioBusBuffers__type0 {
            channelBuffers32: channel_buffers,
        },
    }
}

fn audio() -> MediaType {
    MediaTypes_::kAudio as MediaType
}

fn event() -> MediaType {
    MediaTypes_::kEvent as MediaType
}

fn input() -> BusDirection {
    BusDirections_::kInput as BusDirection
}

fn output() -> BusDirection {
    BusDirections_::kOutput as BusDirection
}

fn assert_flag(value: u32, flag: u32) {
    assert_ne!(value & flag, 0);
}

fn wide_string(buffer: &[TChar]) -> String {
    let len = unsafe { len_wstring(buffer.as_ptr()) };
    String::from_utf16(&buffer[..len]).unwrap()
}
