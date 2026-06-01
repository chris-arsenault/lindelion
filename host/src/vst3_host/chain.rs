//! `ChainProcessor` — drive an ordered serial chain of VST3 processors over a stereo block, with
//! per-slot bypass, under the allocation-free realtime discipline (ADR-0001).
//!
//! Input/output are **interleaved** stereo (matching the ring/Transport); internally the chain
//! deinterleaves to planar L/R, ping-pongs between two preallocated buffer pairs across stages, and
//! interleaves the result back. Bypass skips a stage (the signal stays in the "current" buffer).

use vst3::ComPtr;
use vst3::Steinberg::Vst::{IAudioProcessorTrait, IComponent};

use super::instance::{HostError, PluginInstance};
use super::processing::{ProcessDriver, drive_process};

/// One slot: a prepared plugin and its bypass flag.
pub struct ChainSlot {
    instance: PluginInstance,
    bypassed: bool,
}

/// An ordered serial stereo chain, allocation-free per block.
pub struct ChainProcessor {
    slots: Vec<ChainSlot>,
    a_left: Vec<f32>,
    a_right: Vec<f32>,
    b_left: Vec<f32>,
    b_right: Vec<f32>,
}

// Safe: a `ChainProcessor` is built on the control thread and handed to the audio thread (M3 Step 4);
// it is processed only on the audio thread, never shared, so the COM `ComPtr`s stay single-threaded.
unsafe impl Send for ChainProcessor {}

impl ChainProcessor {
    /// Build and prepare a chain from `instances` (aligned with `bypass`), sized for `max_frames`.
    pub fn new(
        instances: Vec<PluginInstance>,
        bypass: Vec<bool>,
        sample_rate: f64,
        max_frames: usize,
    ) -> Result<Self, HostError> {
        let driver = ProcessDriver::new(sample_rate, max_frames);
        let mut slots = Vec::with_capacity(instances.len());
        let mut bypass = bypass.into_iter();
        for instance in instances {
            driver.prepare(&instance)?;
            slots.push(ChainSlot {
                instance,
                bypassed: bypass.next().unwrap_or(false),
            });
        }
        Ok(Self {
            slots,
            a_left: vec![0.0; max_frames],
            a_right: vec![0.0; max_frames],
            b_left: vec![0.0; max_frames],
            b_right: vec![0.0; max_frames],
        })
    }

    /// Process an interleaved stereo block in place through the (non-bypassed) chain.
    pub fn process_in_place(&mut self, stereo: &mut [f32]) {
        let frames = (stereo.len() / 2).min(self.a_left.len());

        for (frame, pair) in stereo.chunks_exact(2).take(frames).enumerate() {
            self.a_left[frame] = pair[0];
            self.a_right[frame] = pair[1];
        }

        for idx in 0..self.slots.len() {
            if self.slots[idx].bypassed {
                continue;
            }
            unsafe {
                drive_process(
                    self.slots[idx].instance.processor(),
                    [&self.a_left[..frames], &self.a_right[..frames]],
                    [&mut self.b_left[..frames], &mut self.b_right[..frames]],
                );
            }
            std::mem::swap(&mut self.a_left, &mut self.b_left);
            std::mem::swap(&mut self.a_right, &mut self.b_right);
        }

        // Sanitize the chain output so a misbehaving plugin's NaN/Inf can never reach the device
        // (M7). A per-sample branch — allocation-free, on the audio path (ADR-0001).
        for (frame, pair) in stereo.chunks_exact_mut(2).take(frames).enumerate() {
            pair[0] = finite_or_zero(self.a_left[frame]);
            pair[1] = finite_or_zero(self.a_right[frame]);
        }
    }

    /// Number of slots in the chain.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the chain has no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The `IComponent` of slot `idx`, for state capture.
    pub fn component(&self, idx: usize) -> Option<&ComPtr<IComponent>> {
        self.slots.get(idx).map(|slot| slot.instance.component())
    }

