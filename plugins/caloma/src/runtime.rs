//! The chain runtime: processes a signal order's slots in sequence on a mono buffer.
//!
//! Each enabled slot processes the buffer in place, feeding the next; the shared analysis snapshot
//! (computed once at the chain head by the caller — `Caloma`, which owns the single `SharedAnalysis`)
//! is injected into every slot before it runs (a no-op for the non-consumer slots). The runtime
//! itself owns no analysis worker, so it stays pure and thread-free — testable in the fast unit
//! suite; the heavy off-thread analysis lives one level up (M1's `SharedAnalysis`).
//!
//! Per-slot bypass comes from the patch. Latency is fixed-max (D5): `latency_samples()` is the sum
//! over **all** the order's slots regardless of bypass, and a disabled latency-N slot is replaced
//! by a pure N-sample delay so the chain stays time-aligned and the host never re-negotiates
//! latency on a per-slot bypass.

use lindelion_dsp_utils::db_to_gain;
use lindelion_speech_signals::SignalSnapshot;

use crate::chain_effect::{ChainEffect, apply_slot_params, build_chain_slot};
use crate::order::SignalOrder;
use crate::patch::CalomaPatch;
use crate::slot::SlotId;
use crate::topology::order_topology;

/// A pure integer-sample delay used to delay-compensate a bypassed slot. Pre-allocated to the
/// slot's latency; a zero-latency slot has an empty buffer and is a no-op (bit-exact identity).
struct BlockDelay {
    buffer: Vec<f32>,
    head: usize,
}

impl BlockDelay {
    fn new(delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; delay_samples],
            head: 0,
        }
    }

    /// Delay `signal` in place by `self.buffer.len()` samples. Allocation-free.
    fn process(&mut self, signal: &mut [f32]) {
        if self.buffer.is_empty() {
            return;
        }
        for sample in signal.iter_mut() {
            let delayed = self.buffer[self.head];
            self.buffer[self.head] = *sample;
            self.head += 1;
            if self.head == self.buffer.len() {
                self.head = 0;
            }
            *sample = delayed;
        }
    }
}

/// One built slot: its id (for patch lookup), its boxed effect, and a delay matching the effect's
/// latency (used to delay-compensate the slot when it is bypassed).
struct Slot {
    id: SlotId,
    effect: Box<dyn ChainEffect>,
    latency: usize,
    bypass_delay: BlockDelay,
    /// Delays the slot's *dry* input by its latency so the dry/wet intensity blend stays
    /// time-aligned with the (latent) wet output. Only advanced on the blend path.
    dry_delay: BlockDelay,
}

/// The serial mono effect chain for a selected signal order.
pub struct ChainRuntime {
    order: SignalOrder,
    slots: Vec<Slot>,
    sample_rate: f32,
    max_block: usize,
    /// Pre-allocated scratch holding a slot's dry input during a dry/wet intensity blend.
    dry_scratch: Vec<f32>,
    /// The patch whose per-effect knob params were last pushed into the built effects
    /// (`None` = not yet applied, e.g. right after a rebuild). `CalomaPatch` is all-`Copy`
    /// fields, so the compare and the stored copy are allocation-free (ADR-0001).
    applied_params: Option<CalomaPatch>,
}

impl ChainRuntime {
    /// Build the runtime for `order`'s topology.
    pub fn new(order: SignalOrder, sample_rate: f32, max_block: usize) -> Self {
        let mut runtime = Self::from_slots(order_topology(order), sample_rate, max_block);
        runtime.order = order;
        runtime
    }

    /// Build the runtime from an explicit slot list. `set_order` still rebuilds from a topology.
    pub fn from_slots(ids: &[SlotId], sample_rate: f32, max_block: usize) -> Self {
        let mut runtime = Self {
            order: SignalOrder::Clarity,
            slots: Vec::new(),
            sample_rate,
            max_block,
            dry_scratch: vec![0.0; max_block],
            applied_params: None,
        };
        runtime.rebuild(ids);
        runtime
    }

    fn rebuild(&mut self, ids: &[SlotId]) {
        // Freshly built effects carry their crate defaults; the next `process` re-applies the
        // patch's knob params.
        self.applied_params = None;
        self.slots = ids
            .iter()
            .map(|&id| {
                let mut effect = build_chain_slot(id);
                effect.prepare(self.sample_rate, self.max_block);
                let latency = effect.latency_samples();
                Slot {
                    id,
                    effect,
                    latency,
                    bypass_delay: BlockDelay::new(latency),
                    dry_delay: BlockDelay::new(latency),
                }
            })
            .collect();
    }

