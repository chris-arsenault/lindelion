//! `ChainProcessor` — drive an ordered serial chain of VST3 processors over a stereo block, with
//! per-slot bypass, under the allocation-free realtime discipline (ADR-0001).
//!
//! Input/output are **interleaved** stereo (matching the ring/Transport); internally the chain
//! deinterleaves to planar L/R, ping-pongs between two preallocated buffer pairs across stages, and
//! interleaves the result back. Bypass skips a stage (the signal stays in the "current" buffer).
//!
//! The chain does **not** own its plugins — it holds `Arc<PluginInstance>` shared with the
//! controller's persistent instance pool ([`PoolSlot`]). Reordering/bypassing/adding/removing
//! rebuilds only this ordering over the *same* live instances, so plugin state (parameters and
//! transient DSP state) is preserved across edits; an instance is torn down (its `Drop` runs
//! `setActive`/`terminate`) only when the last `Arc` — the pool slot and every chain that referenced
//! it — is gone, which always happens on the control thread (the hand-off reclaims off the audio
//! thread), never while the audio thread is processing it.

use std::sync::Arc;

use vst3::Steinberg::Vst::IAudioProcessorTrait;

use super::instance::PluginInstance;
use super::module::LoadedModule;
use super::processing::drive_process;

/// One live, prepared plugin the controller keeps alive for the life of its chain slot: the shared
/// instance and the loaded module that owns its DLL. Field order is teardown order: the instance must
/// release/terminate before the module unloads.
pub struct PoolSlot {
    pub instance: Arc<PluginInstance>,
    pub module: LoadedModule,
}

/// One slot: a shared (pooled) plugin and its bypass flag.
pub struct ChainSlot {
    instance: Arc<PluginInstance>,
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

// Safe: a `ChainProcessor` is built on the control thread and handed to the audio thread; it is
// processed only on the audio thread (single consumer) and never dropped there (the hand-off retires
// it for the control thread to reclaim). The shared `Arc<PluginInstance>`s are only *dereferenced*
// (to call `process`) on the audio thread — never cloned or dropped there — so no refcount races.
unsafe impl Send for ChainProcessor {}

impl ChainProcessor {
    /// Build a chain over already-prepared `instances` (aligned with `bypass`), sized for
    /// `max_frames`. The instances must already be set up + active (done once by the pool); building
    /// or rebuilding a chain never (re)prepares them, so their state survives reordering.
    pub fn new(instances: Vec<Arc<PluginInstance>>, bypass: Vec<bool>, max_frames: usize) -> Self {
        let mut bypass = bypass.into_iter();
        let slots = instances
            .into_iter()
            .map(|instance| ChainSlot {
                instance,
                bypassed: bypass.next().unwrap_or(false),
            })
            .collect();
        Self {
            slots,
            a_left: vec![0.0; max_frames],
            a_right: vec![0.0; max_frames],
            b_left: vec![0.0; max_frames],
            b_right: vec![0.0; max_frames],
        }
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
    use crate::vst3_host::fixture::{gain_fixture_factory, nan_fixture_factory};
    use crate::vst3_host::{HostContext, ProcessDriver};
    use vst3::ComPtr;
    use vst3::Steinberg::Vst::IHostApplication;

    fn host() -> ComPtr<IHostApplication> {
        HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
    }

    /// A prepared (set up + active) instance, shared like the controller's pool does.
    fn prepared(gain: f32, latency: u32) -> Arc<PluginInstance> {
        let instance = PluginInstance::from_factory(&gain_fixture_factory(gain, latency), &host())
            .expect("instance");
        ProcessDriver::new(48_000.0, 512)
            .prepare(&instance)
            .expect("prepare");
        Arc::new(instance)
    }

    fn nan_instance() -> Arc<PluginInstance> {
        let instance =
            PluginInstance::from_factory(&nan_fixture_factory(), &host()).expect("instance");
        ProcessDriver::new(48_000.0, 512)
            .prepare(&instance)
            .expect("prepare");
        Arc::new(instance)
    }

    #[test]
    fn two_gain_slots_scale_by_product() {
        let mut chain = ChainProcessor::new(
            vec![prepared(0.5, 0), prepared(0.5, 0)],
            vec![false, false],
            512,
        );
        let mut stereo = vec![1.0f32, 1.0, 2.0, 2.0]; // 2 frames
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.25, 0.25, 0.5, 0.5]);
    }

    #[test]
    fn bypassed_slot_is_skipped() {
        let mut chain = ChainProcessor::new(
            vec![prepared(0.5, 0), prepared(0.5, 0)],
            vec![false, true],
            512,
        );
        let mut stereo = vec![1.0f32, 1.0];
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.5, 0.5]);
    }

    #[test]
    fn both_bypassed_is_identity() {
        let mut chain = ChainProcessor::new(
            vec![prepared(0.5, 0), prepared(0.5, 0)],
            vec![true, true],
            512,
        );
        let mut stereo = vec![0.3f32, 0.7];
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.3, 0.7]);
    }

    #[test]
    fn reordering_reuses_instances_and_preserves_them() {
        // The pool owns the instances; chains hold shared clones. Rebuilding in a new order must not
        // recreate or drop them — the pool's `Arc`s keep them alive across the rebuild.
        let pool = [prepared(0.5, 0), prepared(0.25, 0)];
        let forward = ChainProcessor::new(
            vec![pool[0].clone(), pool[1].clone()],
            vec![false, false],
            512,
        );
        // Rebuild reversed from the same pooled instances; the forward chain is dropped here.
        drop(forward);
        let mut reversed = ChainProcessor::new(
            vec![pool[1].clone(), pool[0].clone()],
            vec![false, false],
            512,
        );
        // Each instance is still alive (pool + chain) and processes (0.25 * 0.5 = 0.125 either order).
        assert_eq!(Arc::strong_count(&pool[0]), 2);
        let mut stereo = vec![1.0f32, 1.0];
        reversed.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.125, 0.125]);
    }

    #[test]
    fn aggregate_latency_sums_active_slots() {
        let active = ChainProcessor::new(
            vec![prepared(1.0, 3), prepared(1.0, 4)],
            vec![false, false],
            512,
        );
        assert_eq!(active.aggregate_latency(), 7);

        let bypassed = ChainProcessor::new(
            vec![prepared(1.0, 3), prepared(1.0, 4)],
            vec![true, true],
            512,
        );
        assert_eq!(bypassed.aggregate_latency(), 0);
    }

    #[test]
    fn process_in_place_is_allocation_free() {
        let mut chain = ChainProcessor::new(
            vec![prepared(0.5, 0), prepared(0.5, 0)],
            vec![false, false],
            512,
        );
        let mut stereo = vec![0.5f32; 256];
        lindelion_test_allocator::assert_no_allocations("chain process", || {
            chain.process_in_place(&mut stereo);
        });
    }

    #[test]
    fn nan_plugin_output_is_sanitized_to_finite() {
        let mut chain = ChainProcessor::new(vec![nan_instance()], vec![false], 512);
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
