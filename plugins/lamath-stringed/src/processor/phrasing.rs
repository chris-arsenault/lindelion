//! Phrasing for the bowed/plucked string: the deliberate note lifecycle
//! (shared engine in `lindelion-dsp-utils::phrase`), riding the same
//! physical-target rails as humanize. The NOMINAL_* values are the knob-law
//! 50% sound; the multipliers set the 100% (theatrical) ceiling. Intensity
//! replaces the static note-on effort (the existing effort scale co-moves bow
//! speed and force, so dynamics stay Schelleng-invariant); vibrato rides the
//! played-frequency rail the intonation servo follows; the reactive release
//! holds the note gate open while the taper sounds.

use lindelion_dsp_utils::phrase::{PhraseParams, knob_law_scale};

/// A bow can land from near-silence and swell in.
pub(super) const PHRASE_DEVELOPMENT_START: f32 = 0.22;
const PHRASE_NOMINAL_ATTACK_SECONDS: f32 = 0.35;
const PHRASE_ATTACK_MAX_MULTIPLIER: f32 = 2.0;
const PHRASE_NOMINAL_SWELL_DEPTH: f32 = 0.22;
const PHRASE_SWELL_MAX_MULTIPLIER: f32 = 2.0;
const PHRASE_NOMINAL_VIBRATO_CENTS: f32 = 18.0;
const PHRASE_VIBRATO_MAX_MULTIPLIER: f32 = 2.5;
const PHRASE_VIBRATO_RATE_HZ: f32 = 5.3;
const PHRASE_VIBRATO_ONSET_SECONDS: f32 = 0.35;
const PHRASE_NOMINAL_RELEASE_SECONDS: f32 = 0.18;
const PHRASE_RELEASE_MAX_MULTIPLIER: f32 = 2.8;

/// Vibrato has its own knob: pitch motion masks the rest of the note shape,
/// so the two are auditioned and played independently. `phrasing` scales the
/// lifecycle (attack development, swell, release taper); `vibrato` scales
/// only the pitch motion.
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
