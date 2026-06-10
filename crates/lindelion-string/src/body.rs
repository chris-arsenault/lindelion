use lindelion_dsp_utils::{filters::OnePoleLowpass, math};

use crate::core;

/// Normalized string wave admittance at the bridge (wave variables are normalized,
/// so `G_s = 1`); the body admittance is scaled relative to it.
const STRING_BRIDGE_ADMITTANCE: f32 = 1.0;
/// Broadband body admittance at the bridge. Loading stays deliberately small so
/// the body does not become the decay control; radiation is larger because the
/// soundboard can radiate bridge motion without feeding all of that energy back
/// into the string as uniform loss. The radiation floor also stands in for the
/// dense bed of higher body modes the reduced bank truncates: with only ~10
/// modes, notes whose partials fall between the modelled resonances would
/// otherwise radiate tens of dB below on-resonance notes, far beyond the
/// level spread of a real instrument.
/// Sized against the Iowa MIS arco reference: high enough that between-mode
/// fundamentals stay within an instrument-like level spread, low enough that
/// the modal formant contrast (the violin's H4–H7 radiation valley before the
/// bridge hill) survives — a flat floor that drowns the valleys reads as a
/// reed organ, not a body.
const BODY_BACKGROUND_LOADING_ADMITTANCE: f32 = 0.000_5;
const BODY_BACKGROUND_RADIATION_ADMITTANCE: f32 = 0.006;
/// Global scale on the modal admittance gains: the body colors the timbre (its
/// *radiated* output) at full strength so the body stays audible, while a separate
/// `BODY_LOADING_SCALE` governs how much it *loads* the loop.
const BODY_GAIN_SCALE: f32 = 0.10;
/// M11 P8: fraction of the modal admittance that loads the string loop, decoupled
/// from the radiated coloration. At the old value (loading == radiation == full
/// `BODY_GAIN_SCALE`) the high-Q plate modes over-damped any midrange note whose
/// fundamental landed on them (~1 s vs ~5 s for in-gap notes). Loading the loop
/// less lets the midrange sustain while the body still radiates its colour. The
/// reflectance stays passive (|R| ≤ 1), so this never adds loop energy.
const BODY_LOADING_SCALE: f32 = 0.16;

/// A single body resonance as a driving-point **mobility/admittance**: a fixed
/// **absolute** frequency (a real body resonates at the same Hz regardless of the
/// played note — the string sweeps across it), a quality factor, and the peak
/// admittance magnitude (the coupling strength of that resonance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BodyMode {
    frequency_hz: f32,
    q: f32,
    gain: f32,
}

/// Which reduced-body voicing the String radiates through (M7 [DECISION]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StringBodyMode {
    Disabled,
    #[default]
    Guitar,
    Violin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyFamily {
    Guitar,
    Violin,
}

impl StringBodyMode {
    pub(crate) fn family(self) -> Option<BodyFamily> {
        match self {
            Self::Disabled => None,
            Self::Guitar => Some(BodyFamily::Guitar),
            Self::Violin => Some(BodyFamily::Violin),
        }
    }
}

/// Maximum modes any family uses, so the mode bank is sized once at construction
/// and re-tuning a family never allocates.
const MAX_BODY_MODES: usize = 12;

/// Guitar body: a strong low air/Helmholtz cavity mode plus the top/back plate
/// signature modes, and a broad high formant for the modal-overlap region.
const GUITAR_MODES: &[BodyMode] = &[
    BodyMode {
        frequency_hz: 100.0,
        q: 10.0,
        gain: 0.44,
    }, // air/Helmholtz (A0)
    BodyMode {
        frequency_hz: 200.0,
        q: 9.0,
        gain: 0.45,
    }, // top plate (T1)
    BodyMode {
        frequency_hz: 230.0,
        q: 8.0,
        gain: 0.40,
    }, // back plate
    BodyMode {
        frequency_hz: 280.0,
        q: 7.5,
        gain: 0.34,
    },
    BodyMode {
        frequency_hz: 360.0,
        q: 7.0,
        gain: 0.28,
    },
    BodyMode {
        frequency_hz: 455.0,
        q: 6.0,
        gain: 0.22,
    },
    BodyMode {
        frequency_hz: 590.0,
        q: 5.5,
        gain: 0.18,
    },
    BodyMode {
        frequency_hz: 760.0,
        q: 5.0,
        gain: 0.14,
    },
    BodyMode {
        frequency_hz: 1_050.0,
        q: 4.5,
        gain: 0.12,
    },
    BodyMode {
        frequency_hz: 1_450.0,
        q: 4.0,
        gain: 0.10,
    },
    BodyMode {
        frequency_hz: 2_700.0,
        q: 2.2,
        gain: 0.14,
    }, // broad formant
];

