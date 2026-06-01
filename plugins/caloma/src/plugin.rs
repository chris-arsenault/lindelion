//! The host-agnostic, self-contained Calóma plugin. It surfaces **no host-automatable parameters**
//! (ADR-0023): its Vizia editor is the sole control surface, writing the live `SharedControls`
//! (order, per-slot enable + dry/wet intensity, input/output level) that `process` reads each block.
//! All three signal orders' chains are pre-built in `reset` so an order change is a lock-free index
//! flip — never an audio-thread rebuild (ADR-0001). The single off-thread analysis worker
//! (`SharedAnalysis`) is built in `reset`; its snapshot is computed once per block at the chain head
//! and injected into the chain. Per ADR-0013 this depends only on plugin-shell, the pure effect
//! crates, dsp-utils, and Calóma's own data model.

use std::sync::Arc;

use lindelion_plugin_shell::{
    AudioPlugin, ParameterCodec, ParameterInfo, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup,
};
use lindelion_speech_signals::SignalSnapshot;

use crate::analysis::SharedAnalysis;
use crate::controls::SharedControls;
use crate::order::SignalOrder;
use crate::patch::{CalomaPatch, default_patch_for};
use crate::patch_io::{self, FORMAT_VERSION};
use crate::runtime::ChainRuntime;

/// Plugin descriptor. The 16-byte class id is distinct from the other plugins'.
pub const DESCRIPTOR: PluginDescriptor = PluginDescriptor::effect("Calóma", *b"lindelion_caloma");

/// No host-automatable parameters: the editor is the control surface (ADR-0023).
const PARAMETERS: &[ParameterInfo] = &[];

/// The Calóma plugin: the durable patch, the live shared controls (editor↔DSP), one pre-built chain
/// per signal order, the shared analysis worker, and audio-thread scratch (mono + a projected patch).
pub struct Caloma {
    setup: ProcessSetup,
    /// The durable, persisted patch (carries the per-effect knob params not exposed in the editor).
    patch: CalomaPatch,
    /// The live control surface the editor writes and `process` reads (lock-free).
    controls: Arc<SharedControls>,
    /// One pre-built chain per `SignalOrder`, indexed by `SignalOrder::to_index`.
    chains: [Option<ChainRuntime>; SignalOrder::ALL.len()],
    analysis: Option<SharedAnalysis>,
    mono: Vec<f32>,
    /// Audio-thread-owned scratch the live controls are projected into each block, so the runtime
    /// stays patch-driven without the audio thread ever touching the main-thread `patch`.
    controls_patch: CalomaPatch,
}

impl Default for Caloma {
    fn default() -> Self {
        let patch = default_patch_for(SignalOrder::Clarity);
        let controls = Arc::new(SharedControls::from_patch(&patch));
        Self {
            setup: ProcessSetup::default(),
            controls_patch: patch.clone(),
            patch,
            controls,
            chains: [None, None, None],
            analysis: None,
            mono: Vec::new(),
        }
    }
}

impl Caloma {
    /// The live shared controls — the object the VST3 adapter hands to the Vizia editor (both the
    /// editor and this DSP read/write the same atomics, with no host-relayed channel).
    pub fn controls(&self) -> Arc<SharedControls> {
        Arc::clone(&self.controls)
    }

    /// Total latency in samples of the *currently selected* order's chain (0 until prepared). The
    /// VST3 adapter reports this; an order change re-reports it.
    pub fn latency_samples(&self) -> usize {
        let index = self.controls.order().to_index() as usize;
        self.chains[index]
            .as_ref()
            .map_or(0, ChainRuntime::latency_samples)
    }

    /// Build a single (non-NN) chain for the current order without the off-thread analysis worker,
    /// so the stereo bridge + control application can be exercised in the fast unit suite (the
    /// snapshot falls back to the default). Production builds every order's real topology + the
    /// worker in `reset`; that path and the real NN orders are covered by the heavy e2e. Seeds the
    /// live controls from the current patch.
    #[cfg(test)]
    fn install_chain_without_analysis(
        &mut self,
        slots: &[crate::slot::SlotId],
        sample_rate: f32,
        max_block: usize,
    ) {
        let max_block = max_block.max(1);
        let index = self.controls.order().to_index() as usize;
        self.chains = [None, None, None];
        self.chains[index] = Some(ChainRuntime::from_slots(slots, sample_rate, max_block));
        self.analysis = None;
        self.mono = vec![0.0; max_block];
        self.controls_patch = self.patch.clone();
        self.controls.load_from_patch(&self.patch);
    }

