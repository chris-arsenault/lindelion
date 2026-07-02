//! `ChainProcessor` — drive an ordered serial chain of VST3 processors over a stereo block, with
//! per-slot bypass, under the allocation-free realtime discipline (ADR-0001).
//!
//! Input/output are **interleaved** stereo (matching the ring/Transport); internally the chain
//! deinterleaves to planar L/R, ping-pongs between two preallocated buffer pairs across stages, and
//! interleaves the result back. Bypass skips a stage (the signal stays in the "current" buffer).
//!
//! The chain does **not** own its plugins exclusively — it holds `Arc<PluginInstance>` and
//! `Arc<LoadedModule>` shared with the controller's persistent instance pool ([`PoolSlot`]).
//! Reordering/bypassing/adding/removing rebuilds only this ordering over the *same* live instances,
//! so plugin state (parameters and transient DSP state) is preserved across edits. The module/DLL is
//! retained by every live or retired chain that can still reference the instance, so removing a pool
//! slot cannot unload plugin code out from under the audio thread.

use std::sync::Arc;

use vst3::Steinberg::Vst::IAudioProcessorTrait;

use crate::midi::MidiMessage;

use super::editor_controller::EditorController;
use super::instance::PluginInstance;
use super::module::LoadedModule;
use super::parameters::{MAX_BLOCK_PARAMETER_EDITS, ParameterEdit};
use super::processing::{ProcessBusScratch, vst_ok};

/// One live, prepared plugin the controller keeps alive for the life of its chain slot: the shared
/// controller, instance, and loaded module that owns its DLL. Field order is teardown order: the
/// controller and instance must release/terminate before the module unloads.
pub struct PoolSlot {
    pub controller: Option<Arc<EditorController>>,
    pub instance: Arc<PluginInstance>,
    pub module: Arc<LoadedModule>,
}

/// One slot: a shared (pooled) plugin and its bypass flag.
pub struct ChainSlot {
    instance: Arc<PluginInstance>,
    /// Keeps the plugin DLL mapped for as long as this chain can call into `instance`.
    _module: Option<Arc<LoadedModule>>,
    bypassed: bool,
    process_buses: ProcessBusScratch,
    parameter_edits: [ParameterEdit; MAX_BLOCK_PARAMETER_EDITS],
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
    pub fn new(
        instances: Vec<Arc<PluginInstance>>,
        bypass: Vec<bool>,
        max_frames: usize,
        sample_rate: f64,
    ) -> Self {
        Self::from_entries(
            instances.into_iter().map(|instance| (instance, None)),
            bypass,
            max_frames,
            sample_rate,
        )
    }

    /// Build a chain over pooled instances and the modules that own their DLLs.
    pub fn new_with_modules(
        instances: Vec<(Arc<PluginInstance>, Arc<LoadedModule>)>,
        bypass: Vec<bool>,
        max_frames: usize,
        sample_rate: f64,
    ) -> Self {
        Self::from_entries(
            instances
                .into_iter()
                .map(|(instance, module)| (instance, Some(module))),
            bypass,
            max_frames,
            sample_rate,
        )
    }