/// Violin body: higher air resonance, the B1-/B1+ main wood signature modes, and
/// a strong "bridge hill" formant near 2.5–3 kHz. Mode Qs sit near the low end
/// of measured violin plate/air Qs so neighbouring resonances overlap: with a
/// reduced bank, sharper modes leave deep radiation valleys between them
/// (e.g. an E4 fundamental between A0 and B1- would land ~20 dB below an
/// on-resonance note, far beyond a real violin's note-to-note spread).
const VIOLIN_MODES: &[BodyMode] = &[
    BodyMode {
        frequency_hz: 280.0,
        q: 14.0,
        gain: 0.50,
    }, // air (A0)
    BodyMode {
        frequency_hz: 460.0,
        q: 13.0,
        gain: 0.48,
    }, // main wood (B1-)
    BodyMode {
        frequency_hz: 530.0,
        q: 12.0,
        gain: 0.45,
    }, // (B1+)
    BodyMode {
        frequency_hz: 600.0,
        q: 11.0,
        gain: 0.32,
    },
    // The 600–900 Hz "transition region" radiates strongly on measured violins
    // (the Iowa arco C4 reference carries H3 at ≈ −4 dB rel H1), while the
    // 1–2 kHz region dips before the bridge hill (H4–H7 sit −23..−36 dB).
    // The gains below voice that contrast: generous 700/900 modes, lean
    // 1100/1500 modes.
    BodyMode {
        frequency_hz: 700.0,
        q: 7.0,
        gain: 0.42,
    },
    BodyMode {
        frequency_hz: 900.0,
        q: 9.0,
        gain: 0.30,
    },
    BodyMode {
        frequency_hz: 1_100.0,
        q: 8.0,
        gain: 0.12,
    },
    BodyMode {
        frequency_hz: 1_500.0,
        q: 7.0,
        gain: 0.10,
    },
    BodyMode {
        frequency_hz: 2_000.0,
        q: 6.0,
        gain: 0.15,
    },
    BodyMode {
        frequency_hz: 2_400.0,
        q: 5.0,
        gain: 0.34,
    }, // bridge-hill formant (measured violins peak ≈2.1–2.5 kHz, then fall)
];

fn family_modes(family: BodyFamily) -> &'static [BodyMode] {
    match family {
        BodyFamily::Guitar => GUITAR_MODES,
        BodyFamily::Violin => VIOLIN_MODES,
    }
}

/// High-pass corner of the broadband background *radiation* term, just below
/// the family's lowest signature mode. A body radiates as a monopole rolling
/// off ~12 dB/oct below its first resonance; a background floor flat to DC
/// instead leaks sub-fundamental content straight to the output — audible as
/// a low rumble in bowed tails, where the bow's mean drag force leaves slowly
/// decaying quasi-DC circulating on the string after release.
fn family_radiation_highpass_hz(family: BodyFamily) -> f32 {
    match family {
        BodyFamily::Guitar => 60.0,
        BodyFamily::Violin => 170.0,
    }
}