    /// Whether slot `idx` is bypassed.
    pub fn is_bypassed(&self, idx: usize) -> bool {
        self.slots.get(idx).is_some_and(|slot| slot.bypassed)
    }

    /// Sum of `getLatencySamples` over the non-bypassed slots.
    pub fn aggregate_latency(&self) -> u32 {
        self.slots
            .iter()
            .filter(|slot| !slot.bypassed)
            .map(|slot| unsafe { slot.instance.processor().getLatencySamples() })
            .sum()
    }
}

/// A finite sample, or `0.0` for NaN/Inf — the audio-output guard against a misbehaving plugin.
#[inline]
fn finite_or_zero(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::{gain_fixture_factory, nan_fixture_factory};
    use vst3::Steinberg::Vst::IHostApplication;

    fn host() -> ComPtr<IHostApplication> {
        HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
    }

    fn instance(gain: f32, latency: u32) -> PluginInstance {
        PluginInstance::from_factory(&gain_fixture_factory(gain, latency), &host())
            .expect("instance")
    }

    fn nan_instance() -> PluginInstance {
        PluginInstance::from_factory(&nan_fixture_factory(), &host()).expect("instance")
    }

    #[test]
    fn two_gain_slots_scale_by_product() {
        let mut chain = ChainProcessor::new(
            vec![instance(0.5, 0), instance(0.5, 0)],
            vec![false, false],
            48_000.0,
            512,
        )
        .expect("chain");
        let mut stereo = vec![1.0f32, 1.0, 2.0, 2.0]; // 2 frames
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.25, 0.25, 0.5, 0.5]);
    }

    #[test]
    fn bypassed_slot_is_skipped() {
        let mut chain = ChainProcessor::new(
            vec![instance(0.5, 0), instance(0.5, 0)],
            vec![false, true],
            48_000.0,
            512,
        )
        .expect("chain");
        let mut stereo = vec![1.0f32, 1.0];
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.5, 0.5]);
    }

    #[test]
    fn both_bypassed_is_identity() {
        let mut chain = ChainProcessor::new(
            vec![instance(0.5, 0), instance(0.5, 0)],
            vec![true, true],
            48_000.0,
            512,
        )
        .expect("chain");
        let mut stereo = vec![0.3f32, 0.7];
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.3, 0.7]);
    }

    #[test]
    fn aggregate_latency_sums_active_slots() {
        let active = ChainProcessor::new(
            vec![instance(1.0, 3), instance(1.0, 4)],
            vec![false, false],
            48_000.0,
            512,
        )
        .expect("chain");
        assert_eq!(active.aggregate_latency(), 7);

        let bypassed = ChainProcessor::new(
            vec![instance(1.0, 3), instance(1.0, 4)],
            vec![true, true],
            48_000.0,
            512,
        )
        .expect("chain");
        assert_eq!(bypassed.aggregate_latency(), 0);
    }

    #[test]
    fn process_in_place_is_allocation_free() {
        let mut chain = ChainProcessor::new(
            vec![instance(0.5, 0), instance(0.5, 0)],
            vec![false, false],
            48_000.0,
            512,
        )
        .expect("chain");
        let mut stereo = vec![0.5f32; 256];
        lindelion_test_allocator::assert_no_allocations("chain process", || {
            chain.process_in_place(&mut stereo);
        });
    }

    #[test]
    fn nan_plugin_output_is_sanitized_to_finite() {
        let mut chain =
            ChainProcessor::new(vec![nan_instance()], vec![false], 48_000.0, 512).expect("chain");
        let mut stereo = vec![0.5f32; 256];
        // The guard runs on the audio path, so it must also be allocation-free (ADR-0001).
        lindelion_test_allocator::assert_no_allocations("chain nan guard", || {
            chain.process_in_place(&mut stereo);
        });
        // A NaN-emitting plugin must never leak non-finite samples to the device.
        assert!(stereo.iter().all(|s| s.is_finite()));
        // …and the guard maps them to silence.
        assert!(stereo.iter().all(|&s| s == 0.0));
    }
}
