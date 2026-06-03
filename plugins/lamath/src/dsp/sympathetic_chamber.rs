//! Cross-voice sympathetic resonance chamber (M10, ADR-0028).
//!
//! A shared, instrument-global resonant body, owned by the runtime as a **send/return**
//! around the voice mix — it does not live on, and does not touch, the voice engine.
//! Every block the runtime feeds it the rendered mix; it returns the sympathetic ring
//! summed back into the output.
//!
//! Unlike a fixed drone, its strings are **tuned to the notes you actually play**: a
//! pool of Karplus-Strong damped delay loops, each retuned on note-on to a played
//! pitch and left to ring (and decay) after the note ends. Because each string rings at
//! its fundamental *and its harmonics*, a string tuned to a held note blooms when you
//! play a harmonically related note — so the voices ring **each other**, which is the
//! essence of sympathetic resonance and exactly what a per-voice bank cannot do.
//!
//! The send is **energy-scaled**: a measured-energy follower on the mix drives a
//! squared, dynamics-concentrating send gain (the aggregate analogue of the per-voice
//! M2 energy bus), so soft playing barely stirs the strings and hard playing blooms.
//! The loops have gain < 1 plus an in-loop low-pass, so the chamber is passive and
//! bounded. At depth 0 it is inert (defeatable); `silence()` clears the tail.

use lindelion_dsp_utils::{
    delay::DelayLine,
    energy::EnergyFollower,
    filters::OnePoleLowpass,
    math::{self, midi_note_to_hz},
};

/// Number of sympathetic strings — the polyphony of distinct resonant pitches.
const SYMPATHETIC_STRING_COUNT: usize = 16;
/// Lowest pitch the pool can be tuned to, used to size the delay buffers.
const LOWEST_SYMPATHETIC_HZ: f32 = 28.0;
/// Round-trip loop gain (< 1 — the ring decays; passive/bounded). Sets the
/// sympathetic sustain and the resonant build-up at matching frequencies. Broad
/// enough to catch harmonically-related (not pitch-exact) excitation, long enough to
/// ring on after the exciting note.
const SYMPATHETIC_LOOP_GAIN: f32 = 0.97;
/// In-loop damping low-pass corner: higher partials of each string decay faster.
const SYMPATHETIC_DAMP_CUTOFF_HZ: f32 = 3_500.0;
/// Measured-energy (RMS of the mix) at which the energy-scaled send reaches full
/// strength; `(energy/REF)^2` keeps soft playing subtle and concentrates the bloom on
/// hard playing, matching the squared energy curve of the per-voice nonlinearities.
/// M11 P9: re-calibrated to the *post-P9-makeup* output mix. After P9 step 2 lifted the
/// per-family output level, a single full-velocity note mixes near RMS 0.1–0.34 (was
/// ~0.0025 at the P8-era level this REF's predecessor 0.004 targeted), so this REF lets a
/// single forte note reach ~0.5–1.0 send drive and a chord saturate, while soft playing
/// stays subtle. This is the one energy reference that observes the post-output mix —
/// i.e. downstream of the P9 gain staging — so it tracks the makeup, not the raw bus.
const SYMPATHETIC_SEND_ENERGY_REF: f32 = 0.2;
/// Base excitation scale into the strings. Small, because a high-Q loop builds the
/// matching frequencies up by ~`1/(1 - loop_gain)`; this keeps a fully-resonant string
/// near the mix level rather than dominating it.
const SYMPATHETIC_SEND_GAIN: f32 = 0.02;
/// Overall return gain of the summed strings back into the mix.
const SYMPATHETIC_RETURN_GAIN: f32 = 0.6;
/// Activity time constant (s) for the steal heuristic: a freshly-excited string reads
/// "busy" and a rung-out one reads "free".
const SYMPATHETIC_ACTIVITY_SECONDS: f32 = 0.25;
/// Activity stamped on a freshly (re)tuned string so the next note-on does not
/// immediately steal it back before the mix has excited it.
const SYMPATHETIC_ALLOC_ACTIVITY: f32 = 1.0;

#[derive(Debug)]
struct SympatheticString {
    delay: DelayLine,
    damping: OnePoleLowpass,
    delay_samples: f32,
    /// The MIDI note this string is currently tuned to (`None` = never tuned).
    note: Option<u8>,
    /// Peak-hold activity follower for the steal heuristic.
    activity: f32,
    /// Equal-power stereo placement of this string in the sympathetic field.
    pan_left: f32,
    pan_right: f32,
}

impl SympatheticString {
    fn new(sample_rate: f32, capacity: usize, pan: f32) -> Self {
        let mut damping = OnePoleLowpass::default();
        damping.set_cutoff(SYMPATHETIC_DAMP_CUTOFF_HZ, sample_rate);
        // Equal-power pan: `pan` in [-1, 1] maps to an angle in [0, pi/2].
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * (std::f32::consts::FRAC_PI_4);
        Self {
            delay: DelayLine::new(capacity),
            damping,
            delay_samples: 0.0,
            note: None,
            activity: 0.0,
            pan_left: angle.cos(),
            pan_right: angle.sin(),
        }
    }

