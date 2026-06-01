//! Headless spike — load a real `.vst3` module, drive a known signal through it, and report.
//! Composes the module loader (M1 Step 5), factory driver (Step 3), and process driver (Step 4).
//! Run on Windows against `Cenedril.vst3` (a passthrough we control) and a third-party `.vst3`; on
//! Linux it cross-compiles, and the equivalent behaviour is proven in-process by the Step 4 tests.

use std::f32::consts::PI;
use std::ffi::CStr;
use std::path::Path;

use vst3::{ComPtr, Steinberg::Vst::*, Steinberg::*};

use super::host_context::HostContext;
use super::instance::{HostError, PluginInstance};
use super::module::load_module;
use super::processing::ProcessDriver;

/// Result of a spike run against one module.
pub struct SpikeReport {
    /// Name of the loaded `"Audio Module Class"`.
    pub class_name: String,
    /// Latency the plugin reports, in samples.
    pub latency_samples: u32,
    /// Whether an all-zero block stayed all-zero through the plugin.
    pub silence_ok: bool,
    /// Whether a sine passed through unchanged (true for a passthrough such as Cenedril).
    pub sine_ok: bool,
}

/// Load `path`, instantiate its audio-processor class, and run silence + sine blocks through it.
pub fn run_spike(path: &Path) -> Result<SpikeReport, HostError> {
    let module = load_module(path)?;
    let host = HostContext::new()
        .to_com_ptr::<IHostApplication>()
        .expect("host exposes IHostApplication");
    let instance = PluginInstance::from_factory(module.factory(), &host)?;

    let driver = ProcessDriver::new(48_000.0, 512);
    driver.prepare(&instance)?;

    let frames = 480;
    let silence = vec![vec![0.0f32; frames], vec![0.0f32; frames]];
    let silence_out = driver.process_block(&instance, &silence);
    let silence_ok = silence_out.iter().all(|ch| ch.iter().all(|&s| s == 0.0));

    let sine: Vec<f32> = (0..frames)
        .map(|i| (2.0 * PI * 1000.0 * i as f32 / 48_000.0).sin())
        .collect();
    let sine_out = driver.process_block(&instance, &[sine.clone(), sine.clone()]);
    let sine_ok = sine_out[0] == sine && sine_out[1] == sine;

    let latency_samples = unsafe { instance.processor().getLatencySamples() };
    let class_name = audio_class_name(module.factory()).unwrap_or_default();

    Ok(SpikeReport {
        class_name,
        latency_samples,
        silence_ok,
        sine_ok,
    })
}

/// Read the name of the factory's `"Audio Module Class"`, if any.
fn audio_class_name(factory: &ComPtr<IPluginFactory>) -> Option<String> {
    unsafe {
        let count = factory.countClasses();
        for index in 0..count {
            let mut info = std::mem::zeroed::<PClassInfo>();
            if factory.getClassInfo(index, &mut info) != kResultOk {
                continue;
            }
            if CStr::from_ptr(info.category.as_ptr()).to_bytes() == b"Audio Module Class" {
                return Some(
                    CStr::from_ptr(info.name.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    None
}
