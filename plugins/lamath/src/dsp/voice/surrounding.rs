//! Effort/energy-scaled surrounding effects (M10) — the "surrounding" link of the
//! dynamic-response chain (ADR-0014).
//!
//! This is a **per-voice, host-rate** stage that sits between the resonator output and
//! the output stage (the resonator output is already decimated out of the engine's 2x
//! loop, so no oversampling). It reads the M2 effort/energy bus and adds the
//! per-voice surrounding effects:
//!
//! - **Radiation brightening** (M10 step 5): an energy-scaled high-shelf, so a more
//!   energetically-sounding note radiates brighter.
//! - **Mechanical noise** (M10 step 6): an effort-scaled pick/breath attack-noise burst.
//!
//! The shared, **global** sympathetic-resonance bank is not here — it is excited by the
//! whole mix and lives at the engine level (M10 step 7).
//!
//! At the defeated default (every depth `0`) the stage is an identity pass-through, so a
//! default patch renders exactly as before M10.

use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use crate::SurroundingConfig;

/// Radiation-brightening high-shelf corner; the energy-scaled boost lifts the band
/// above it, the "more sound radiates brighter" character.
const RADIATION_SHELF_CUTOFF_HZ: f32 = 3_000.0;
/// High-shelf boost at full depth and full energy (dB). Bounded so the brightening
/// can never run away.
const RADIATION_MAX_BOOST_DB: f32 = 12.0;
/// Measured-energy (follower) value that maps to the top of the brightening curve;
/// `energy/REF` clamped to `[0, 1]` so soft notes stay flat and loud notes brighten.
/// M11 P8: calibrated to the measured per-voice energy bus (a full-velocity note peaks
/// near RMS 0.010) so loud playing reaches most of the +12 dB radiation lift; the old
/// 0.2 left a loud note at ≈5% of the curve (no audible radiation brightening).
const RADIATION_ENERGY_REF: f32 = 0.012;

/// Filter Q for the mechanical-noise shaping filters.
const NOISE_FILTER_Q: f32 = 0.7;
/// Pick-click high-pass corner: a bright, short attack transient.
const PICK_HIGHPASS_HZ: f32 = 2_000.0;
/// Pick-click decay time constant (ms): a very fast mechanical click.
const PICK_DECAY_MS: f32 = 5.0;
/// Breath-rush band-pass centre: a band-limited noise rush.
const BREATH_BANDPASS_HZ: f32 = 2_500.0;
/// Breath-rush decay time constant (ms): a slower band-limited rush than the click.
const BREATH_DECAY_MS: f32 = 30.0;
/// Output gain of the combined (pick + breath) mechanical-noise burst at full effort
/// and full depth, before the `effort × depth` scaling.
const MECHANICAL_NOISE_GAIN: f32 = 0.5;
/// Below this envelope level the burst is treated as finished (skip the noise work).
const NOISE_ENVELOPE_FLOOR: f32 = 1.0e-4;
/// Fixed non-zero seed for the deterministic xorshift PRNG (seeded => unit tests are
/// reproducible; not wall-clock / `Math.random`, so it respects the test hygiene rule).
const NOISE_PRNG_SEED: u32 = 0x9E37_79B9;

#[derive(Debug)]
pub(super) struct SurroundingStage {
    config: SurroundingConfig,
    sample_rate: f32,
    // Radiation brightening (M10 step 5): an energy-scaled high-shelf. Built flat at
    // construction; the boost is re-set at control rate from depth × measured energy.
    radiation_shelf: Biquad,
    // Mechanical noise (M10 step 6): a seeded xorshift PRNG driving a bright pick-click
    // (high-passed, fast decay) and a band-limited breath rush (band-passed, slower
    // decay), both armed at note-on and scaled by player effort.
    prng_state: u32,
    pick_envelope: f32,
    breath_envelope: f32,
    pick_decay: f32,
    breath_decay: f32,
    pick_filter: Biquad,
    breath_filter: Biquad,
}

