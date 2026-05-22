use crate::math;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnePoleLowpass {
    z1: f32,
    coefficient: f32,
}

impl Default for OnePoleLowpass {
    fn default() -> Self {
        Self {
            z1: 0.0,
            coefficient: 1.0,
        }
    }
}

impl OnePoleLowpass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut filter = Self::default();
        filter.set_cutoff(cutoff_hz, sample_rate);
        filter
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let cutoff_hz = cutoff_hz.clamp(20.0, sample_rate * 0.45);
        let x = (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate).exp();
        self.coefficient = 1.0 - x;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.z1 += self.coefficient * (input - self.z1);
        self.z1 = math::snap_to_zero(self.z1);
        self.z1
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefficients {
    pub fn lowpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        rbj(sample_rate, cutoff_hz, q, BiquadKind::Lowpass)
    }

    pub fn highpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        rbj(sample_rate, cutoff_hz, q, BiquadKind::Highpass)
    }

    pub fn bandpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        rbj(sample_rate, cutoff_hz, q, BiquadKind::Bandpass)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BiquadKind {
    Lowpass,
    Highpass,
    Bandpass,
}

fn rbj(sample_rate: f32, cutoff_hz: f32, q: f32, kind: BiquadKind) -> BiquadCoefficients {
    let cutoff_hz = cutoff_hz.clamp(20.0, sample_rate * 0.45);
    let q = q.max(0.05);
    let omega = std::f32::consts::TAU * cutoff_hz / sample_rate;
    let sin = omega.sin();
    let cos = omega.cos();
    let alpha = sin / (2.0 * q);

    let (b0, b1, b2) = match kind {
        BiquadKind::Lowpass => ((1.0 - cos) * 0.5, 1.0 - cos, (1.0 - cos) * 0.5),
        BiquadKind::Highpass => ((1.0 + cos) * 0.5, -(1.0 + cos), (1.0 + cos) * 0.5),
        BiquadKind::Bandpass => (alpha, 0.0, -alpha),
    };

    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos;
    let a2 = 1.0 - alpha;

    BiquadCoefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    coefficients: BiquadCoefficients,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            coefficients,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn set_coefficients(&mut self, coefficients: BiquadCoefficients) {
        self.coefficients = coefficients;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let c = self.coefficients;
        let output =
            c.b0 * input + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = math::snap_to_zero(output);
        self.y1
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvfMode {
    Lowpass,
    Bandpass,
    Highpass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Svf {
    sample_rate: f32,
    cutoff_hz: f32,
    resonance: f32,
    mode: SvfMode,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            mode: SvfMode::Lowpass,
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }

    pub fn set_params(&mut self, cutoff_hz: f32, resonance: f32, mode: SvfMode) {
        self.cutoff_hz = cutoff_hz;
        self.resonance = resonance.clamp(0.0, 0.999);
        self.mode = mode;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let cutoff_hz = self.cutoff_hz.clamp(20.0, self.sample_rate * 0.45);
        let g = (std::f32::consts::PI * cutoff_hz / self.sample_rate).tan();
        let damping = 2.0 - 1.9 * self.resonance;
        let h = 1.0 / (1.0 + damping * g + g * g);

        let high = (input - damping * self.ic1eq - self.ic2eq) * h;
        let band = g * high + self.ic1eq;
        let low = g * band + self.ic2eq;

        self.ic1eq = math::snap_to_zero(g * high + band);
        self.ic2eq = math::snap_to_zero(g * band + low);

        match self.mode {
            SvfMode::Lowpass => low,
            SvfMode::Bandpass => band,
            SvfMode::Highpass => high,
        }
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{assert_all_finite, dft_magnitude_at, peak_abs};

    #[test]
    fn one_pole_step_response_moves_toward_input() {
        let mut filter = OnePoleLowpass::new(1_000.0, 48_000.0);
        let mut last = 0.0;
        for _ in 0..128 {
            let next = filter.process(1.0);
            assert!(next >= last);
            last = next;
        }

        assert!(last > 0.9);
        assert!(last < 1.0);
    }

    #[test]
    fn lowpass_reduces_high_frequency_more_than_low_frequency() {
        let sample_rate = 48_000.0;
        let mut filter = Biquad::new(BiquadCoefficients::lowpass(sample_rate, 1_000.0, 0.707));
        let mut output = Vec::new();

        for index in 0..8192 {
            let low = (std::f32::consts::TAU * 250.0 * index as f32 / sample_rate).sin();
            let high = (std::f32::consts::TAU * 8_000.0 * index as f32 / sample_rate).sin();
            output.push(filter.process(low + high));
        }

        assert!(
            dft_magnitude_at(&output[1024..], sample_rate, 250.0)
                > dft_magnitude_at(&output[1024..], sample_rate, 8_000.0) * 10.0
        );
    }

    #[test]
    fn svf_remains_finite_under_parameter_sweep() {
        let mut svf = Svf::new(48_000.0);
        let mut output = Vec::new();

        for index in 0..10_000 {
            let normalized = index as f32 / 9_999.0;
            svf.set_params(
                20.0 + normalized * 20_000.0,
                normalized,
                if index % 3 == 0 {
                    SvfMode::Lowpass
                } else if index % 3 == 1 {
                    SvfMode::Bandpass
                } else {
                    SvfMode::Highpass
                },
            );
            output.push(svf.process(if index == 0 { 1.0 } else { 0.0 }));
        }

        assert_all_finite(&output);
        assert!(peak_abs(&output) < 4.0);
    }
}