    fn from_entries(
        instances: impl IntoIterator<Item = (Arc<PluginInstance>, Option<Arc<LoadedModule>>)>,
        bypass: Vec<bool>,
        max_frames: usize,
        sample_rate: f64,
    ) -> Self {
        let mut bypass = bypass.into_iter();
        let slots = instances
            .into_iter()
            .map(|(instance, module)| {
                let process_buses = ProcessBusScratch::from_component(
                    instance.debug_name(),
                    instance.component(),
                    instance.processor(),
                    max_frames,
                    sample_rate,
                )
                .expect("prepared plugin exposes process buses");
                ChainSlot {
                    instance,
                    _module: module,
                    bypassed: bypass.next().unwrap_or(false),
                    process_buses,
                    parameter_edits: [ParameterEdit::default(); MAX_BLOCK_PARAMETER_EDITS],
                }
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
        self.process_in_place_with_midi(stereo, &[]);
    }

    /// Process an interleaved stereo block in place, passing live MIDI events to every active slot.
    pub fn process_in_place_with_midi(&mut self, stereo: &mut [f32], midi: &[MidiMessage]) {
        let frames = (stereo.len() / 2).min(self.a_left.len());

        for (frame, pair) in stereo.chunks_exact(2).take(frames).enumerate() {
            self.a_left[frame] = pair[0];
            self.a_right[frame] = pair[1];
        }

        for idx in 0..self.slots.len() {
            let slot = &mut self.slots[idx];
            if slot.bypassed {
                continue;
            }
            let parameter_count = slot
                .instance
                .parameter_edits()
                .drain(&mut slot.parameter_edits);
            // Clear the destination first: a plugin that fails (or writes nothing) must not
            // re-emit whatever the previous block left in the ping-pong buffer.
            self.b_left[..frames].fill(0.0);
            self.b_right[..frames].fill(0.0);
            let result = unsafe {
                slot.process_buses.drive_stereo_with_events(
                    slot.instance.processor(),
                    [&self.a_left[..frames], &self.a_right[..frames]],
                    [&mut self.b_left[..frames], &mut self.b_right[..frames]],
                    midi,
                    &slot.parameter_edits[..parameter_count],
                )
            };
            if !vst_ok(result) {
                // A failed slot acts bypassed for this block: keep the signal in the current
                // buffers instead of taking the (zeroed/garbage) output.
                continue;
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
    use crate::midi::MidiMessage;
    use crate::vst3_host::fixture::{
        context_fixture_factory, gain_fixture_factory, midi_note_fixture_factory,
        nan_fixture_factory, output_only_midi_note_fixture_factory, parameter_fixture_factory,
        process_error_factory, strict_sidechain_fixture_factory,
    };
    use crate::vst3_host::{HostContext, ProcessDriver};
    use vst3::ComPtr;
    use vst3::Steinberg::IPluginFactory;
    use vst3::Steinberg::Vst::IHostApplication;

    const TEST_SAMPLE_RATE: f64 = 48_000.0;

    fn host() -> ComPtr<IHostApplication> {
        HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
    }

    /// A prepared (set up + active) instance, shared like the controller's pool does.
    fn prepared(gain: f32, latency: u32) -> Arc<PluginInstance> {
        prepared_from_factory(gain_fixture_factory(gain, latency))
    }

    fn prepared_from_factory(factory: ComPtr<IPluginFactory>) -> Arc<PluginInstance> {
        let instance = PluginInstance::from_factory(&factory, &host()).expect("instance");
        ProcessDriver::new(TEST_SAMPLE_RATE, 512)
            .prepare(&instance)
            .expect("prepare");
        Arc::new(instance)
    }

    fn nan_instance() -> Arc<PluginInstance> {
        let instance =
            PluginInstance::from_factory(&nan_fixture_factory(), &host()).expect("instance");
        ProcessDriver::new(TEST_SAMPLE_RATE, 512)
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
            TEST_SAMPLE_RATE,
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
            TEST_SAMPLE_RATE,
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
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.3f32, 0.7];
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.3, 0.7]);
    }

    #[test]
    fn declared_extra_audio_busses_are_present_in_process_data() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(strict_sidechain_fixture_factory())],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.25f32, 0.5, 0.75, 1.0];

        chain.process_in_place(&mut stereo);

        assert_eq!(stereo, vec![0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn process_context_is_present_for_live_chain_plugins() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(context_fixture_factory())],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.25f32, 0.5, 0.75, 1.0];

        chain.process_in_place(&mut stereo);

