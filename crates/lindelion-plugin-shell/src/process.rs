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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioInputBuffer<'a> {
    pub left: Option<&'a [f32]>,
    pub right: Option<&'a [f32]>,
}

impl<'a> AudioInputBuffer<'a> {
    pub const fn empty() -> Self {
        Self {
            left: None,
            right: None,
        }
    }

    pub const fn mono(input: &'a [f32]) -> Self {
        Self {
            left: Some(input),
            right: None,
        }
    }

    pub const fn stereo(left: &'a [f32], right: &'a [f32]) -> Self {
        Self {
            left: Some(left),
            right: Some(right),
        }
    }

    pub fn len(&self) -> usize {
        match (self.left, self.right) {
            (Some(left), Some(right)) => left.len().min(right.len()),
            (Some(left), None) => left.len(),
            (None, Some(right)) => right.len(),
            (None, None) => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn mono_sample(&self, index: usize) -> f32 {
        match (
            self.left.and_then(|left| left.get(index)),
            self.right.and_then(|right| right.get(index)),
        ) {
            (Some(left), Some(right)) => sanitize_input_sample((*left + *right) * 0.5),
            (Some(left), None) => sanitize_input_sample(*left),
            (None, Some(right)) => sanitize_input_sample(*right),
            (None, None) => 0.0,
        }
    }

    pub fn write_mono_to(&self, target: &mut [f32]) -> usize {
        let len = self.len().min(target.len());
        for (index, sample) in target.iter_mut().take(len).enumerate() {
            *sample = self.mono_sample(index);
        }
        len
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: u16,
    pub denominator: u16,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    pub fn new(numerator: i32, denominator: i32) -> Self {
        Self {
            numerator: numerator.clamp(1, u16::MAX as i32) as u16,
            denominator: denominator.clamp(1, u16::MAX as i32) as u16,
        }
    }

    pub fn beats_per_bar(self) -> f64 {
        self.numerator as f64 * 4.0 / self.denominator.max(1) as f64
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransportContext {
    pub playing: bool,
    pub recording: bool,
    pub sample_position: Option<i64>,
    pub project_quarter_note: Option<f64>,
    pub bar_position_quarter_note: Option<f64>,
    pub cycle_active: bool,
    pub cycle_start_quarter_note: Option<f64>,
    pub cycle_end_quarter_note: Option<f64>,
    pub tempo_bpm: Option<f64>,
    pub time_signature: Option<TimeSignature>,
}

impl TransportContext {
    pub fn tempo_bpm_or(self, fallback: f64) -> f64 {
        sanitize_positive_f64(self.tempo_bpm, fallback)
    }

    pub fn time_signature_or_default(self) -> TimeSignature {
        self.time_signature.unwrap_or_default()
    }

    pub fn beats_per_bar(self) -> f64 {
        self.time_signature_or_default().beats_per_bar()
    }
}

#[derive(Debug)]
pub struct ProcessContext<'a> {
    pub setup: ProcessSetup,
    pub input: AudioInputBuffer<'a>,
    pub buffer: AudioBuffer<'a>,
    pub events: &'a [MidiEvent],
    pub transport: TransportContext,
}

impl<'a> ProcessContext<'a> {
    pub const fn new(
        setup: ProcessSetup,
        buffer: AudioBuffer<'a>,
        events: &'a [MidiEvent],
    ) -> Self {
        Self {
            setup,
            input: AudioInputBuffer::empty(),
            buffer,
            events,
            transport: TransportContext {
                playing: false,
                recording: false,
                sample_position: None,
                project_quarter_note: None,
                bar_position_quarter_note: None,
                cycle_active: false,
                cycle_start_quarter_note: None,
                cycle_end_quarter_note: None,
                tempo_bpm: None,
                time_signature: None,
            },
        }
    }

    pub const fn with_input(mut self, input: AudioInputBuffer<'a>) -> Self {
        self.input = input;
        self
    }

    pub const fn with_transport(mut self, transport: TransportContext) -> Self {
        self.transport = transport;
        self
    }
}

pub trait AudioPlugin {
    fn descriptor(&self) -> &'static PluginDescriptor;

    fn parameters(&self) -> &'static [ParameterInfo];

    fn reset(&mut self, setup: ProcessSetup);

    fn process(&mut self, context: ProcessContext<'_>);

    fn state(&self) -> PluginState;

    fn load_state(&mut self, state: PluginState);
}

fn sanitize_input_sample(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

fn sanitize_positive_f64(value: Option<f64>, fallback: f64) -> f64 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_input_buffer_sums_stereo_to_sanitized_mono() {
        let left = [0.0, 0.5, f32::NAN];
        let right = [1.0, -0.5, 0.25];
        let input = AudioInputBuffer::stereo(&left, &right);
        let mut mono = [0.0; 3];

        let written = input.write_mono_to(&mut mono);

        assert_eq!(written, 3);
        assert_eq!(mono, [0.5, 0.0, 0.0]);
    }

    #[test]
    fn transport_context_provides_musical_fallbacks() {
        let transport = TransportContext {
            tempo_bpm: Some(f64::NAN),
            time_signature: Some(TimeSignature::new(7, 8)),
            ..TransportContext::default()
        };

        assert_eq!(transport.tempo_bpm_or(120.0), 120.0);
        assert_eq!(transport.beats_per_bar(), 3.5);
    }
}