impl SurroundingStage {
    pub(super) fn new(sample_rate: f32) -> Self {
        let safe_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            config: SurroundingConfig::default(),
            sample_rate: safe_rate,
            radiation_shelf: Biquad::new(BiquadCoefficients::identity()),
            prng_state: NOISE_PRNG_SEED,
            pick_envelope: 0.0,
            breath_envelope: 0.0,
            pick_decay: envelope_decay(PICK_DECAY_MS, safe_rate),
            breath_decay: envelope_decay(BREATH_DECAY_MS, safe_rate),
            pick_filter: Biquad::new(BiquadCoefficients::highpass(
                safe_rate,
                PICK_HIGHPASS_HZ,
                NOISE_FILTER_Q,
            )),
            breath_filter: Biquad::new(BiquadCoefficients::bandpass(
                safe_rate,
                BREATH_BANDPASS_HZ,
                NOISE_FILTER_Q,
            )),
        }
    }

    /// Select the surrounding-effects config from the patch (M10). Set at note-on,
    /// mirroring the driver/contact config setters.
    pub(super) fn set_config(&mut self, config: SurroundingConfig) {
        self.config = config;
    }

    /// Arm the per-note surrounding effects (the mechanical-noise burst) at note-on.
    pub(super) fn trigger(&mut self) {
        self.pick_envelope = 1.0;
        self.breath_envelope = 1.0;
        self.pick_filter.reset();
        self.breath_filter.reset();
    }

    pub(super) fn reset(&mut self) {
        self.radiation_shelf.reset();
        self.pick_envelope = 0.0;
        self.breath_envelope = 0.0;
        self.pick_filter.reset();
        self.breath_filter.reset();
        self.prng_state = NOISE_PRNG_SEED;
    }

    /// Process one host-rate resonator-output sample with the M2 bus (`effort` is
    /// player effort, `energy` is the measured resonator energy). At the defeated
    /// default this returns the sample unchanged. Mechanical noise is added first (it
    /// is part of the radiated attack), then radiation brightening colours the sum.
    pub(super) fn process(&mut self, sample: f32, effort: f32, energy: f32) -> f32 {
        let noised = self.apply_mechanical_noise(sample, effort);
        self.apply_radiation_brightening(noised, energy)
    }

    /// Effort-scaled mechanical noise: a bright pick click + a band-limited breath
    /// rush, armed at note-on and decaying over the attack. At depth 0 (or once the
    /// burst has decayed) the sample passes through unchanged (defeatable identity).
    fn apply_mechanical_noise(&mut self, sample: f32, effort: f32) -> f32 {
        let depth = math::finite_clamp(self.config.mechanical_noise, 0.0, 1.0, 0.0);
        if depth <= 0.0 {
            return sample;
        }
        let pick_envelope = self.pick_envelope;
        let breath_envelope = self.breath_envelope;
        self.pick_envelope *= self.pick_decay;
        self.breath_envelope *= self.breath_decay;
        if pick_envelope < NOISE_ENVELOPE_FLOOR && breath_envelope < NOISE_ENVELOPE_FLOOR {
            return sample;
        }
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let white = self.next_white();
        let pick = self.pick_filter.process(white) * pick_envelope;
        let breath = self.breath_filter.process(white) * breath_envelope;
        let noise = (pick + breath) * effort * depth * MECHANICAL_NOISE_GAIN;
        math::snap_to_zero(sample + noise)
    }

    /// One white-noise sample in `[-1, 1]` from the seeded xorshift32 PRNG.
    fn next_white(&mut self) -> f32 {
        let mut x = self.prng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.prng_state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Energy-scaled high-shelf: a more energetically-sounding note radiates brighter.
    /// At depth 0 (or zero energy) the shelf gain is 0 dB and the sample passes through
    /// unchanged (defeatable identity); the boost is bounded by `RADIATION_MAX_BOOST_DB`.
    fn apply_radiation_brightening(&mut self, sample: f32, energy: f32) -> f32 {
        let depth = math::finite_clamp(self.config.radiation_brightness, 0.0, 1.0, 0.0);
        if depth <= 0.0 {
            return sample;
        }
        let energy = math::finite_clamp(energy / RADIATION_ENERGY_REF, 0.0, 1.0, 0.0);
        let gain_db = depth * energy * RADIATION_MAX_BOOST_DB;
        self.radiation_shelf
            .set_coefficients(BiquadCoefficients::high_shelf(
                self.sample_rate,
                RADIATION_SHELF_CUTOFF_HZ,
                gain_db,
            ));
        math::snap_to_zero(self.radiation_shelf.process(sample))
    }
}

