//! Shared score-support types for the symphony's movements: mixer placement
//! specs, track definitions, and phrase helpers. The music itself lives in
//! the per-movement modules under [`crate::movements`].

use crate::render::Voice;
use crate::score::{Note, Part};

/// (bar, beat, duration beats, written MIDI note, velocity)
pub(crate) type PhraseNote = (usize, f32, f32, u8, f32);

/// How the mixer levels a track before summing.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Level {
    /// Normalize the track's active-region RMS to this dBFS value. For
    /// sustained material, where loudness is what matters.
    ActiveRms(f32),
    /// Normalize the track's absolute peak to this dBFS value. For sparse
    /// transient material (the crash), whose ring-tail RMS says nothing about
    /// how hard its strikes hit the mix bus.
    Peak(f32),
}

/// One mixer channel's placement: level target, pan, cleanup filter, fader ride.
pub(crate) struct MixSpec {
    pub(crate) level: Level,
    /// Constant-power pan, -1 (left) to +1 (right).
    pub(crate) pan: f32,
    /// Cleanup highpass cutoff applied to the stem before leveling.
    pub(crate) highpass_hz: f32,
    /// Fader ride as (bar, dB) breakpoints, linearly interpolated; constant
    /// before the first and after the last. Empty = flat at 0 dB.
    pub(crate) ride: &'static [(f32, f32)],
}

/// One mixer channel: an instrument voice, its notes, and its mix placement.
pub(crate) struct TrackSpec {
    pub(crate) name: &'static str,
    pub(crate) voice: Voice,
    pub(crate) notes: Vec<Note>,
    pub(crate) mix: MixSpec,
}

pub(crate) const FLAT: &[(f32, f32)] = &[];

/// Active-RMS-leveled mixer placement.
pub(crate) fn rms_mix(
    target_rms_dbfs: f32,
    pan: f32,
    highpass_hz: f32,
    ride: &'static [(f32, f32)],
) -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(target_rms_dbfs),
        pan,
        highpass_hz,
        ride,
    }
}

/// Peak-leveled mixer placement (sparse transient tracks).
pub(crate) fn peak_mix(target_dbfs: f32, pan: f32, highpass_hz: f32) -> MixSpec {
    MixSpec {
        level: Level::Peak(target_dbfs),
        pan,
        highpass_hz,
        ride: FLAT,
    }
}

pub(crate) fn track(name: &'static str, voice: Voice, notes: Vec<Note>, mix: MixSpec) -> TrackSpec {
    TrackSpec {
        name,
        voice,
        notes,
        mix,
    }
}

/// Push `phrase`, shifting bars by `bar_offset` and dropping entries past `last_bar`.
pub(crate) fn push_phrase(
    part: &mut Part,
    phrase: &[PhraseNote],
    bar_offset: usize,
    last_bar: usize,
) {
    for (bar, beat, duration, midi, velocity) in phrase {
        if *bar <= last_bar {
            part.hit(bar + bar_offset, *beat, *duration, *midi, *velocity);
        }
    }
}
