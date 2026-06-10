//! Shared note-lifecycle phrasing engine.
//!
//! Phrasing is the deliberate musical shape of a note — the sibling of
//! [`crate::variance`]'s involuntary drift, riding the same per-sample offset
//! rails onto an instrument's physical targets. The engine is causal: it never
//! knows when a note will end. Its contours are therefore duration-agnostic —
//! the attack develops asymptotically toward a plateau, the sustain breathes
//! around it, vibrato blooms in after the note settles, and the *release is
//! reactive*: a musical taper that begins at note-off and extends past it.
//! Anticipatory endings (a diminuendo into a known ending) are the player's
//! knowledge and arrive either through the host expression layer or simply by
//! releasing the note early and letting the taper carry the ending.
//!
//! The engine emits two instrument-agnostic signals per sample: `intensity`
//! (the living replacement for a static note-on effort) and
//! `pitch_lean_cents` (vibrato, later host pitch bend). Each instrument maps
//! these onto its own physical targets and tunes its own nominal depths; the
//! knob law ([`knob_law_scale`]) is shared: 0.5 of the control is the nominal
//! musical phrasing, 1.0 approaches the unmusical.

use crate::variance::{self, SmoothNoise};

/// Knob-law mapping for expressive-depth controls: 0..0.5 ramps linearly to
/// the nominal (scale 1.0 at 0.5); 0.5..1.0 continues toward
/// `extreme_multiplier` at full.
pub fn knob_law_scale(knob: f32, extreme_multiplier: f32) -> f32 {
    let knob = if knob.is_finite() {
        knob.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if knob <= 0.5 {
        2.0 * knob
    } else {
        1.0 + (2.0 * knob - 1.0) * (extreme_multiplier.max(1.0) - 1.0)
    }
}

/// Per-sample phrase parameters in physical units, already knob-scaled by the
/// instrument (each instrument owns its nominal depths and ceilings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhraseParams {
    /// Attack-development time constant toward the sustain plateau (seconds).
    pub attack_seconds: f32,
    /// Depth of the slow deliberate sustain swell (fraction of intensity).
    pub swell_depth: f32,
    /// Vibrato depth at full bloom (cents).
    pub vibrato_cents: f32,
    /// Nominal vibrato rate (Hz); the engine adds natural rate/depth wander.
    pub vibrato_rate_hz: f32,
    /// Delay before vibrato starts blooming after a fresh attack (seconds).
    pub vibrato_onset_seconds: f32,
    /// Reactive release taper time constant (seconds), starting at note-off.
    pub release_seconds: f32,
}

/// The engine's per-sample outputs. `intensity` is the fully composed signal
/// (velocity x development x swell x release); the components are also
/// exposed separately because threshold oscillators (reeds) cannot take
/// multiplicative level modulation near the speaking threshold — they map the
/// swell onto a pressure rail instead and gate on `release_multiplier` alone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PhraseOutputs {
    /// Living effort signal in `[0, 1]` (velocity shaped by the note
    /// lifecycle). Zero when idle.
    pub intensity: f32,
    /// Deliberate pitch motion in cents (vibrato; fades with the release).
    pub pitch_lean_cents: f32,
    /// Signed sustain-swell fraction (depth-scaled walk; 0 when idle).
    pub sustain_swell: f32,
    /// Release taper: 1 during the note, decaying after note-off, 0 idle.
    pub release_multiplier: f32,
    /// True from note-on until the release taper has fully decayed; drive the
    /// note gate from this so the taper is audible before the gate closes.
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhrasePhase {
    Idle,
    Note,
    Release,
}

