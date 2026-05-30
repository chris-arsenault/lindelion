//! Room Tone: synthetic room-tone bed with speech ducking.
//!
//! Ports hot-mic's `Enhance-Room-Tone.md`. A low-level filtered-noise bed fills silence so gaps do
//! not sound dead; it is ducked by SpeechPresence (inline) so it disappears under speech. Reuses
//! dsp-utils filters + the inline presence signal.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::SpeechPresence;

pub const PARAM_LEVEL_DB: u32 = 0;

const TONE_LPF_HZ: f32 = 2_000.0;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_LEVEL_DB,
    name: "Level",
    min: -60.0,
    max: -20.0,
    default: -40.0,
    unit: "dB",
}];

/// Room-tone bed gain for a level and speech presence (ducked by presence).
pub fn bed_gain(level_linear: f32, presence: f32) -> f32 {
    level_linear * (1.0 - presence.clamp(0.0, 1.0))
}

/// Speech-ducked room-tone bed.
pub struct RoomTone {
    level_db: f32,
    bypassed: bool,
    presence: SpeechPresence,
    tone_lpf: Biquad,
    rng: u32,
}

impl RoomTone {
    pub fn new() -> Self {
        Self {
            level_db: -40.0,
            bypassed: false,
            presence: SpeechPresence::new(),
            tone_lpf: Biquad::new(BiquadCoefficients::identity()),
            rng: 0x5EED_1234,
        }
    }

    fn next_noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl Default for RoomTone {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for RoomTone {
    fn name(&self) -> &str {
        "Room Tone"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        if index == PARAM_LEVEL_DB {
            self.level_db = value.clamp(-60.0, -20.0);
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.presence.prepare(sample_rate);
        self.tone_lpf.set_coefficients(BiquadCoefficients::lowpass(
            sample_rate,
            TONE_LPF_HZ,
            0.707,
        ));
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let level = db_to_gain(self.level_db);
        for sample in buffer.iter_mut() {
            let dry = *sample;
            let presence = self.presence.process(dry);
            let noise = self.next_noise();
            let tone = self.tone_lpf.process(noise);
            *sample = dry + bed_gain(level, presence) * tone;
        }
    }

    fn latency_samples(&self) -> usize {
        0
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.presence.reset();
        self.tone_lpf.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        self.level_db.to_le_bytes().to_vec()
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.set_parameter(
                PARAM_LEVEL_DB,
                f32::from_le_bytes([state[0], state[1], state[2], state[3]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::rms;

    #[test]
    fn bed_ducked_by_presence() {
        let level = db_to_gain(-30.0);
        assert!(bed_gain(level, 0.0) > bed_gain(level, 1.0));
        assert_eq!(bed_gain(level, 1.0), 0.0);
    }

    #[test]
    fn bed_present_in_silence_ducked_in_speech() {
        let level = db_to_gain(-26.0);
        // Silence -> bed audible.
        let mut effect = RoomTone::new();
        effect.set_parameter(PARAM_LEVEL_DB, -26.0);
        effect.prepare(48_000.0, 1_024);
        let mut silence = vec![0.0_f32; 8_192];
        effect.process(&mut silence);
        assert!(
            rms(&silence) > level * 0.1,
            "no room tone in silence: {}",
            rms(&silence)
        );

        // Loud speech -> bed ducked away.
        let mut effect = RoomTone::new();
        effect.set_parameter(PARAM_LEVEL_DB, -26.0);
        effect.prepare(48_000.0, 1_024);
        let n = 16_384;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 300.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let tail = n / 2;
        let bed: Vec<f32> = buffer[tail..]
            .iter()
            .zip(&input[tail..])
            .map(|(o, i)| o - i)
            .collect();
        assert!(
            rms(&bed) < level * 0.3,
            "bed not ducked under speech: {}",
            rms(&bed)
        );
    }
}