/// Per-sample multiplicative decay for an exponential envelope with the given time
/// constant (ms) at the sample rate: `exp(-1 / (τ · sr))`.
fn envelope_decay(time_constant_ms: f32, sample_rate: f32) -> f32 {
    let samples = (time_constant_ms * 0.001 * sample_rate).max(1.0);
    (-1.0 / samples).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;
    use lindelion_dsp_utils::analysis::{audio_window_metrics, rms};

    #[test]
    fn radiation_brightening_rises_with_energy_and_is_defeatable() {
        // Feed an impulse (flat spectrum) through the stage; the energy-scaled high-
        // shelf lifts the high band, so a high-energy note is brighter (higher spectral
        // centroid) than a low-energy one. At depth 0 the shelf is inert — energy does
        // not change the output (defeatable).
        let render = |depth: f32, energy: f32| {
            let mut stage = SurroundingStage::new(48_000.0);
            stage.set_config(SurroundingConfig {
                radiation_brightness: depth,
                ..SurroundingConfig::default()
            });
            (0..2_048)
                .map(|index| stage.process((index == 0) as u8 as f32, 0.0, energy))
                .collect::<Vec<_>>()
        };
        let soft = render(0.9, 0.0);
        let loud = render(0.9, 1.0);
        let soft_centroid = audio_window_metrics(&soft, 48_000.0)
            .spectral_centroid_hz
            .unwrap();
        let loud_centroid = audio_window_metrics(&loud, 48_000.0)
            .spectral_centroid_hz
            .unwrap();
        assert!(
            loud_centroid > soft_centroid * 1.1,
            "brightening should rise with energy: soft={soft_centroid} loud={loud_centroid}"
        );

        // Defeatable: at depth 0 the output is independent of energy.
        assert_eq!(
            render(0.0, 0.0),
            render(0.0, 1.0),
            "depth-0 brightening leaked energy"
        );
    }

    #[test]
    fn surrounding_stage_process_does_not_allocate() {
        let mut stage = SurroundingStage::new(48_000.0);
        stage.set_config(SurroundingConfig {
            mechanical_noise: 0.6,
            radiation_brightness: 0.5,
            sympathetic: 0.0,
        });
        stage.trigger();
        assert_no_allocations("surrounding_stage_process", || {
            for index in 0..512 {
                let energy = 0.1 + 0.05 * (index as f32 * 0.05).sin();
                stage.process((index == 0) as u8 as f32, 0.7, energy);
            }
        });
    }

    #[test]
    fn mechanical_noise_scales_with_effort_and_is_defeatable() {
        // The mechanical-noise burst is armed at note-on; rendering into silence
        // isolates it. Its attack energy rises with player effort, and at depth 0 it
        // is silent (defeatable). Energy 0 keeps the brightening flat so the measured
        // signal is the noise alone.
        let render = |depth: f32, effort: f32| {
            let mut stage = SurroundingStage::new(48_000.0);
            stage.set_config(SurroundingConfig {
                mechanical_noise: depth,
                ..SurroundingConfig::default()
            });
            stage.trigger();
            (0..4_096)
                .map(|_| stage.process(0.0, effort, 0.0))
                .collect::<Vec<_>>()
        };
        let soft = render(0.9, 0.1);
        let hard = render(0.9, 1.0);
        let soft_rms = rms(&soft);
        let hard_rms = rms(&hard);
        assert!(soft_rms > 0.0, "mechanical noise should be audible");
        assert!(
            hard_rms > soft_rms * 1.5,
            "noise should rise with effort: soft={soft_rms} hard={hard_rms}"
        );

        // Defeatable: at depth 0 the noise burst is silent.
        assert_eq!(
            rms(&render(0.0, 1.0)),
            0.0,
            "depth-0 mechanical noise not silent"
        );
    }

    #[test]
    fn defeated_surrounding_stage_is_pass_through() {
        // At the defeated default (every depth 0), the stage returns its input
        // unchanged for any effort/energy, so a default patch renders as before M10.
        let mut stage = SurroundingStage::new(48_000.0);
        for &sample in &[-1.5_f32, -0.2, 0.0, 0.3, 1.2] {
            for &effort in &[0.0_f32, 0.5, 1.0] {
                for &energy in &[0.0_f32, 0.4, 1.0] {
                    assert_eq!(stage.process(sample, effort, energy), sample);
                }
            }
        }
    }
}