        assert_eq!(stereo, vec![0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn midi_events_are_delivered_to_live_chain_plugins() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(midi_note_fixture_factory())],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.0f32; 4];

        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.0, 0.0, 0.0, 0.0]);

        stereo.fill(0.0);
        chain.process_in_place_with_midi(&mut stereo, &[MidiMessage::from_bytes(0x90, 60, 100)]);
        assert_eq!(stereo, vec![0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn output_only_instruments_receive_midi_and_generate_audio() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(
                output_only_midi_note_fixture_factory(),
            )],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.0f32; 4];

        chain.process_in_place_with_midi(&mut stereo, &[MidiMessage::from_bytes(0x90, 60, 100)]);

        assert_eq!(stereo, vec![0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn parameter_edits_are_delivered_to_live_chain_plugins() {
        let instance = prepared_from_factory(parameter_fixture_factory());
        let mut chain =
            ChainProcessor::new(vec![instance.clone()], vec![false], 512, TEST_SAMPLE_RATE);
        let mut stereo = vec![0.0f32; 4];

        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.0, 0.0, 0.0, 0.0]);

        assert!(
            instance
                .parameter_edits()
                .push(ParameterEdit::new(1, 0.625))
        );
        chain.process_in_place(&mut stereo);
        assert_eq!(stereo, vec![0.625, 0.625, 0.625, 0.625]);
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
            TEST_SAMPLE_RATE,
        );
        // Rebuild reversed from the same pooled instances; the forward chain is dropped here.
        drop(forward);
        let mut reversed = ChainProcessor::new(
            vec![pool[1].clone(), pool[0].clone()],
            vec![false, false],
            512,
            TEST_SAMPLE_RATE,
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
            TEST_SAMPLE_RATE,
        );
        assert_eq!(active.aggregate_latency(), 7);

        let bypassed = ChainProcessor::new(
            vec![prepared(1.0, 3), prepared(1.0, 4)],
            vec![true, true],
            512,
            TEST_SAMPLE_RATE,
        );
        assert_eq!(bypassed.aggregate_latency(), 0);
    }

    #[test]
    fn process_in_place_is_allocation_free() {
        let mut chain = ChainProcessor::new(
            vec![prepared(0.5, 0), prepared(0.5, 0)],
            vec![false, false],
            512,
            TEST_SAMPLE_RATE,
        );
        let mut stereo = vec![0.5f32; 256];
        lindelion_test_allocator::assert_no_allocations("chain process", || {
            chain.process_in_place(&mut stereo);
        });
    }

    #[test]
    fn process_in_place_with_midi_is_allocation_free() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(midi_note_fixture_factory())],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let midi = [MidiMessage::from_bytes(0x90, 60, 100)];
        let mut stereo = vec![0.0f32; 256];
        lindelion_test_allocator::assert_no_allocations("chain process with midi", || {
            chain.process_in_place_with_midi(&mut stereo, &midi);
        });
        assert!(stereo.iter().all(|&sample| sample == 0.25));
    }

    #[test]
    fn process_output_only_instrument_is_allocation_free() {
        let mut chain = ChainProcessor::new(
            vec![prepared_from_factory(
                output_only_midi_note_fixture_factory(),
            )],
            vec![false],
            512,
            TEST_SAMPLE_RATE,
        );
        let midi = [MidiMessage::from_bytes(0x90, 60, 100)];
        let mut stereo = vec![0.0f32; 256];
        lindelion_test_allocator::assert_no_allocations("output-only instrument process", || {
            chain.process_in_place_with_midi(&mut stereo, &midi);
        });
        assert!(stereo.iter().all(|&sample| sample == 0.25));
    }

    #[test]
    fn process_in_place_with_parameter_edits_is_allocation_free() {
        let instance = prepared_from_factory(parameter_fixture_factory());
        let mut chain =
            ChainProcessor::new(vec![instance.clone()], vec![false], 512, TEST_SAMPLE_RATE);
        let mut stereo = vec![0.0f32; 256];
        assert!(instance.parameter_edits().push(ParameterEdit::new(1, 0.5)));

        lindelion_test_allocator::assert_no_allocations(
            "chain process with parameter edit",
            || {
                chain.process_in_place(&mut stereo);
            },
        );
        assert!(stereo.iter().all(|&sample| sample == 0.5));
    }

    #[test]
    fn failing_slot_acts_bypassed_and_never_replays_stale_audio() {
        // Regression: a slot whose `process` fails must be skipped for that block — previously the
        // ping-pong buffers swapped unconditionally, so the chain re-emitted whatever the failing
        // slot's destination buffer held from an earlier block.
        let mut chain = ChainProcessor::new(
            vec![
                prepared_from_factory(process_error_factory()),
                prepared(0.5, 0),
            ],
            vec![false, false],
            512,
            TEST_SAMPLE_RATE,
        );
        // The failing slot contributes nothing; the healthy gain slot still runs.
        let mut first = vec![1.0f32, 1.0, 2.0, 2.0];
        chain.process_in_place(&mut first);
        assert_eq!(first, vec![0.5, 0.5, 1.0, 1.0]);
        // A second, different block must reflect its own input, not the first block's audio.
        let mut second = vec![0.2f32, 0.2];
        chain.process_in_place(&mut second);
        assert_eq!(second, vec![0.1, 0.1]);
    }

    #[test]
    fn nan_plugin_output_is_sanitized_to_finite() {
        let mut chain =
            ChainProcessor::new(vec![nan_instance()], vec![false], 512, TEST_SAMPLE_RATE);
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
