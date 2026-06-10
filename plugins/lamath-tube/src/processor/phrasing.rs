//! Phrasing for the wind voice: the shared note-lifecycle engine
//! (`lindelion-dsp-utils::phrase`) mapped onto breath. Intensity replaces the
//! static note-on effort; vibrato is *breath* vibrato — the engine's pitch
//! signal maps onto the same breath-pressure rail the humanize walks ride,
//! not onto bore pitch. Knob law: NOMINAL_* are the 50% sound, the
//! multipliers set the theatrical ceiling.

use lindelion_dsp_utils::phrase::{PhraseParams, knob_law_scale};

/// The reed is a threshold oscillator with starting hysteresis: oscillation
/// locks from the note-on kick at speaking pressure, and a breath that ramps
/// up from below the threshold may never speak at all (soft notes died in the
/// register sweep at any lower start). A wind note therefore has no *level*
/// development at the attack — its development is brightness/overpressure,
/// which the existing effort-coupled attack envelope provides; the phrase
/// engine contributes the sustain swell, breath vibrato, and release taper.
pub(super) const PHRASE_DEVELOPMENT_START: f32 = 1.0;
const PHRASE_NOMINAL_ATTACK_SECONDS: f32 = 0.15;
const PHRASE_ATTACK_MAX_MULTIPLIER: f32 = 2.0;
const PHRASE_NOMINAL_SWELL_DEPTH: f32 = 0.18;
const PHRASE_SWELL_MAX_MULTIPLIER: f32 = 2.0;
const PHRASE_NOMINAL_VIBRATO_CENTS: f32 = 18.0;
const PHRASE_VIBRATO_MAX_MULTIPLIER: f32 = 2.5;
const PHRASE_VIBRATO_RATE_HZ: f32 = 5.1;
const PHRASE_VIBRATO_ONSET_SECONDS: f32 = 0.35;
const PHRASE_NOMINAL_RELEASE_SECONDS: f32 = 0.12;
const PHRASE_RELEASE_MAX_MULTIPLIER: f32 = 2.5;
/// Breath-pressure modulation per engine vibrato cent (nominal 18 cents →
/// ≈ ±11% breath, an audible wind vibrato).
pub(super) const PHRASE_VIBRATO_BREATH_PER_CENT: f32 = 0.006;

pub(super) fn phrase_params_for_knobs(phrasing: f32, vibrato: f32) -> PhraseParams {
    PhraseParams {
        attack_seconds: PHRASE_NOMINAL_ATTACK_SECONDS
            * knob_law_scale(phrasing, PHRASE_ATTACK_MAX_MULTIPLIER),
        swell_depth: PHRASE_NOMINAL_SWELL_DEPTH
            * knob_law_scale(phrasing, PHRASE_SWELL_MAX_MULTIPLIER),
        vibrato_cents: PHRASE_NOMINAL_VIBRATO_CENTS
            * knob_law_scale(vibrato, PHRASE_VIBRATO_MAX_MULTIPLIER),
        vibrato_rate_hz: PHRASE_VIBRATO_RATE_HZ,
        vibrato_onset_seconds: PHRASE_VIBRATO_ONSET_SECONDS,
        release_seconds: PHRASE_NOMINAL_RELEASE_SECONDS
            * knob_law_scale(phrasing, PHRASE_RELEASE_MAX_MULTIPLIER),
    }
}