/// Fallback fraction of the plateau a fresh attack starts from when the
/// caller passes a non-finite value. The real start is per instrument: a bow
/// can land from near-silence, while a threshold oscillator (a reed) must
/// start at speaking pressure and bloom above it.
const DEVELOPMENT_START_FALLBACK: f32 = 0.35;
/// Release multiplier below which the note is considered finished.
const RELEASE_FLOOR: f32 = 0.015;
/// Vibrato bloom time constant once past the onset delay (seconds).
const VIBRATO_BLOOM_SECONDS: f32 = 0.4;
/// Natural wander of the vibrato rate/depth and the slow sustain swell:
/// deliberate motion is slower than the involuntary humanize walks.
const VIBRATO_RATE_WANDER_FRACTION: f32 = 0.10;
const VIBRATO_RATE_WALK_TARGET_SECONDS: f32 = 1.1;
const VIBRATO_RATE_WALK_SMOOTH_SECONDS: f32 = 0.8;
const VIBRATO_DEPTH_WANDER_FRACTION: f32 = 0.25;
const VIBRATO_DEPTH_WALK_TARGET_SECONDS: f32 = 1.4;
const VIBRATO_DEPTH_WALK_SMOOTH_SECONDS: f32 = 1.0;
const SWELL_WALK_TARGET_SECONDS: f32 = 2.7;
const SWELL_WALK_SMOOTH_SECONDS: f32 = 1.9;
const VIBRATO_RATE_WALK_TAG: u32 = 0x7E19_4D2B;
const VIBRATO_DEPTH_WALK_TAG: u32 = 0x2C8B_F167;
const SWELL_WALK_TAG: u32 = 0x91D3_6E4F;

/// Per-voice phrasing engine. Allocation-free and `Copy`-state; suitable for
/// per-sample use on the audio thread.
#[derive(Debug, Clone, Copy)]
pub struct PhraseEngine {
    sample_rate: f32,
    phase: PhrasePhase,
    velocity: f32,
    development: f32,
    time_in_note_seconds: f32,
    vibrato_phase: f32,
    vibrato_bloom: f32,
    release_multiplier: f32,
    rate_walk: SmoothNoise,
    depth_walk: SmoothNoise,
    swell_walk: SmoothNoise,
}

