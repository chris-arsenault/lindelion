use crate::{MidiEvent, ParameterInfo, PluginDescriptor, PluginState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessMode {
    Realtime,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessSetup {
    pub sample_rate: f64,
    pub max_block_size: usize,
    pub mode: ProcessMode,
}

impl Default for ProcessSetup {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 1024,
            mode: ProcessMode::Realtime,
        }
    }
}

#[derive(Debug)]
pub struct AudioBuffer<'a> {
    pub left: &'a mut [f32],
    pub right: &'a mut [f32],
}

impl AudioBuffer<'_> {
    pub fn len(&self) -> usize {
        self.left.len().min(self.right.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        for sample in self.left.iter_mut().chain(self.right.iter_mut()) {
            *sample = 0.0;
        }
    }
}

#[derive(Debug)]
pub struct ProcessContext<'a> {
    pub setup: ProcessSetup,
    pub buffer: AudioBuffer<'a>,
    pub events: &'a [MidiEvent],
}

pub trait AudioPlugin {
    fn descriptor(&self) -> &'static PluginDescriptor;

    fn parameters(&self) -> &'static [ParameterInfo];

    fn reset(&mut self, setup: ProcessSetup);

    fn process(&mut self, context: ProcessContext<'_>);

    fn state(&self) -> PluginState;

    fn load_state(&mut self, state: PluginState);
}
