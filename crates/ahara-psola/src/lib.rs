#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchEpoch {
    pub position_samples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchEstimate {
    pub position_samples: usize,
    pub fundamental_hz: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PitchAnalysis {
    pub sample_rate: u32,
    pub epochs: Vec<PitchEpoch>,
    pub estimates: Vec<PitchEstimate>,
}

impl PitchAnalysis {
    pub fn empty(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            epochs: Vec::new(),
            estimates: Vec::new(),
        }
    }

    pub fn median_fundamental_hz(&self) -> Option<f32> {
        let mut values = self
            .estimates
            .iter()
            .filter(|estimate| estimate.confidence > 0.5 && estimate.fundamental_hz > 0.0)
            .map(|estimate| estimate.fundamental_hz)
            .collect::<Vec<_>>();

        if values.is_empty() {
            return None;
        }

        values.sort_by(|a, b| a.total_cmp(b));
        Some(values[values.len() / 2])
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchShift {
    pub semitones: i32,
    pub cents: f32,
}

impl PitchShift {
    pub fn ratio(self) -> f32 {
        2.0_f32.powf((self.semitones as f32 + self.cents / 100.0) / 12.0)
    }
}

#[derive(Debug, Default)]
pub struct PsolaEngine;

impl PsolaEngine {
    pub fn process_placeholder(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        _analysis: &PitchAnalysis,
        _shift: PitchShift,
    ) {
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);
        output[len..].fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octave_shift_ratio_is_two() {
        let shift = PitchShift {
            semitones: 12,
            cents: 0.0,
        };
        assert!((shift.ratio() - 2.0).abs() < 0.000_001);
    }
}