impl PhraseEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let instance = variance::variance_instance_seed();
        Self {
            sample_rate,
            phase: PhrasePhase::Idle,
            velocity: 0.0,
            development: 0.0,
            time_in_note_seconds: 0.0,
            vibrato_phase: 0.0,
            vibrato_bloom: 0.0,
            release_multiplier: 1.0,
            rate_walk: SmoothNoise::new(
                sample_rate,
                VIBRATO_RATE_WALK_TARGET_SECONDS,
                VIBRATO_RATE_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, VIBRATO_RATE_WALK_TAG),
            ),
            depth_walk: SmoothNoise::new(
                sample_rate,
                VIBRATO_DEPTH_WALK_TARGET_SECONDS,
                VIBRATO_DEPTH_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, VIBRATO_DEPTH_WALK_TAG),
            ),
            swell_walk: SmoothNoise::new(
                sample_rate,
                SWELL_WALK_TARGET_SECONDS,
                SWELL_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, SWELL_WALK_TAG),
            ),
        }
    }

    pub fn reset(&mut self) {
        self.phase = PhrasePhase::Idle;
        self.velocity = 0.0;
        self.development = 0.0;
        self.time_in_note_seconds = 0.0;
        self.vibrato_phase = 0.0;
        self.vibrato_bloom = 0.0;
        self.release_multiplier = 1.0;
    }

    /// Begin (or continue) a note. A legato entry — a note arriving while the
    /// previous one still sounds — keeps the developed intensity and the
    /// blooming vibrato; a fresh attack restarts development (from the
    /// instrument's `development_start` fraction of the plateau) and the
    /// vibrato onset clock.
    pub fn note_on(&mut self, velocity: f32, legato: bool, development_start: f32) {
        let velocity = if velocity.is_finite() {
            velocity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let development_start = if development_start.is_finite() {
            development_start.clamp(0.0, 1.0)
        } else {
            DEVELOPMENT_START_FALLBACK
        };
        self.velocity = velocity;
        self.release_multiplier = 1.0;
        if !legato {
            self.development = development_start;
            self.time_in_note_seconds = 0.0;
            self.vibrato_bloom = 0.0;
        }
        self.phase = PhrasePhase::Note;
    }

    /// Note released: begin the reactive taper. The engine stays `active`
    /// until the taper decays, so the ending is heard, not gated.
    pub fn note_off(&mut self) {
        if self.phase == PhrasePhase::Note {
            self.phase = PhrasePhase::Release;
        }
    }

    pub fn is_active(&self) -> bool {
        self.phase != PhrasePhase::Idle
    }

    pub fn process(&mut self, params: PhraseParams) -> PhraseOutputs {
        if self.phase == PhrasePhase::Idle {
            return PhraseOutputs::default();
        }
        let dt = 1.0 / self.sample_rate;
        self.time_in_note_seconds += dt;

        // Asymptotic development toward the sustain plateau (duration-agnostic).
        let attack_coeff = (dt / params.attack_seconds.max(dt)).min(1.0);
        self.development += (1.0 - self.development) * attack_coeff;

        // Slow deliberate sustain swell around the plateau.
        let sustain_swell = self.swell_walk.process() * params.swell_depth;
        let swell = 1.0 + sustain_swell;

        // Vibrato blooms in after the onset delay; rate and depth wander.
        if self.time_in_note_seconds > params.vibrato_onset_seconds {
            let bloom_coeff = (dt / VIBRATO_BLOOM_SECONDS).min(1.0);
            self.vibrato_bloom += (1.0 - self.vibrato_bloom) * bloom_coeff;
        }
        let rate = params.vibrato_rate_hz
            * (1.0 + VIBRATO_RATE_WANDER_FRACTION * self.rate_walk.process());
        self.vibrato_phase += std::f32::consts::TAU * rate.max(0.0) * dt;
        if self.vibrato_phase > std::f32::consts::TAU {
            self.vibrato_phase -= std::f32::consts::TAU;
        }
        let depth = params.vibrato_cents
            * (1.0 + VIBRATO_DEPTH_WANDER_FRACTION * self.depth_walk.process())
            * self.vibrato_bloom;

        // Reactive release: taper from note-off, vibrato fading with it.
        if self.phase == PhrasePhase::Release {
            let release_coeff = (dt / params.release_seconds.max(dt)).min(1.0);
            self.release_multiplier -= self.release_multiplier * release_coeff;
            if self.release_multiplier < RELEASE_FLOOR {
                self.phase = PhrasePhase::Idle;
                return PhraseOutputs::default();
            }
        }

        let intensity =
            (self.velocity * self.development * swell * self.release_multiplier).clamp(0.0, 1.0);
        let pitch_lean_cents = depth * self.vibrato_phase.sin() * self.release_multiplier;
        PhraseOutputs {
            intensity,
            pitch_lean_cents: if pitch_lean_cents.is_finite() {
                pitch_lean_cents
            } else {
                0.0
            },
            sustain_swell: if sustain_swell.is_finite() {
                sustain_swell * self.release_multiplier
            } else {
                0.0
            },
            release_multiplier: self.release_multiplier,
            active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f32 = 48_000.0;

    fn test_params() -> PhraseParams {
        PhraseParams {
            attack_seconds: 0.1,
            swell_depth: 0.1,
            vibrato_cents: 18.0,
            vibrato_rate_hz: 5.3,
            vibrato_onset_seconds: 0.3,
            release_seconds: 0.2,
        }
    }

    fn run(engine: &mut PhraseEngine, samples: usize) -> Vec<PhraseOutputs> {
        (0..samples)
            .map(|_| engine.process(test_params()))
            .collect()
    }

    #[test]
    fn idle_engine_is_inert() {
        let mut engine = PhraseEngine::new(SAMPLE_RATE);
        let outputs = run(&mut engine, 256);
        assert!(outputs.iter().all(|o| *o == PhraseOutputs::default()));
    }

    #[test]
    fn attack_develops_to_plateau_and_vibrato_blooms_after_onset() {
        let mut engine = PhraseEngine::new(SAMPLE_RATE);
        engine.note_on(0.8, false, 0.22);
        let outputs = run(&mut engine, 48_000);
        let early = outputs[480].intensity;
        let settled = outputs[24_000].intensity;
        assert!(
            early < settled,
            "attack should develop: {early} -> {settled}"
        );
        assert!(
            (0.6..=1.0).contains(&settled),
            "plateau near velocity: {settled}"
        );
        let early_pitch = outputs[..9_600]
            .iter()
            .map(|o| o.pitch_lean_cents.abs())
            .fold(0.0_f32, f32::max);
        let late_pitch = outputs[33_600..]
            .iter()
            .map(|o| o.pitch_lean_cents.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            late_pitch > early_pitch * 2.0 && late_pitch > 8.0,
            "vibrato should bloom in late: early={early_pitch:.2} late={late_pitch:.2}"
        );
    }

    #[test]
    fn release_tapers_then_deactivates_and_fades_vibrato() {
        let mut engine = PhraseEngine::new(SAMPLE_RATE);
        engine.note_on(1.0, false, 0.22);
        run(&mut engine, 48_000);
        engine.note_off();
        let outputs = run(&mut engine, 48_000);
        let just_after = outputs[480].intensity;
        let later = outputs[9_600].intensity;
        assert!(
            just_after > later,
            "release should taper: {just_after} -> {later}"
        );
        assert!(!outputs.last().unwrap().active, "release should deactivate");
        assert!(
            outputs.last().unwrap().pitch_lean_cents == 0.0,
            "vibrato should be gone when idle"
        );
    }

    #[test]
    fn legato_keeps_development_and_vibrato_clock() {
        let mut engine = PhraseEngine::new(SAMPLE_RATE);
        engine.note_on(0.9, false, 0.22);
        run(&mut engine, 48_000);
        engine.note_on(0.9, true, 0.22);
        let after_legato = engine.process(test_params());
        assert!(
            after_legato.intensity > 0.7,
            "legato should keep the developed intensity: {}",
            after_legato.intensity
        );
        let outputs = run(&mut engine, 4_800);
        let vibrato = outputs
            .iter()
            .map(|o| o.pitch_lean_cents.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            vibrato > 8.0,
            "legato should keep vibrato blooming: {vibrato}"
        );
    }

    #[test]
    fn host_expression_is_exactly_inert_without_events() {
        let mut expression = HostExpression::new(SAMPLE_RATE);
        for _ in 0..4_800 {
            let outputs = expression.process();
            assert_eq!(outputs.intensity_factor, 1.0);
            assert_eq!(outputs.bend_cents, 0.0);
        }
    }

    #[test]
    fn host_expression_follows_the_dynamics_line_and_bend() {
        let mut expression = HostExpression::new(SAMPLE_RATE);
        expression.set_expression(0.25);
        expression.set_bend_semitones(0.5);
        let mut last = expression.process();
        for _ in 0..9_600 {
            last = expression.process();
        }
        assert!((last.intensity_factor - 0.25).abs() < 0.01, "{last:?}");
        assert!((last.bend_cents - 50.0).abs() < 1.0, "{last:?}");
        expression.set_aftertouch(1.0);
        for _ in 0..9_600 {
            last = expression.process();
        }
        assert!(
            last.intensity_factor > 0.3,
            "aftertouch should swell above the line: {last:?}"
        );
    }

    #[test]
    fn knob_law_maps_half_to_nominal_and_full_to_extreme() {
        assert_eq!(knob_law_scale(0.0, 2.5), 0.0);
        assert!((knob_law_scale(0.5, 2.5) - 1.0).abs() < 1.0e-6);
        assert!((knob_law_scale(1.0, 2.5) - 2.5).abs() < 1.0e-6);
        assert!(knob_law_scale(0.25, 2.5) < 1.0);
        assert!(knob_law_scale(0.75, 2.5) > 1.0);
    }
}

/// Frequency ratio for a (small) cents offset: `1 + cents·ln2/1200`, exact to
/// a part in 10^5 over expressive ranges.
pub fn cents_ratio(cents: f32) -> f32 {
    if cents.is_finite() {
        1.0 + 0.000_577_623 * cents
    } else {
        1.0
    }
}

/// Smoothing time of the host-expression inputs.
const EXPRESSION_SMOOTH_SECONDS: f32 = 0.03;
/// Aftertouch swells *above* the expression line (additive, so a controller
/// resting at zero channel pressure leaves the sound untouched).
const AFTERTOUCH_SWELL_DEPTH: f32 = 0.3;

/// Per-sample host-expression outputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostExpressionOutputs {
    /// Multiplier on the phrase intensity (1.0 when the host sends nothing).
    pub intensity_factor: f32,
    /// Additive pitch offset in cents (0.0 when the host sends nothing).
    pub bend_cents: f32,
}

/// The performance layer: host expression events smoothed into per-sample
/// signals that *ride on top of* the automatic phrasing. The CC dynamics line
/// (CC1/CC11) multiplies intensity — the orchestral-library idiom, so a drawn
/// lane carries the macro phrase (including anticipatory endings the causal
/// engine cannot know) while the automatic layer keeps supplying the
/// micro-musicality inside it. With no events the layer is exactly inert.
#[derive(Debug, Clone, Copy)]
pub struct HostExpression {
    expression_target: f32,
    expression: f32,
    aftertouch_target: f32,
    aftertouch: f32,
    bend_target_cents: f32,
    bend_cents: f32,
    coeff: f32,
}

impl HostExpression {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            expression_target: 1.0,
            expression: 1.0,
            aftertouch_target: 0.0,
            aftertouch: 0.0,
            bend_target_cents: 0.0,
            bend_cents: 0.0,
            coeff: (1.0 / (EXPRESSION_SMOOTH_SECONDS * sample_rate)).clamp(0.0, 1.0),
        }
    }

    pub fn reset(&mut self) {
        self.expression_target = 1.0;
        self.expression = 1.0;
        self.aftertouch_target = 0.0;
        self.aftertouch = 0.0;
        self.bend_target_cents = 0.0;
        self.bend_cents = 0.0;
    }

    /// CC1/CC11 dynamics line, `0..1`.
    pub fn set_expression(&mut self, value: f32) {
        if value.is_finite() {
            self.expression_target = value.clamp(0.0, 1.0);
        }
    }

    /// Channel pressure, `0..1` (swells above the expression line).
    pub fn set_aftertouch(&mut self, value: f32) {
        if value.is_finite() {
            self.aftertouch_target = value.clamp(0.0, 1.0);
        }
    }

    /// Pitch bend in semitones (host range already applied).
    pub fn set_bend_semitones(&mut self, semitones: f32) {
        if semitones.is_finite() {
            self.bend_target_cents = (semitones * 100.0).clamp(-2_400.0, 2_400.0);
        }
    }

    pub fn process(&mut self) -> HostExpressionOutputs {
        self.expression += self.coeff * (self.expression_target - self.expression);
        self.aftertouch += self.coeff * (self.aftertouch_target - self.aftertouch);
        self.bend_cents += self.coeff * (self.bend_target_cents - self.bend_cents);
        HostExpressionOutputs {
            intensity_factor: self.expression * (1.0 + AFTERTOUCH_SWELL_DEPTH * self.aftertouch),
            bend_cents: self.bend_cents,
        }
    }
}
