use std::sync::Arc;

use lindelion_plugin_shell::{
    AudioPlugin, ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};

use crate::analysis::CenedrilAnalysis;
use crate::settings::{self, SettingsCell};

/// Cenedril is a passthrough Visualizer effect: audio is mirrored to the output bit-exact at zero
/// declared latency, while a parallel analysis tap feeds the editor (the tap never touches the
/// output). The editor itself is a later milestone.
pub const DESCRIPTOR: PluginDescriptor = PluginDescriptor::effect("Cenedril", *b"lindelion_cenedr");

#[derive(Default)]
pub struct Cenedril {
    setup: ProcessSetup,
    analysis: CenedrilAnalysis,
    settings: Arc<SettingsCell>,
}

impl AudioPlugin for Cenedril {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        &[]
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        self.analysis
            .reset(setup.sample_rate as f32, setup.max_block_size);
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        let ProcessContext { input, buffer, .. } = context;
        passthrough_channel(input.left, buffer.left);
        passthrough_channel(input.right, buffer.right);
        // Parallel analysis tap — reads `input` only, never the output (passthrough stays bit-exact).
        self.analysis.process(input);
    }

    fn state(&self) -> PluginState {
        settings::to_plugin_state(&self.settings.get())
            .unwrap_or_else(|_| PluginState::empty(settings::FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(loaded) = settings::from_plugin_state(state) {
            self.settings.set(loaded);
        }
    }
}

impl Cenedril {
    /// Start the off-thread analysis worker. Called from the VST3 host processing path (not the
    /// allocation-free/thread-free audio-thread core tests), since it spawns a thread.
    pub fn start_analysis_worker(&mut self, sample_rate: f32) {
        self.analysis.start_worker(sample_rate);
    }

    /// The latest heavy-analysis snapshot for the editor (M5).
    pub fn latest_snapshot(&self) -> lindelion_speech_signals::SignalSnapshot {
        self.analysis.latest_snapshot()
    }

    /// A clone of the audio→editor frame ring (shared `Arc`), for the editor to drain.
    pub fn frame_ring(&self) -> std::sync::Arc<crate::analysis::FrameRing> {
        self.analysis.frame_ring().clone()
    }

    /// A clone of the level/loudness meter cell (shared `Arc`), for the editor to read.
    pub fn meter(&self) -> std::sync::Arc<crate::analysis::MeterCell> {
        self.analysis.meter().clone()
    }

    /// A clone of the analysis-signal cell (shared `Arc`), for the editor to read.
    pub fn analysis(&self) -> std::sync::Arc<crate::analysis::SignalCell> {
        self.analysis.analysis().clone()
    }

    /// A clone of the editor-settings cell (shared `Arc`), for the editor to read/write.
    pub fn settings(&self) -> Arc<SettingsCell> {
        self.settings.clone()
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

        let mut plugin = Cenedril::default();
        plugin.reset(ProcessSetup::default());

        crate::assert_no_allocations("cenedril passthrough", || {
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
    }

    #[test]
    fn editor_settings_round_trip_across_state_reload() {
        use crate::settings::CenedrilEditorSettings;

        let a = Cenedril::default();
        assert_eq!(a.settings().get(), CenedrilEditorSettings::default());
        a.settings().set(CenedrilEditorSettings {
            active_view: 1,
            freq_scale: 1,
            color_map: 2,
            db_floor: -80.0,
            db_ceil: -6.0,
        });
        let state = a.state();

        let mut b = Cenedril::default();
        b.load_state(state);
        assert_eq!(b.settings().get(), a.settings().get());
    }
}
