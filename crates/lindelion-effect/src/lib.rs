//! Host-agnostic audio-effect contract.
//!
//! [`Effect`] is the effect-processor counterpart to `lindelion-plugin-shell`'s host-coupled
//! `AudioPlugin`. It depends on no host, no VST3, and no UI (ADR-0013), and its [`Effect::process`]
//! is allocation-free on the audio thread (ADR-0001). It exposes only neutral primitives: a
//! sample-block `process`, indexed float parameters, opaque byte-blob state, latency in samples,
//! and bypass — enough for any packaging (standalone app, single VST, or one VST per effect) to
//! adapt without the core choosing one.

#![forbid(unsafe_code)]

/// Descriptor for one indexed float parameter exposed by an effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectParam {
    /// Stable index used by [`Effect::set_parameter`].
    pub index: u32,
    /// Human-readable parameter name.
    pub name: &'static str,
    /// Inclusive minimum plain value.
    pub min: f32,
    /// Inclusive maximum plain value.
    pub max: f32,
    /// Default plain value.
    pub default: f32,
    /// Unit label (for example `"dB"`); empty when unitless.
    pub unit: &'static str,
}

/// A host-agnostic audio effect that processes one block of mono samples in place.
///
/// Contract:
/// - [`Effect::process`] must not allocate (ADR-0001).
/// - A bypassed effect is identity: while [`Effect::is_bypassed`] is `true`, `process` leaves the
///   buffer unchanged.
/// - [`Effect::latency_samples`] reports the delay between an input sample and its first effect on
///   the output.
pub trait Effect {
    /// Stable, human-readable effect name.
    fn name(&self) -> &str;

    /// The effect's indexed float parameters, in index order.
    fn parameters(&self) -> &[EffectParam];

    /// Set a parameter by its [`EffectParam::index`]. Unknown indices are ignored.
    fn set_parameter(&mut self, index: u32, value: f32);

    /// Prepare for playback at `sample_rate` with blocks up to `max_block` samples. Implementors
    /// size any internal scratch here so [`Effect::process`] need not allocate.
    fn prepare(&mut self, sample_rate: f32, max_block: usize);

    /// Process one block in place. Must not allocate; must be identity when bypassed.
    fn process(&mut self, buffer: &mut [f32]);

    /// Processing latency in samples (`0` when the effect adds none).
    fn latency_samples(&self) -> usize {
        0
    }

    /// Whether the effect is currently bypassed.
    fn is_bypassed(&self) -> bool;

    /// Set the bypass state.
    fn set_bypassed(&mut self, bypassed: bool);

    /// Clear internal state (smoothers, filter memory) without changing parameters.
    fn reset(&mut self);

    /// Serialize parameter state to an opaque blob. Not called on the audio thread.
    fn save_state(&self) -> Vec<u8>;

    /// Restore parameter state from a blob produced by [`Effect::save_state`].
    fn load_state(&mut self, state: &[u8]);
}

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal transparent effect used to exercise the trait surface.
    #[derive(Default)]
    struct Passthrough {
        bypassed: bool,
    }

    impl Effect for Passthrough {
        fn name(&self) -> &str {
            "Passthrough"
        }
        fn parameters(&self) -> &[EffectParam] {
            &[]
        }
        fn set_parameter(&mut self, _index: u32, _value: f32) {}
        fn prepare(&mut self, _sample_rate: f32, _max_block: usize) {}
        fn process(&mut self, _buffer: &mut [f32]) {}
        fn is_bypassed(&self) -> bool {
            self.bypassed
        }
        fn set_bypassed(&mut self, bypassed: bool) {
            self.bypassed = bypassed;
        }
        fn reset(&mut self) {}
        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }
        fn load_state(&mut self, _state: &[u8]) {}
    }

    #[test]
    fn effect_is_object_safe_and_usable_via_dyn() {
        let mut effect: Box<dyn Effect> = Box::new(Passthrough::default());
        effect.prepare(48_000.0, 64);
        let mut buffer = [0.25_f32; 8];
        effect.process(&mut buffer);
        assert_eq!(effect.name(), "Passthrough");
        assert_eq!(effect.latency_samples(), 0);
        assert!(!effect.is_bypassed());
        assert_eq!(buffer, [0.25_f32; 8]);
    }

    #[test]
    fn process_is_allocation_free() {
        let mut effect = Passthrough::default();
        effect.prepare(48_000.0, 256);
        let mut buffer = [0.1_f32; 256];
        lindelion_test_allocator::assert_no_allocations("passthrough process", || {
            effect.process(&mut buffer);
        });
    }
}
