#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopePhase {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdsrState {
    phase: EnvelopePhase,
    value: f32,
    release_start: f32,
}

impl Default for AdsrState {
    fn default() -> Self {
        Self {
            phase: EnvelopePhase::Idle,
            value: 0.0,
            release_start: 0.0,
        }
    }
}

impl AdsrState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn note_on(&mut self) {
        self.phase = if self.value >= 1.0 {
            EnvelopePhase::Decay
        } else {
            EnvelopePhase::Attack
        };
    }

    pub fn note_off(&mut self) {
        self.release_start = self.value;
        self.phase = EnvelopePhase::Release;
    }

    pub const fn phase(&self) -> EnvelopePhase {
        self.phase
    }

    pub const fn value(&self) -> f32 {
        self.value
    }

    pub fn next_sample(&mut self, adsr: Adsr, sample_rate: f32) -> f32 {
        let sustain = adsr.sustain.clamp(0.0, 1.0);

        match self.phase {
            EnvelopePhase::Idle => {
                self.value = 0.0;
            }
            EnvelopePhase::Attack => {
                self.value += step_for_ms(adsr.attack_ms, sample_rate);
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.phase = EnvelopePhase::Decay;
                }
            }
            EnvelopePhase::Decay => {
                self.value -= (1.0 - sustain) * step_for_ms(adsr.decay_ms, sample_rate);
                if self.value <= sustain + f32::EPSILON {
                    self.value = sustain;
                    self.phase = EnvelopePhase::Sustain;
                }
            }
            EnvelopePhase::Sustain => {
                self.value = sustain;
            }
            EnvelopePhase::Release => {
                self.value -= self.release_start * step_for_ms(adsr.release_ms, sample_rate);
                if self.value <= 0.0 {
                    self.value = 0.0;
                    self.phase = EnvelopePhase::Idle;
                }
            }
        }

        self.value
    }
}

fn step_for_ms(ms: f32, sample_rate: f32) -> f32 {
    if ms <= 0.0 {
        1.0
    } else {
        1.0 / (ms * 0.001 * sample_rate).max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_reaches_one_at_requested_time() {
        let adsr = Adsr {
            attack_ms: 10.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 10.0,
        };
        let sample_rate = 1_000.0;
        let mut state = AdsrState::default();
        state.note_on();

        for _ in 0..10 {
            state.next_sample(adsr, sample_rate);
        }

        assert_eq!(state.value(), 1.0);
        assert_eq!(state.phase(), EnvelopePhase::Decay);
    }

    #[test]
    fn decay_lands_on_sustain() {
        let adsr = Adsr {
            attack_ms: 0.0,
            decay_ms: 10.0,
            sustain: 0.25,
            release_ms: 10.0,
        };
        let sample_rate = 1_000.0;
        let mut state = AdsrState::default();
        state.note_on();
        state.next_sample(adsr, sample_rate);

        for _ in 0..10 {
            state.next_sample(adsr, sample_rate);
        }

        assert!((state.value() - 0.25).abs() < 0.000_01);
        assert_eq!(state.phase(), EnvelopePhase::Sustain);
    }

    #[test]
    fn release_reaches_idle() {
        let adsr = Adsr {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 10.0,
        };
        let sample_rate = 1_000.0;
        let mut state = AdsrState::default();
        state.note_on();
        state.next_sample(adsr, sample_rate);
        state.note_off();

        for _ in 0..10 {
            state.next_sample(adsr, sample_rate);
        }

        assert_eq!(state.value(), 0.0);
        assert_eq!(state.phase(), EnvelopePhase::Idle);
    }
}