    /// Clear every slot's streaming state and the delay lines, keeping the built slots (and their
    /// loaded NN models) in place. Off the audio path — used by the offline default-tuner to re-run
    /// fixtures from a clean state without rebuilding the chain.
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.effect.reset();
            slot.bypass_delay = BlockDelay::new(slot.latency);
            slot.dry_delay = BlockDelay::new(slot.latency);
        }
    }

    /// Re-route to a different order, rebuilding its slots.
    pub fn set_order(&mut self, order: SignalOrder) {
        self.order = order;
        self.rebuild(order_topology(order));
    }

    /// The current order.
    pub fn order(&self) -> SignalOrder {
        self.order
    }

    /// The ordered slot ids currently in the chain.
    pub fn slot_ids(&self) -> Vec<SlotId> {
        self.slots.iter().map(|slot| slot.id).collect()
    }

    /// Total chain latency in samples — the sum over **all** slots, independent of per-slot bypass
    /// (fixed-max, D5).
    pub fn latency_samples(&self) -> usize {
        self.slots.iter().map(|slot| slot.latency).sum()
    }

    /// Process one mono block in place. `snapshot` is the shared analysis snapshot for this block.
    ///
    /// Gain staging and per-effect intensity come from `patch` (the editor's control surface):
    /// the input trim (`input_level_db`) is applied at the chain head, the output level
    /// (`output_level_db`) at the tail, and each enabled slot is blended with its own latency-
    /// compensated dry input by its `intensity` (1.0 = fully wet, 0.0 = the effect contributes
    /// nothing). An enabled slot at full intensity processes normally (applying its own latency);
    /// a disabled slot is replaced by a delay equal to its latency (so the chain stays time-
    /// aligned), which is a no-op for a zero-latency slot.
    pub fn process(
        &mut self,
        buffer: &mut [f32],
        patch: &CalomaPatch,
        snapshot: &SignalSnapshot,
    ) -> BlockMeters {
        let Self {
            slots,
            dry_scratch,
            applied_params,
            ..
        } = self;
        // Push the patch's per-effect knob params into the effects whenever the patch changes
        // (and on the first block after a build). Without this application the effects would run
        // at their crate defaults forever — the persisted patch and the committed per-order
        // defaults would be decorative.
        if applied_params.as_ref() != Some(patch) {
            for slot in slots.iter_mut() {
                apply_slot_params(slot.effect.as_mut(), slot.id, patch);
            }
            *applied_params = Some(patch.clone());
        }
        apply_gain(buffer, db_to_gain(patch.input_level_db));
        let input_peak = block_peak(buffer);
        for slot in slots.iter_mut() {
            slot.effect.set_snapshot(snapshot);
            if !patch.slot_enabled(slot.id) {
                slot.bypass_delay.process(buffer);
                continue;
            }
            let intensity = patch.slot_intensity(slot.id).clamp(0.0, 1.0);
            if intensity >= 1.0 {
                slot.effect.process(buffer);
                continue;
            }
            // Dry/wet intensity blend, latency-compensated: keep the slot input, run the wet path
            // in place, delay the dry copy by the slot's latency to align it with the wet output,
            // then crossfade.
            let n = buffer.len();
            let dry = &mut dry_scratch[..n];
            dry.copy_from_slice(buffer);
            slot.effect.process(buffer);
            slot.dry_delay.process(dry);
            for (out, &d) in buffer.iter_mut().zip(dry.iter()) {
                *out = d * (1.0 - intensity) + *out * intensity;
            }
        }
        apply_gain(buffer, db_to_gain(patch.output_level_db));
        BlockMeters {
            input_peak,
            output_peak: block_peak(buffer),
        }
    }
}

/// Per-block peak levels tapped at the chain head (after input trim) and tail (after output level),
/// in linear amplitude. The plugin publishes these to `SharedControls` so the editor's in/out meters
/// can display the signal the user is actually staging (`set_input_meter`/`set_output_meter`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockMeters {
    pub input_peak: f32,
    pub output_peak: f32,
}

/// Scale `buffer` in place by `gain` (skipped at unity so 0 dB is bit-exact).
fn apply_gain(buffer: &mut [f32], gain: f32) {
    if gain == 1.0 {
        return;
    }
    for sample in buffer.iter_mut() {
        *sample *= gain;
    }
}