    /// Install a distinct (non-NN) chain for every order, so order-routing and per-order latency can
    /// be exercised in the fast unit suite without building the real (NN) topologies. `per_order` is
    /// indexed by `SignalOrder::to_index`.
    #[cfg(test)]
    fn install_order_chains_without_analysis(
        &mut self,
        per_order: [&[crate::slot::SlotId]; SignalOrder::ALL.len()],
        sample_rate: f32,
        max_block: usize,
    ) {
        let max_block = max_block.max(1);
        for order in SignalOrder::ALL {
            let index = order.to_index() as usize;
            self.chains[index] = Some(ChainRuntime::from_slots(
                per_order[index],
                sample_rate,
                max_block,
            ));
        }
        self.analysis = None;
        self.mono = vec![0.0; max_block];
        self.controls_patch = self.patch.clone();
        self.controls.load_from_patch(&self.patch);
    }
}

impl AudioPlugin for Caloma {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        let sample_rate = setup.sample_rate as f32;
        let max_block = setup.max_block_size.max(1);
        // Pre-build every order's chain so an order switch is a lock-free index flip in `process`
        // (heavy one-time model load off the audio thread). Indexed by `SignalOrder::to_index`.
        for order in SignalOrder::ALL {
            self.chains[order.to_index() as usize] =
                Some(ChainRuntime::new(order, sample_rate, max_block));
        }
        self.analysis = Some(SharedAnalysis::new(setup.sample_rate as u32));
        self.mono = vec![0.0; max_block];
        self.controls_patch = self.patch.clone();
        self.controls.load_from_patch(&self.patch);
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        let ProcessContext { input, buffer, .. } = context;
        let frames = buffer
            .left
            .len()
            .min(buffer.right.len())
            .min(self.mono.len());
        let mono = &mut self.mono[..frames];
        downmix_to_mono(input.left, input.right, mono);
        let snapshot = match self.analysis.as_ref() {
            Some(analysis) => analysis.update(mono),
            None => SignalSnapshot::default(),
        };
        // Project the live controls into the audio-thread patch, then run the selected order's chain.
        let index = self.controls.order().to_index() as usize;
        self.controls.store_to_patch(&mut self.controls_patch);
        if let Some(chain) = self.chains[index].as_mut() {
            chain.process(mono, &self.controls_patch, &snapshot);
        }
        write_mono_to_stereo(mono, buffer.left, buffer.right);
    }

    fn state(&self) -> PluginState {
        // Overlay the live controls onto the durable patch (which keeps the knob params) before
        // serializing, so the editor's edits persist.
        let mut patch = self.patch.clone();
        self.controls.store_to_patch(&mut patch);
        patch_io::to_plugin_state(&patch).unwrap_or_else(|_| PluginState::empty(FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(patch) = patch_io::from_plugin_state(state) {
            self.controls.load_from_patch(&patch);
            self.patch = patch;
        }
    }
}

/// Average the present input channels into `out` (mono). Allocation-free.
fn downmix_to_mono(left: Option<&[f32]>, right: Option<&[f32]>, out: &mut [f32]) {
    match (left, right) {
        (Some(l), Some(r)) => {
            for (i, o) in out.iter_mut().enumerate() {
                *o = 0.5 * (l.get(i).copied().unwrap_or(0.0) + r.get(i).copied().unwrap_or(0.0));
            }
        }
        (Some(c), None) | (None, Some(c)) => {
            for (i, o) in out.iter_mut().enumerate() {
                *o = c.get(i).copied().unwrap_or(0.0);
            }
        }
        (None, None) => out.fill(0.0),
    }
}

/// Write the mono result to both output channels (zeroing any samples past the mono buffer).
/// Allocation-free.
fn write_mono_to_stereo(mono: &[f32], left: &mut [f32], right: &mut [f32]) {
    for (i, s) in left.iter_mut().enumerate() {
        *s = mono.get(i).copied().unwrap_or(0.0);
    }
    for (i, s) in right.iter_mut().enumerate() {
        *s = mono.get(i).copied().unwrap_or(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioBuffer, AudioInputBuffer};

    #[test]
    fn downmix_averages_present_channels() {
        let left = [1.0_f32, 0.0, -0.5];
        let right = [0.0_f32, 1.0, 0.5];
        let mut out = [9.0_f32; 3];
        downmix_to_mono(Some(&left), Some(&right), &mut out);
        assert_eq!(out, [0.5, 0.5, 0.0]);
        // A single present channel passes through unchanged.
        downmix_to_mono(Some(&left), None, &mut out);
        assert_eq!(out, left);
        downmix_to_mono(None, None, &mut out);
        assert_eq!(out, [0.0; 3]);
    }

    #[test]
    fn write_mono_to_stereo_copies_to_both() {
        let mono = [0.1_f32, -0.2, 0.3];
        let mut left = [9.0_f32; 3];
        let mut right = [9.0_f32; 3];
        write_mono_to_stereo(&mono, &mut left, &mut right);
        assert_eq!(left, mono);
        assert_eq!(right, mono);
    }

    #[test]
    fn process_runs_the_chain_finite_non_clipping_and_allocation_free() {
        use crate::slot::SlotId;
        let mut plugin = Caloma::default();
        // A non-NN chain so the fast suite stays model-free (real orders are heavy → e2e).
        let slots = [
            SlotId::HighPass,
            SlotId::FiveBandEq,
            SlotId::Compressor,
            SlotId::Limiter,
        ];
        plugin.patch.high_pass.enabled = true;
        plugin.patch.five_band_eq.enabled = true;
        plugin.patch.compressor.enabled = true;
        plugin.patch.limiter.enabled = true;
        plugin.install_chain_without_analysis(&slots, 48_000.0, 512);

        let input: Vec<f32> = (0..512)
            .map(|i| 0.2 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut out_l = [7.0_f32; 512];
        let mut out_r = [7.0_f32; 512];

        crate::assert_no_allocations("caloma chain process", || {
            let context = ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut out_l,
                    right: &mut out_r,
                },
                &[],
            )
            .with_input(AudioInputBuffer::stereo(&input, &input));
            plugin.process(context);
        });

        assert!(
            out_l.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "chain output must be finite and non-clipping"
        );
        assert_eq!(
            out_l, out_r,
            "the mono chain writes both channels identically"
        );
        assert_ne!(
            out_l.to_vec(),
            input,
            "the chain must alter the signal (no longer a passthrough)"
        );
    }

    #[test]
    fn latency_reflects_the_chain() {
        use crate::slot::SlotId;
        let slots = [SlotId::Dereverberation, SlotId::Limiter];
        let mut plugin = Caloma::default();
        assert_eq!(plugin.latency_samples(), 0, "no latency before prepare");
        plugin.install_chain_without_analysis(&slots, 48_000.0, 512);
        let expected = ChainRuntime::from_slots(&slots, 48_000.0, 512).latency_samples();
        assert_eq!(plugin.latency_samples(), expected);
    }

    #[test]
    fn exposes_no_host_parameters() {
        // The editor is the control surface (ADR-0023); the host sees no automatable parameters.
        assert!(Caloma::default().parameters().is_empty());
    }

    #[test]
    fn state_round_trips_the_live_controls_and_knob_params() {
        use crate::slot::SlotId;
        let mut plugin = Caloma::default();
        // A knob param the editor never touches lives only in the patch.
        plugin.patch.compressor.params.ratio = 8.0;
        // The editor's control surface lives in the shared controls.
        plugin.controls.set_order(SignalOrder::Broadcast);
        plugin.controls.set_slot_enabled(SlotId::Compressor, true);
        plugin.controls.set_slot_intensity(SlotId::Compressor, 0.5);
        plugin.controls.set_input_level_db(-2.0);
        plugin.controls.set_output_level_db(1.0);

        // The persisted patch is the durable patch overlaid with the live controls.
        let mut expected = plugin.patch.clone();
        plugin.controls.store_to_patch(&mut expected);

        let state = plugin.state();
        let mut restored = Caloma::default();
        restored.load_state(state);

        assert_eq!(restored.patch, expected);
        // The restored controls reflect the persisted control surface.
        assert_eq!(restored.controls.order(), SignalOrder::Broadcast);
        assert_eq!(restored.controls.slot_intensity(SlotId::Compressor), 0.5);
        assert_eq!(restored.controls.input_level_db(), -2.0);
    }

    #[test]
    fn process_applies_the_live_controls() {
        use crate::slot::SlotId;
        // FiveBandEq is latency-0: enabled at intensity 0 it contributes nothing, so the chain
        // output equals the (downmixed) input. Driving everything from the shared controls proves
        // `process` reads them, not the patch.
        let mut plugin = Caloma::default();
        plugin.install_chain_without_analysis(&[SlotId::FiveBandEq], 48_000.0, 256);
        plugin.controls.set_slot_enabled(SlotId::FiveBandEq, true);
        // Neutralize the (tuned) default gain staging so this test isolates the intensity blend.
        plugin.controls.set_input_level_db(0.0);
        plugin.controls.set_output_level_db(0.0);

        let input: Vec<f32> = (0..256)
            .map(|i| 0.2 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();

        let run = |plugin: &mut Caloma| {
            let mut out_l = [0.0_f32; 256];
            let mut out_r = [0.0_f32; 256];
            let context = ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut out_l,
                    right: &mut out_r,
                },
                &[],
            )
            .with_input(AudioInputBuffer::stereo(&input, &input));
            plugin.process(context);
            out_l
        };

        plugin.controls.set_slot_intensity(SlotId::FiveBandEq, 1.0);
        let wet = run(&mut plugin);
        plugin.controls.set_slot_intensity(SlotId::FiveBandEq, 0.0);
        let dry = run(&mut plugin);

        for (d, i) in dry.iter().zip(input.iter()) {
            assert!(
                (*d - i).abs() < 1e-5,
                "intensity 0 must pass the input through"
            );
        }
        assert_ne!(wet.to_vec(), input, "intensity 1 must apply the effect");
    }

    #[test]
    fn process_routes_to_the_selected_orders_chain() {
        use crate::slot::SlotId;
        // Clarity (idx 0) → an EQ that alters the signal; Light (idx 2) → an empty passthrough chain.
        // Flipping the order atomic must change which pre-built chain `process` runs.
        let mut plugin = Caloma::default();
        plugin.install_order_chains_without_analysis(
            [&[SlotId::FiveBandEq], &[SlotId::FiveBandEq], &[]],
            48_000.0,
            256,
        );
        plugin.controls.set_slot_enabled(SlotId::FiveBandEq, true);
        // Neutralize the (tuned) default gain staging so the empty Light chain is a true passthrough.
        plugin.controls.set_input_level_db(0.0);
        plugin.controls.set_output_level_db(0.0);

        let input: Vec<f32> = (0..256)
            .map(|i| 0.2 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();
        let run = |plugin: &mut Caloma| {
            let mut out_l = [0.0_f32; 256];
            let mut out_r = [0.0_f32; 256];
            let context = ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut out_l,
                    right: &mut out_r,
                },
                &[],
            )
            .with_input(AudioInputBuffer::stereo(&input, &input));
            plugin.process(context);
            out_l
        };

        plugin.controls.set_order(SignalOrder::Light);
        let passthrough = run(&mut plugin);
        plugin.controls.set_order(SignalOrder::Clarity);
        let altered = run(&mut plugin);

        for (p, i) in passthrough.iter().zip(input.iter()) {
            assert!(
                (*p - i).abs() < 1e-6,
                "the empty Light chain must pass through"
            );
        }
        assert_ne!(
            altered.to_vec(),
            input,
            "the Clarity chain must alter the signal"
        );
    }

    #[test]
    fn latency_tracks_the_selected_order() {
        // Per-order pre-built chains with distinct latencies; flipping the order atomic re-reports
        // that order's chain latency (non-NN chains keep this in the fast suite).
        use crate::slot::SlotId;
        let per_order: [&[SlotId]; 3] = [
            &[SlotId::FiveBandEq],                       // Clarity: latency 0
            &[SlotId::Dereverberation],                  // Broadcast: latency > 0
            &[SlotId::Dereverberation, SlotId::Limiter], // Light: a different, larger latency
        ];
        let mut plugin = Caloma::default();
        plugin.install_order_chains_without_analysis(per_order, 48_000.0, 8_192);
        for order in SignalOrder::ALL {
            plugin.controls.set_order(order);
            let expected =
                ChainRuntime::from_slots(per_order[order.to_index() as usize], 48_000.0, 8_192)
                    .latency_samples();
            assert_eq!(plugin.latency_samples(), expected, "{order:?}");
        }
    }
}
