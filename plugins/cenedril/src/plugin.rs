use lindelion_plugin_shell::{
    AudioPlugin, ParameterInfo, PluginDescriptor, PluginState, ProcessContext, ProcessSetup,
};

/// Cenedril is a passthrough Visualizer effect: audio is mirrored to the output bit-exact at zero
/// declared latency. (Analysis tap and editor are later milestones.)
pub const DESCRIPTOR: PluginDescriptor = PluginDescriptor::effect("Cenedril", *b"lindelion_cenedr");

const STATE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Default)]
pub struct Cenedril {
    setup: ProcessSetup,
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
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        let ProcessContext { input, buffer, .. } = context;
        passthrough_channel(input.left, buffer.left);
        passthrough_channel(input.right, buffer.right);
    }

    fn state(&self) -> PluginState {
        PluginState::empty(STATE_FORMAT_VERSION)
    }

    fn load_state(&mut self, _state: PluginState) {
        // No persisted fields in M0; editor settings arrive in M6.
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
}