/// The block's peak (max `|sample|`), in linear amplitude. Allocation-free (a plain reduction over
/// the buffer the audio thread already owns), so it is safe to call inside `process` (ADR-0001).
fn block_peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0_f32, |peak, &s| peak.max(s.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_like(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / 48_000.0;
                0.2 * ((150.0 * t).sin() + 0.5 * (300.0 * t).sin() + 0.25 * (450.0 * t).sin())
            })
            .collect()
    }

    // A representative multi-stage chain with NO neural slots, so the runtime mechanics
    // (ordering, bypass, latency, finiteness) stay testable in the fast unit suite. The real NN
    // orders are exercised by the heavy e2e (`tests/chain_e2e.rs`, `make test-models`).
    const NON_NN_CHAIN: &[SlotId] = &[
        SlotId::Gain,
        SlotId::HighPass,
        SlotId::Dereverberation,
        SlotId::FiveBandEq,
        SlotId::DynamicEq,
        SlotId::Compressor,
        SlotId::Vitalizer,
        SlotId::BassEnhancer,
        SlotId::AirExciter,
        SlotId::UpwardExpander,
        SlotId::DeEsser,
        SlotId::Limiter,
    ];

    fn enable(patch: &mut CalomaPatch, ids: &[SlotId]) {
        // Mirror of `slot_enabled`: set each named slot's enable flag. Test-only.
        for &id in ids {
            match id {
                SlotId::Gain => patch.gain.enabled = true,
                SlotId::HighPass => patch.high_pass.enabled = true,
                SlotId::Dereverberation => patch.dereverberation.enabled = true,
                SlotId::FiveBandEq => patch.five_band_eq.enabled = true,
                SlotId::DynamicEq => patch.dynamic_eq.enabled = true,
                SlotId::Compressor => patch.compressor.enabled = true,
                SlotId::Vitalizer => patch.vitalizer.enabled = true,
                SlotId::BassEnhancer => patch.bass_enhancer.enabled = true,
                SlotId::AirExciter => patch.air_exciter.enabled = true,
                SlotId::UpwardExpander => patch.upward_expander.enabled = true,
                SlotId::DeEsser => patch.de_esser.enabled = true,
                SlotId::Limiter => patch.limiter.enabled = true,
                other => panic!("test helper does not handle {other:?}"),
            }
        }
    }

    #[test]
    fn full_chain_output_is_finite_and_non_clipping() {
        let mut runtime = ChainRuntime::from_slots(NON_NN_CHAIN, 48_000.0, 4_096);
        let mut patch = CalomaPatch::default();
        enable(&mut patch, NON_NN_CHAIN);
        let mut buffer = speech_like(4_096);
        runtime.process(&mut buffer, &patch, &SignalSnapshot::default());
        assert!(
            buffer.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "chain output must be finite and non-clipping"
        );
    }

    #[test]
    fn disabled_latency_zero_slot_is_bit_exact_identity() {
        // FiveBandEq has zero latency; disabled (the default), it must leave the buffer untouched.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let patch = CalomaPatch::default();
        assert!(!patch.slot_enabled(SlotId::FiveBandEq));
        let input: Vec<f32> = (0..1_024)
            .map(|i| 0.3 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        runtime.process(&mut buffer, &patch, &SignalSnapshot::default());
        assert_eq!(buffer, input);
    }

    #[test]
    fn process_reports_head_and_tail_peaks() {
        // An identity chain (FiveBandEq disabled, zero latency): the input meter is tapped after the
        // input trim and the output meter after the output level, so a gain change moves the tail
        // peak but not the input peak relative to a unity reference.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let buffer: Vec<f32> = (0..1_024)
            .map(|i| 0.5 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();
        let input_peak = block_peak(&buffer);

        // Unity gain staging: both meters see the unaltered signal.
        let mut patch = CalomaPatch::default();
        let mut unity = buffer.clone();
        let meters = runtime.process(&mut unity, &patch, &SignalSnapshot::default());
        assert!((meters.input_peak - input_peak).abs() < 1e-6);
        assert!((meters.output_peak - input_peak).abs() < 1e-6);

        // A -6 dB output level halves the tail peak (~0.501x) but leaves the head peak unchanged.
        patch.output_level_db = -6.0;
        let mut attenuated = buffer.clone();
        let meters = runtime.process(&mut attenuated, &patch, &SignalSnapshot::default());
        assert!((meters.input_peak - input_peak).abs() < 1e-6);
        assert!(meters.output_peak < input_peak * 0.6);
        assert!(meters.output_peak > input_peak * 0.4);
    }

    #[test]
    fn from_slots_builds_the_given_sequence() {
        // The rebuild primitive that `new`/`set_order` use. Re-routing across the real (NN) orders
        // is covered by the heavy e2e.
        let runtime = ChainRuntime::from_slots(NON_NN_CHAIN, 48_000.0, 1_024);
        assert_eq!(runtime.slot_ids(), NON_NN_CHAIN.to_vec());
        let other = ChainRuntime::from_slots(&[SlotId::HighPass, SlotId::Limiter], 48_000.0, 1_024);
        assert_eq!(other.slot_ids(), vec![SlotId::HighPass, SlotId::Limiter]);
        assert_ne!(runtime.slot_ids(), other.slot_ids());
    }

    fn slot_latency(id: SlotId) -> usize {
        ChainRuntime::from_slots(&[id], 48_000.0, 4_096).latency_samples()
    }

    #[test]
    fn reported_latency_is_the_sum_over_all_slots_and_bypass_independent() {
        let runtime = ChainRuntime::from_slots(NON_NN_CHAIN, 48_000.0, 4_096);
        // The chain carries latency from Dereverberation + Limiter; everything else is zero.
        let expected = NON_NN_CHAIN
            .iter()
            .map(|&id| slot_latency(id))
            .sum::<usize>();
        assert_eq!(runtime.latency_samples(), expected);
        // `latency_samples` consults no patch, so it is constant regardless of which slots a patch
        // disables (fixed-max).
        assert!(slot_latency(SlotId::Dereverberation) > 0);
    }

    #[test]
    fn disabled_latency_slot_is_delay_compensated() {
        // A bypassed Dereverberation outputs the dry signal delayed by exactly its latency.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::Dereverberation], 48_000.0, 8_192);
        let patch = CalomaPatch::default();
        assert!(!patch.slot_enabled(SlotId::Dereverberation));
        let lat = runtime.latency_samples();
        assert!(lat > 0 && lat < 8_000);

        let mut buffer = vec![0.0_f32; lat + 64];
        buffer[0] = 1.0; // impulse at the head
        runtime.process(&mut buffer, &patch, &SignalSnapshot::default());

        assert_eq!(
            buffer[lat], 1.0,
            "impulse must appear delayed by the slot latency"
        );
        assert!(
            buffer[..lat].iter().all(|&s| s == 0.0),
            "pre-delay region must be silent"
        );
        assert!(
            buffer[lat + 1..].iter().all(|&s| s == 0.0),
            "only the delayed impulse should be present"
        );
    }

    #[test]
    fn intensity_zero_on_latency_zero_slot_passes_dry_through() {
        // FiveBandEq is latency-0; enabled but at intensity 0 it must contribute nothing — the
        // dry/wet blend collapses to the (undelayed) dry input.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let mut patch = CalomaPatch::default();
        enable(&mut patch, &[SlotId::FiveBandEq]);
        patch.five_band_eq.intensity = 0.0;
        let input = speech_like(1_024);
        let mut buffer = input.clone();
        runtime.process(&mut buffer, &patch, &SignalSnapshot::default());
        assert_eq!(
            buffer, input,
            "intensity 0 must pass the dry signal unchanged"
        );
    }

    #[test]
    fn full_intensity_matches_the_wet_effect() {
        // At intensity 1.0 the blend is bypassed entirely: the output is the bare effect.
        let mut wet = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let mut blend = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let mut patch = CalomaPatch::default();
        enable(&mut patch, &[SlotId::FiveBandEq]);
        patch.five_band_eq.intensity = 1.0;
        let input = speech_like(1_024);
        let mut wet_buf = input.clone();
        let mut blend_buf = input.clone();
        wet.process(&mut wet_buf, &patch, &SignalSnapshot::default());
        blend.process(&mut blend_buf, &patch, &SignalSnapshot::default());
        assert_eq!(wet_buf, blend_buf);
        assert_ne!(wet_buf, input, "the effect must actually change the signal");
    }

    #[test]
    fn partial_intensity_lies_between_dry_and_wet() {
        // A latency-0 slot at intensity 0.5 is the arithmetic mean of dry and full-wet.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let mut wet_rt = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 1_024);
        let input = speech_like(1_024);

        let mut wet_patch = CalomaPatch::default();
        enable(&mut wet_patch, &[SlotId::FiveBandEq]);
        let mut wet = input.clone();
        wet_rt.process(&mut wet, &wet_patch, &SignalSnapshot::default());

        let mut half_patch = CalomaPatch::default();
        enable(&mut half_patch, &[SlotId::FiveBandEq]);
        half_patch.five_band_eq.intensity = 0.5;
        let mut half = input.clone();
        runtime.process(&mut half, &half_patch, &SignalSnapshot::default());

        for ((h, d), w) in half.iter().zip(input.iter()).zip(wet.iter()) {
            assert!(
                (*h - 0.5 * (d + w)).abs() < 1e-5,
                "expected dry/wet midpoint"
            );
        }
    }

    #[test]
    fn input_and_output_levels_scale_by_db_gain() {
        // An empty chain just applies the head/tail gains; +6 dB in and -6 dB out is unity overall,
        // while either alone scales by its dB gain.
        let mut runtime = ChainRuntime::from_slots(&[], 48_000.0, 1_024);
        let input = speech_like(1_024);

        let mut boosted = input.clone();
        let patch = CalomaPatch {
            input_level_db: 6.0,
            ..CalomaPatch::default()
        };
        runtime.process(&mut boosted, &patch, &SignalSnapshot::default());
        let g = db_to_gain(6.0);
        for (b, i) in boosted.iter().zip(input.iter()) {
            assert!(
                (*b - i * g).abs() < 1e-5,
                "input trim must scale by its dB gain"
            );
        }

        let mut unity = input.clone();
        let io_patch = CalomaPatch {
            input_level_db: 6.0,
            output_level_db: -6.0,
            ..CalomaPatch::default()
        };
        runtime.process(&mut unity, &io_patch, &SignalSnapshot::default());
        for (u, i) in unity.iter().zip(input.iter()) {
            assert!((*u - i).abs() < 1e-5, "+6/-6 dB must round-trip to unity");
        }
    }

    #[test]
    fn patch_knob_params_reach_the_effects() {
        // Regression: the persisted per-effect knob params must be pushed into the built effects
        // (they were once serialized but never applied, so every effect ran at its crate defaults
        // and the tuner's objective was flat). Two patches differing only in the EQ high-shelf
        // gain must produce audibly different output from the same runtime — including on the
        // block right after the change.
        let mut runtime = ChainRuntime::from_slots(&[SlotId::FiveBandEq], 48_000.0, 4_096);
        let mut patch = CalomaPatch::default();
        patch.five_band_eq.enabled = true;
        let input: Vec<f32> = (0..4_096)
            .map(|i| 0.3 * (std::f32::consts::TAU * 8_000.0 * i as f32 / 48_000.0).sin())
            .collect();
        let tail_rms = |b: &[f32]| {
            let tail = &b[b.len() / 2..];
            (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
        };

        patch.five_band_eq.params.high_shelf_gain = -24.0;
        let mut cut = input.clone();
        runtime.process(&mut cut, &patch, &SignalSnapshot::default());

        patch.five_band_eq.params.high_shelf_gain = 24.0;
        let mut boosted = input.clone();
        runtime.process(&mut boosted, &patch, &SignalSnapshot::default());

        assert!(
            tail_rms(&boosted) > tail_rms(&cut) * 4.0,
            "knob params must reach the DSP: cut {} vs boost {}",
            tail_rms(&cut),
            tail_rms(&boosted)
        );
    }

    #[test]
    fn blended_chain_process_is_allocation_free() {
        // The dry/wet intensity blend and gain staging run on the audio thread (ADR-0001).
        let mut runtime = ChainRuntime::from_slots(NON_NN_CHAIN, 48_000.0, 4_096);
        let mut patch = CalomaPatch::default();
        enable(&mut patch, NON_NN_CHAIN);
        patch.input_level_db = -2.0;
        patch.output_level_db = 1.0;
        // Force the blend path on a latent and a latency-0 slot.
        patch.dereverberation.intensity = 0.5;
        patch.five_band_eq.intensity = 0.3;
        let mut buffer = speech_like(4_096);
        let snapshot = SignalSnapshot::default();
        crate::assert_no_allocations("caloma runtime blend", || {
            runtime.process(&mut buffer, &patch, &snapshot);
        });
        assert!(buffer.iter().all(|s| s.is_finite()));
    }
}
