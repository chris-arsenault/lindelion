//! Load-time plugin validation (M7). Probe a plugin end-to-end — instantiate the audio class,
//! prepare it, and process one silent stereo block — so an incompatible or mid-process-failing
//! plugin is rejected *before* it is added to a chain, instead of failing later on the audio thread.
//! Drives the same host path the live chain uses (M1 instantiate/setup, M3 `drive_process`).

use std::path::Path;

use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;
use vst3::Steinberg::{IPluginFactory, kResultOk, kResultTrue};

use super::host_context::HostContext;
use super::instance::{HostError, PluginInstance};
use super::module::load_module;
use super::processing::{ProcessDriver, drive_process};

/// Probe block geometry — one short silent block is enough to exercise instantiate→setup→process.
const VALIDATE_SAMPLE_RATE: f64 = 48_000.0;
const VALIDATE_BLOCK: usize = 512;

/// Probe the `.vst3` at `path`: load it, then validate its factory. Returns the first error and never
/// panics on a misbehaving plugin (the failure modes the host can contain — incompatible class or an
/// error returned from `process`; foreign-code aborts/hangs are out of in-process reach, see Step 5).
pub fn validate_plugin(path: &Path) -> Result<(), HostError> {
    let module = load_module(path)?;
    let host = HostContext::new()
        .to_com_ptr::<IHostApplication>()
        .expect("host exposes IHostApplication");
    validate_factory(module.factory(), &host)
}

/// Probe a factory: instantiate the audio class, prepare it, and process one silent stereo block,
/// surfacing the first failure as a `HostError`.
fn validate_factory(
    factory: &ComPtr<IPluginFactory>,
    host: &ComPtr<IHostApplication>,
) -> Result<(), HostError> {
    let instance = PluginInstance::from_factory(factory, host)?;
    let driver = ProcessDriver::new(VALIDATE_SAMPLE_RATE, VALIDATE_BLOCK);
    driver.prepare(&instance)?;

    let silent = vec![0.0f32; VALIDATE_BLOCK];
    let mut out_left = vec![0.0f32; VALIDATE_BLOCK];
    let mut out_right = vec![0.0f32; VALIDATE_BLOCK];
    let result = unsafe {
        drive_process(
            instance.processor(),
            [&silent, &silent],
            [&mut out_left, &mut out_right],
        )
    };
    if result != kResultOk && result != kResultTrue {
        return Err(HostError::ProcessFailed(result));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::fixture::{
        gain_fixture_factory, no_audio_class_factory, process_error_factory,
    };

    fn host() -> ComPtr<IHostApplication> {
        HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
    }

    #[test]
    fn well_behaved_plugin_validates() {
        assert!(validate_factory(&gain_fixture_factory(1.0, 0), &host()).is_ok());
    }

    #[test]
    fn incompatible_plugin_is_rejected() {
        assert!(matches!(
            validate_factory(&no_audio_class_factory(), &host()),
            Err(HostError::NoAudioClass)
        ));
    }

    #[test]
    fn process_error_plugin_is_rejected() {
        assert!(matches!(
            validate_factory(&process_error_factory(), &host()),
            Err(HostError::ProcessFailed(_))
        ));
    }
}