    /// Run one sample of the damped loop with `excitation` injected, returning the
    /// string's vibrating output.
    fn process(&mut self, excitation: f32, activity_decay: f32) -> f32 {
        if self.note.is_none() {
            return 0.0;
        }
        let delayed = self.delay.read(self.delay_samples);
        let vibrating = math::snap_to_zero(self.damping.process(delayed) * SYMPATHETIC_LOOP_GAIN);
        self.delay.push(math::snap_to_zero(vibrating + excitation));
        self.activity = (self.activity * activity_decay).max(vibrating.abs());
        vibrating
    }

    fn clear(&mut self) {
        self.delay.clear();
        self.damping.reset();
        self.note = None;
        self.activity = 0.0;
    }
}

#[derive(Debug)]
pub(crate) struct SympatheticChamber {
    strings: [SympatheticString; SYMPATHETIC_STRING_COUNT],
    energy: EnergyFollower,
    activity_decay: f32,
    sample_rate: f32,
    depth: f32,
}

impl SympatheticChamber {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let capacity = (sample_rate / LOWEST_SYMPATHETIC_HZ).ceil() as usize + 4;
        let count = SYMPATHETIC_STRING_COUNT;
        Self {
            strings: std::array::from_fn(|index| {
                // Spread the strings across the stereo field for a wide sympathetic
                // body rather than a mono blob.
                let pan = if count > 1 {
                    (index as f32 / (count - 1) as f32) * 2.0 - 1.0
                } else {
                    0.0
                };
                SympatheticString::new(sample_rate, capacity, pan)
            }),
            energy: EnergyFollower::new(sample_rate),
            activity_decay: (-1.0 / (SYMPATHETIC_ACTIVITY_SECONDS * sample_rate)).exp(),
            sample_rate,
            depth: 0.0,
        }
    }

    /// Tune a string to a played note (M10): reuse the string already tuned to it,
    /// otherwise steal the least-active (most rung-out) string and retune it clean.
    /// The string then resonates whenever the mix carries energy at its pitch — the
    /// chamber needs no explicit strike, the radiated mix excites it.
    pub(crate) fn note_on(&mut self, note: u8) {
        let frequency =
            midi_note_to_hz(note as f32).clamp(LOWEST_SYMPATHETIC_HZ, self.sample_rate * 0.45);
        let capacity = self.strings[0].delay.capacity() as f32;
        let delay_samples = (self.sample_rate / frequency).clamp(2.0, capacity - 2.0);

        if let Some(existing) = self.strings.iter_mut().find(|s| s.note == Some(note)) {
            // Already resonant at this pitch — keep it alive, leave its ring intact.
            existing.activity = existing.activity.max(SYMPATHETIC_ALLOC_ACTIVITY);
            return;
        }

        let target = self
            .strings
            .iter_mut()
            .min_by(|a, b| a.activity.total_cmp(&b.activity))
            .expect("string pool is non-empty");
        target.clear();
        target.note = Some(note);
        target.delay_samples = delay_samples;
        target.activity = SYMPATHETIC_ALLOC_ACTIVITY;
    }

    pub(crate) fn set_depth(&mut self, depth: f32) {
        self.depth = math::finite_clamp(depth, 0.0, 1.0, 0.0);
    }

    /// Process the rendered mix in place: excite the tuned strings with an
    /// energy-scaled send and add the summed sympathetic ring back. Defeated (no-op)
    /// at depth 0.
    pub(crate) fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.depth <= 0.0 {
            return;
        }
        let len = left.len().min(right.len());
        for index in 0..len {
            let mix = math::snap_to_zero((left[index] + right[index]) * 0.5);
            // Energy-scaled send: soft playing barely stirs the strings, hard playing
            // blooms (squared, dynamics-concentrated — the aggregate energy bus).
            let energy = self.energy.observe(mix);
            let drive = math::finite_clamp(energy / SYMPATHETIC_SEND_ENERGY_REF, 0.0, 1.0, 0.0);
            let excitation = mix * self.depth * drive * drive * SYMPATHETIC_SEND_GAIN;

            let mut ring_left = 0.0;
            let mut ring_right = 0.0;
            for string in &mut self.strings {
                let vibrating = string.process(excitation, self.activity_decay);
                ring_left += vibrating * string.pan_left;
                ring_right += vibrating * string.pan_right;
            }
            left[index] = math::snap_to_zero(left[index] + ring_left * SYMPATHETIC_RETURN_GAIN);
            right[index] = math::snap_to_zero(right[index] + ring_right * SYMPATHETIC_RETURN_GAIN);
        }
    }

    /// Silence the sympathetic tail (panic / patch change / reset).
    pub(crate) fn silence(&mut self) {
        for string in &mut self.strings {
            string.clear();
        }
        self.energy.reset();
    }
}

#[cfg(test)]
mod tests;
