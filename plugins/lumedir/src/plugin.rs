use std::sync::Arc;

use lindelion_plugin_shell::{
    AudioPlugin, ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};
use lindelion_speech_signals::SignalSnapshot;

use crate::delivery::DeliverySnapshot;
use crate::{DeliveryReader, DeliveryWorker, SharedConfig, config_io};

/// Lúmedir is a passthrough Speech-Coach effect: audio is mirrored to the output bit-exact at zero
/// declared latency, while a mono mix is fed to the off-thread delivery worker, which assembles the
/// delivery snapshot (rate/WPM, dynamism, pauses, clarity) the editor reads.
pub const DESCRIPTOR: PluginDescriptor = PluginDescriptor::effect("Lumedir", *b"lindelion_lumedr");

#[derive(Default)]
pub struct Lumedir {
    setup: ProcessSetup,
    /// Live coaching config (syllables-per-word factor + target bands), shared with the editor
    /// (writer) and the delivery worker (reads the factor). Persisted via [`crate::config_io`].
    shared_config: Arc<SharedConfig>,
    /// Off-thread delivery worker, (re)built on `reset` at the host sample rate. The audio thread
    /// only feeds it (allocation-free); all analysis and aggregation happen on its worker thread.
    worker: Option<DeliveryWorker>,
    /// Pre-sized mono-mix buffer for the worker feed, so `process` never allocates.
    mono_scratch: Vec<f32>,
}

impl AudioPlugin for Lumedir {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        &[]
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        // Spawn the worker and size the feed buffer here (not on the audio thread). The worker reads
        // the live factor from the shared config, so it survives factor edits without a respawn.
        self.worker = Some(DeliveryWorker::new(
            setup.sample_rate as f32,
            Arc::clone(&self.shared_config),
        ));
        self.mono_scratch.clear();
        self.mono_scratch.resize(setup.max_block_size, 0.0);
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        let ProcessContext { input, buffer, .. } = context;
        passthrough_channel(input.left, buffer.left);
        passthrough_channel(input.right, buffer.right);

        // Feed a mono mix of the input to the worker. `write_mono_to` fills the pre-sized scratch
        // in place and `push` is allocation-free in the off-thread build (ADR-0001).
        let fed = input.write_mono_to(&mut self.mono_scratch);
        if let Some(worker) = &self.worker {
            worker.push(&self.mono_scratch[..fed]);
        }
    }

    fn state(&self) -> PluginState {
        // Serialize the live coaching config (factor + target bands) the editor has been editing.
        config_io::to_plugin_state(&self.shared_config.to_config())
            .unwrap_or_else(|_| PluginState::empty(config_io::FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(config) = config_io::from_plugin_state(state) {
            self.shared_config.load_from_config(&config);
        }
    }
}

impl Lumedir {
    /// The latest delivery snapshot from the worker (the read path the editor consumes in M4).
    /// Returns the default snapshot before `reset` or before the worker has produced anything.
    pub fn latest_delivery(&self) -> DeliverySnapshot {
        self.worker
            .as_ref()
            .map(DeliveryWorker::latest_delivery)
            .unwrap_or_default()
    }

    /// The latest underlying analysis snapshot from the worker (continuity with M0).
    pub fn latest_snapshot(&self) -> SignalSnapshot {
        self.worker
            .as_ref()
            .map(DeliveryWorker::latest_snapshot)
            .unwrap_or_default()
    }

    /// A cloneable read handle onto the worker's snapshots, for the editor to poll off the audio
    /// thread. `None` before `reset` has spawned the worker.
    pub fn delivery_reader(&self) -> Option<DeliveryReader> {
        self.worker.as_ref().map(DeliveryWorker::reader)
    }

    /// The shared coaching config (factor + target bands), for the editor to read and edit.
    pub fn shared_config(&self) -> Arc<SharedConfig> {
        Arc::clone(&self.shared_config)
    }
}

/// Copy one input channel to its output channel sample-for-sample (bit-exact). When the input
/// channel is absent, the output channel is silenced. No averaging, no sanitization, no allocation.
fn passthrough_channel(source: Option<&[f32]>, destination: &mut [f32]) {
    match source {
        Some(source) => {
            let copied = source.len().min(destination.len());
            destination[..copied].copy_from_slice(&source[..copied]);
            for sample in &mut destination[copied..] {
                *sample = 0.0;
            }
        }
        None => {
            for sample in destination.iter_mut() {
                *sample = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioBuffer, AudioInputBuffer};

    #[test]
    fn process_is_bit_exact_stereo_passthrough_without_allocating() {
        let left_in = [0.0_f32, 0.5, -0.25, 1.0, f32::MIN_POSITIVE];
        let right_in = [-1.0_f32, 0.123, 0.0, -0.5, 0.999];
        let mut left_out = [7.0_f32; 5];
        let mut right_out = [7.0_f32; 5];

        let mut plugin = Lumedir::default();
        plugin.reset(ProcessSetup::default());

        // The asserted region runs the full `process` — passthrough *and* the worker feed
        // (`write_mono_to` into the pre-sized scratch + the off-thread `push`) — proving the feed
        // is allocation-free on the audio thread (the counting allocator is thread-local, so the
        // worker thread's own allocations are not counted here).
        crate::assert_no_allocations("lumedir passthrough + feed", || {
            let context = ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut left_out,
                    right: &mut right_out,
                },
                &[],
            )
            .with_input(AudioInputBuffer::stereo(&left_in, &right_in));
            plugin.process(context);
        });

        assert_eq!(left_out, left_in);
        assert_eq!(right_out, right_in);
        // The worker read path exists and is callable off the audio thread.
        let _snapshot = plugin.latest_snapshot();
    }

    #[test]
    fn latest_delivery_is_default_before_processing() {
        // The delivery read path the editor consumes (M4) is reachable off the audio thread and
        // starts at the default snapshot.
        let mut plugin = Lumedir::default();
        plugin.reset(ProcessSetup::default());
        assert_eq!(
            plugin.latest_delivery(),
            crate::delivery::DeliverySnapshot::default()
        );
    }

    #[test]
    fn state_round_trips_the_coaching_config() {
        use crate::config::{LumedirConfig, TargetBands};

        let edited = LumedirConfig {
            syllables_per_word: 1.9,
            bands: TargetBands {
                wpm_max: 175.0,
                clarity_min: 0.7,
                ..TargetBands::default()
            },
        };
        // Load the edited config, then serialize via `state()` and confirm it round-trips.
        let mut plugin = Lumedir::default();
        plugin.load_state(crate::config_io::to_plugin_state(&edited).unwrap());
        let decoded = crate::config_io::from_plugin_state(plugin.state()).unwrap();
        assert_eq!(decoded, edited);
    }
}