/// One body resonance as a positive-real bandpass **admittance** (mobility) biquad
/// (RBJ band-pass, 0 dB peak scaled to the mode gain). Direct-Form-I so its
/// instantaneous coefficient (`b0`) can be split from the state contribution — the
/// split the wave-digital bridge junction needs to resolve its delay-free loop.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyResonator {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BodyResonator {
    fn new(sample_rate: f32, mode: BodyMode) -> Self {
        let mut resonator = Self {
            b0: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        resonator.retune(sample_rate, mode);
        resonator
    }

    fn retune(&mut self, sample_rate: f32, mode: BodyMode) {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let frequency_hz = math::finite_clamp(mode.frequency_hz, 1.0, sample_rate * 0.45, 100.0);
        let q = math::finite_clamp(mode.q, 0.5, 200.0, 10.0);
        let gain = math::finite_clamp(mode.gain * BODY_GAIN_SCALE, 0.0, 4.0, 0.0);
        let omega = std::f32::consts::TAU * frequency_hz / sample_rate;
        let alpha = omega.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        // RBJ band-pass (constant 0 dB peak), scaled to the peak admittance `gain`.
        self.b0 = gain * alpha / a0;
        self.b2 = -gain * alpha / a0;
        self.a1 = -2.0 * omega.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    /// Instantaneous admittance coefficient (output per unit current input).
    fn instantaneous(self) -> f32 {
        self.b0
    }

    /// Output contribution from past state alone (current input = 0).
    fn state_contribution(self) -> f32 {
        self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2
    }

    /// Advance with the junction force and return the mode velocity.
    fn advance(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.state_contribution();
        self.x2 = self.x1;
        self.x1 = math::snap_to_zero(input);
        self.y2 = self.y1;
        self.y1 = math::snap_to_zero(output);
        self.y1
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Reduced modal body (Woodhouse; euphonics.org): a handful of signature modes, an
/// air/Helmholtz cavity mode, and a broad formant, modelling the body's
/// driving-point admittance. Coupled two-way at the bridge as a passive wave-
/// digital termination `R(z) = (G_s − Y_b)/(G_s + Y_b)` (|R| ≤ 1 at any mode Q, so
/// the resonant body never destabilizes the string loop). Fixed-size: modes
/// allocated once at construction so re-tuning a family is allocation-free.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ReducedBody {
    sample_rate: f32,
    family: BodyFamily,
    modes: Vec<BodyResonator>,
    // Two cascaded one-pole stages give the background radiation its 12 dB/oct
    // low-frequency roll-off (`hp = x - lp(x)` per stage); the modal terms are
    // bandpass biquads and already block DC.
    radiation_lowpass_a: OnePoleLowpass,
    radiation_lowpass_b: OnePoleLowpass,
}

impl ReducedBody {
    pub(super) fn new(sample_rate: f32, family: BodyFamily) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let mut body = Self {
            sample_rate,
            family,
            modes: Vec::with_capacity(MAX_BODY_MODES),
            radiation_lowpass_a: OnePoleLowpass::default(),
            radiation_lowpass_b: OnePoleLowpass::default(),
        };
        body.configure(family);
        body
    }

    /// Re-tune the body to a family in place (allocation-free): reuse existing
    /// resonators, growing only within the construction-time capacity.
    pub(super) fn configure(&mut self, family: BodyFamily) {
        self.family = family;
        let table = family_modes(family);
        for (slot, mode) in self.modes.iter_mut().zip(table.iter()) {
            slot.retune(self.sample_rate, *mode);
        }
        for mode in table.iter().skip(self.modes.len()) {
            self.modes.push(BodyResonator::new(self.sample_rate, *mode));
        }
        self.modes.truncate(table.len());
        let highpass_hz = family_radiation_highpass_hz(family);
        self.radiation_lowpass_a
            .set_cutoff(highpass_hz, self.sample_rate);
        self.radiation_lowpass_b
            .set_cutoff(highpass_hz, self.sample_rate);
    }

    pub(super) fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.reset();
        }
        self.radiation_lowpass_a.reset();
        self.radiation_lowpass_b.reset();
    }

    /// Wave-digital bridge junction. Given the string's bridge-incident wave,
    /// return `(reflected, radiated)`: the body admittance loads the reflection
    /// passively (`|R| ≤ 1`, dips at body modes → those partials decay faster), and
    /// the body velocity is the radiated output.
    pub(super) fn bridge(&mut self, incident: f32) -> (f32, f32) {
        let incident = math::snap_to_zero(incident);
        let g = STRING_BRIDGE_ADMITTANCE;
        // The flat background admittance contributes to the instantaneous term, so
        // the body loads and radiates the string broadband (not only at its modes).
        let mut y_inf = BODY_BACKGROUND_LOADING_ADMITTANCE;
        let mut v_state = 0.0;
        // The modal admittance loads the loop at `BODY_LOADING_SCALE` of its radiated
        // strength (M11 P8): the body still colours the *output* at full gain, but it
        // damps the string loop only lightly, so a midrange fundamental landing on a
        // high-Q plate mode rings on instead of being choked.
        for mode in &self.modes {
            y_inf += BODY_LOADING_SCALE * mode.instantaneous();
            v_state += BODY_LOADING_SCALE * mode.state_contribution();
        }
        // Explicit junction solve (the delay-free loop resolved):
        // G(a - b) = Y_inf*(a + b) + v_state  =>  b = [a(G - Y_inf) - v_state]/(G + Y_inf).
        let reflected = (incident * (g - y_inf) - v_state) / (g + y_inf);
        let force = incident + reflected;
        // Body velocity = Y * force: the broadband background (high-passed below
        // the lowest mode — see `family_radiation_highpass_hz`) plus the modal
        // peaks.
        let background = {
            let stage_a = force - self.radiation_lowpass_a.process(force);
            stage_a - self.radiation_lowpass_b.process(stage_a)
        };
        let mut velocity = BODY_BACKGROUND_RADIATION_ADMITTANCE * background;
        for mode in &mut self.modes {
            velocity += mode.advance(force);
        }
        (math::snap_to_zero(reflected), math::snap_to_zero(velocity))
    }
}
