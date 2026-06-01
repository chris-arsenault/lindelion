use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::{WaveguideParams, WaveguideStyle, core};
use crate::dsp::constants::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY};

/// Normalized string wave admittance at the bridge (wave variables are normalized,
/// so `G_s = 1`); the body admittance is scaled relative to it.
const STRING_BRIDGE_ADMITTANCE: f32 = 1.0;
/// Broadband background admittance: a real body is not infinitely stiff between its
/// resonances, so it moves (and radiates) a little at every frequency the string
/// drives it. Kept small: it enters the bridge reflectance `y_inf`, so it loads the
/// string loop with a *uniform* loss at every frequency — too large a value would
/// override `loop_gain` as the decay control and over-damp the whole string, not
/// just the partials near body modes (the intended two-way wolf-note coloring lives
/// in the modal admittance, which is frequency-localized). The String output blends
/// in a pickup tap that already carries the broadband pitch, so the body no longer
/// needs a large background just to radiate the fundamental; this is only the body's
/// faint inter-resonance motion.
///
/// M11 P2 step 2: lowered from 0.015 so this flat loss no longer *overrides*
/// `loop_gain` as the decay control — at the old value it capped the free-pluck
/// tail near ~1 s regardless of the loop. The body's audible identity is its
/// frequency-localized modal admittance (`BODY_GAIN_SCALE` × the mode bank),
/// which is untouched; this is only the characterless broadband term, kept just
/// large enough to remain present.
const BODY_BACKGROUND_ADMITTANCE: f32 = 0.000_5;
/// Global scale on the modal admittance gains: the body colors the timbre and
/// loads the loop at its modes, but stays a light coupling so the string pitch
/// dominates (the string is far higher impedance than the body) and the fundamental
/// is only gently pulled near body resonances.
const BODY_GAIN_SCALE: f32 = 0.08;

/// Tube body voicing (M11 P4): fixed bore-body resonance and a broad bell-flare
/// formant the played note sweeps across (a real instrument body, not a
/// pitch-following formant).
const TUBE_BORE_BODY_HZ: f32 = 280.0;
const TUBE_BELL_FLARE_HZ: f32 = 1_500.0;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyFamily {
    Guitar,
    // Both families ship (M7 [DECISION]); the String defaults to Guitar today and
    // family selection is a future user control, so Violin is only built in tests.
    #[allow(dead_code)]
    Violin,
}

/// Maximum modes any family uses, so the mode bank is sized once at construction
/// and re-tuning a family never allocates.
const MAX_BODY_MODES: usize = 12;

/// Guitar body: a strong low air/Helmholtz cavity mode plus the top/back plate
/// signature modes, and a broad high formant for the modal-overlap region.
const GUITAR_MODES: &[BodyMode] = &[
    BodyMode {
        frequency_hz: 100.0,
        q: 28.0,
        gain: 0.55,
    }, // air/Helmholtz (A0)
    BodyMode {
        frequency_hz: 200.0,
        q: 26.0,
        gain: 0.45,
    }, // top plate (T1)
    BodyMode {
        frequency_hz: 230.0,
        q: 24.0,
        gain: 0.38,
    }, // back plate
    BodyMode {
        frequency_hz: 280.0,
        q: 22.0,
        gain: 0.30,
    },
    BodyMode {
        frequency_hz: 370.0,
        q: 20.0,
        gain: 0.26,
    },
    BodyMode {
        frequency_hz: 430.0,
        q: 18.0,
        gain: 0.22,
    },
    BodyMode {
        frequency_hz: 550.0,
        q: 16.0,
        gain: 0.18,
    },
    BodyMode {
        frequency_hz: 650.0,
        q: 15.0,
        gain: 0.15,
    },
    BodyMode {
        frequency_hz: 820.0,
        q: 13.0,
        gain: 0.12,
    },
    BodyMode {
        frequency_hz: 1_180.0,
        q: 11.0,
        gain: 0.10,
    },
    BodyMode {
        frequency_hz: 2_500.0,
        q: 3.0,
        gain: 0.18,
    }, // broad formant
];

/// Violin body: higher air resonance, the B1-/B1+ main wood signature modes, and
/// a strong "bridge hill" formant near 2.5–3 kHz.
const VIOLIN_MODES: &[BodyMode] = &[
    BodyMode {
        frequency_hz: 280.0,
        q: 24.0,
        gain: 0.50,
    }, // air (A0)
    BodyMode {
        frequency_hz: 460.0,
        q: 22.0,
        gain: 0.48,
    }, // main wood (B1-)
    BodyMode {
        frequency_hz: 530.0,
        q: 21.0,
        gain: 0.45,
    }, // (B1+)
    BodyMode {
        frequency_hz: 600.0,
        q: 19.0,
        gain: 0.32,
    },
    BodyMode {
        frequency_hz: 700.0,
        q: 17.0,
        gain: 0.28,
    },
    BodyMode {
        frequency_hz: 900.0,
        q: 15.0,
        gain: 0.24,
    },
    BodyMode {
        frequency_hz: 1_100.0,
        q: 13.0,
        gain: 0.20,
    },
    BodyMode {
        frequency_hz: 1_500.0,
        q: 11.0,
        gain: 0.17,
    },
    BodyMode {
        frequency_hz: 2_000.0,
        q: 9.0,
        gain: 0.15,
    },
    BodyMode {
        frequency_hz: 2_800.0,
        q: 4.0,
        gain: 0.34,
    }, // bridge-hill formant
];

fn family_modes(family: BodyFamily) -> &'static [BodyMode] {
    match family {
        BodyFamily::Guitar => GUITAR_MODES,
        BodyFamily::Violin => VIOLIN_MODES,
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
}

impl ReducedBody {
    pub(super) fn new(sample_rate: f32, family: BodyFamily) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let mut body = Self {
            sample_rate,
            family,
            modes: Vec::with_capacity(MAX_BODY_MODES),
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
    }

    pub(super) fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.reset();
        }
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
        let mut y_inf = BODY_BACKGROUND_ADMITTANCE;
        let mut v_state = 0.0;
        for mode in &self.modes {
            y_inf += mode.instantaneous();
            v_state += mode.state_contribution();
        }
        // Explicit junction solve (the delay-free loop resolved):
        // G(a - b) = Y_inf*(a + b) + v_state  =>  b = [a(G - Y_inf) - v_state]/(G + Y_inf).
        let reflected = (incident * (g - y_inf) - v_state) / (g + y_inf);
        let force = incident + reflected;
        // Body velocity = Y * force: the broadband background plus the modal peaks.
        let mut velocity = BODY_BACKGROUND_ADMITTANCE * force;
        for mode in &mut self.modes {
            velocity += mode.advance(force);
        }
        (math::snap_to_zero(reflected), math::snap_to_zero(velocity))
    }

    /// Standalone radiation (for testing the body in isolation): drive the bridge
    /// with an input and return the radiated motion.
    #[cfg(test)]
    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        self.bridge(input).1
    }
}

// Candidate extraction (ADR-0003): the prepared-model cache pattern below is
// single-consumer today; keep it local in Lamath until a second product needs it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct WaveguideBody {
    sample_rate: f32,
    highpass: Biquad,
    lowpass: Biquad,
    low_resonance: Biquad,
    high_resonance: Biquad,
    /// Control-rate cache of the derived body profile: recomputed only when the
    /// incoming params move, so `process_sample` does no per-sample derivation.
    prepared: Option<(WaveguideParams, BodyProfile)>,
    #[cfg(test)]
    recompute_count: u32,
}

impl WaveguideBody {
    pub(super) fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            highpass: Biquad::new(BiquadCoefficients::identity()),
            lowpass: Biquad::new(BiquadCoefficients::identity()),
            low_resonance: Biquad::new(BiquadCoefficients::identity()),
            high_resonance: Biquad::new(BiquadCoefficients::identity()),
            prepared: None,
            #[cfg(test)]
            recompute_count: 0,
        }
    }

    pub(super) fn reset(&mut self) {
        self.highpass.reset();
        self.lowpass.reset();
        self.low_resonance.reset();
        self.high_resonance.reset();
        self.prepared = None;
    }

    pub(super) fn process_sample(&mut self, input: f32, params: WaveguideParams) -> f32 {
        let profile = self.prepared_profile(params);

        let input = math::snap_to_zero(input);
        let radiating_input = self.highpass.process(input);
        let direct = self.lowpass.process(radiating_input) * profile.direct_gain;
        let low_body = self.low_resonance.process(radiating_input) * profile.low_resonance_gain;
        let high_body = self.high_resonance.process(radiating_input) * profile.high_resonance_gain;

        math::snap_to_zero((direct + low_body + high_body) * profile.output_gain)
    }

    /// Return the cached body profile, re-deriving it and pushing the new
    /// coefficients into the filters only when the incoming params have moved.
    fn prepared_profile(&mut self, params: WaveguideParams) -> BodyProfile {
        if let Some((cached_params, profile)) = self.prepared
            && cached_params == params
        {
            return profile;
        }

        let profile = BodyProfile::from_params(self.sample_rate, params);
        self.highpass.set_coefficients(profile.highpass);
        self.lowpass.set_coefficients(profile.lowpass);
        self.low_resonance.set_coefficients(profile.low_resonance);
        self.high_resonance.set_coefficients(profile.high_resonance);
        self.prepared = Some((params, profile));
        #[cfg(test)]
        {
            self.recompute_count += 1;
        }
        profile
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyProfile {
    highpass: BiquadCoefficients,
    lowpass: BiquadCoefficients,
    low_resonance: BiquadCoefficients,
    high_resonance: BiquadCoefficients,
    direct_gain: f32,
    low_resonance_gain: f32,
    high_resonance_gain: f32,
    output_gain: f32,
}

impl BodyProfile {
    fn from_params(sample_rate: f32, params: WaveguideParams) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let frequency_hz = math::finite_clamp(params.frequency_hz, 20.0, sample_rate * 0.45, 220.0);
        let loop_cutoff =
            math::finite_clamp(params.loop_filter_cutoff, 20.0, sample_rate * 0.45, 8_000.0);

        match params.style {
            WaveguideStyle::String => {
                let low_body_hz =
                    math::finite_clamp(150.0 + frequency_hz * 0.22, 110.0, 460.0, 180.0);
                let high_body_hz =
                    math::finite_clamp(760.0 + frequency_hz * 0.5, 480.0, 2_600.0, 900.0);
                let radiation_cutoff =
                    math::finite_clamp(loop_cutoff * 1.2, 2_800.0, sample_rate * 0.45, 9_000.0);

                Self {
                    highpass: BiquadCoefficients::highpass(sample_rate, 28.0, DEFAULT_BIQUAD_Q),
                    lowpass: BiquadCoefficients::lowpass(
                        sample_rate,
                        radiation_cutoff,
                        DEFAULT_BIQUAD_Q,
                    ),
                    low_resonance: BiquadCoefficients::bandpass(sample_rate, low_body_hz, 1.1),
                    high_resonance: BiquadCoefficients::bandpass(sample_rate, high_body_hz, 1.4),
                    direct_gain: 0.82,
                    low_resonance_gain: 0.18,
                    high_resonance_gain: 0.08,
                    output_gain: 0.92,
                }
            }
            WaveguideStyle::Tube => {
                // M11 P4 step 4: a real instrument body resonates at fixed Hz that the
                // played note sweeps across — a bore-body resonance and a broad
                // bell-flare formant — rather than the old pitch-following ×2/×5
                // formants. The radiation low-pass still tracks the loop cutoff.
                let bore_body_hz = TUBE_BORE_BODY_HZ;
                let bell_flare_hz = TUBE_BELL_FLARE_HZ;
                let radiation_cutoff =
                    math::finite_clamp(loop_cutoff * 1.4, 2_200.0, sample_rate * 0.45, 10_000.0);

                Self {
                    highpass: BiquadCoefficients::highpass(sample_rate, 45.0, DEFAULT_BIQUAD_Q),
                    lowpass: BiquadCoefficients::lowpass(
                        sample_rate,
                        radiation_cutoff,
                        DEFAULT_BIQUAD_Q,
                    ),
                    low_resonance: BiquadCoefficients::bandpass(sample_rate, bore_body_hz, 2.0),
                    high_resonance: BiquadCoefficients::bandpass(sample_rate, bell_flare_hz, 1.6),
                    direct_gain: 0.72,
                    low_resonance_gain: 0.18,
                    high_resonance_gain: 0.16,
                    output_gain: TUBE_BOUNDARY.output_gain(params.boundary_reflection),
                }
            }
        }
    }
}

/// A one-way modal coloration body (M11 P4): a parallel bank of fixed-frequency
/// band-pass formants summed with a dry path, for a resonator that radiates
/// *through* a body rather than coupling two-way into it (the Mesh, and any other
/// output-coloring body). Reuses `BodyMode` for the formant set but, unlike
/// `ReducedBody`, applies the mode gains directly (no String bridge-coupling
/// scale) and never feeds back into the resonator. Fixed-size: the bank is built
/// once at construction, so re-using a voice only `reset`s it (allocation-free).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OutputBody {
    filters: Vec<Biquad>,
    gains: Vec<f32>,
    dry_gain: f32,
}

impl OutputBody {
    pub(super) fn new(sample_rate: f32, modes: &[BodyMode], dry_gain: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let filters = modes
            .iter()
            .map(|mode| {
                let frequency_hz =
                    math::finite_clamp(mode.frequency_hz, 1.0, sample_rate * 0.45, 100.0);
                let q = math::finite_clamp(mode.q, 0.5, 200.0, 10.0);
                Biquad::new(BiquadCoefficients::bandpass(sample_rate, frequency_hz, q))
            })
            .collect();
        let gains = modes
            .iter()
            .map(|mode| math::finite_clamp(mode.gain, 0.0, 8.0, 0.0))
            .collect();
        Self {
            filters,
            gains,
            dry_gain: math::finite_clamp(dry_gain, 0.0, 2.0, 1.0),
        }
    }

    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        let input = math::snap_to_zero(input);
        let mut wet = 0.0;
        for (filter, &gain) in self.filters.iter_mut().zip(&self.gains) {
            wet += gain * filter.process(input);
        }
        math::snap_to_zero(self.dry_gain * input + wet)
    }

    pub(super) fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
    }
}

/// Mesh body voicing (M11 P4): a low metallic shell/air resonance and a broad high
/// "bridge-hill" formant, light gains so the mesh's own plate modes still dominate
/// — a cymbal/gong shell radiating.
const MESH_BODY_MODES: [BodyMode; 2] = [
    BodyMode {
        frequency_hz: 420.0,
        q: 5.0,
        gain: 0.35,
    },
    BodyMode {
        frequency_hz: 3_400.0,
        q: 2.5,
        gain: 0.5,
    },
];

/// Build the Mesh's one-way coloration body. The body is fixed (absolute formant
/// frequencies, like a real shell), so the mesh builds it once and only resets it.
pub(super) fn mesh_output_body(sample_rate: f32) -> OutputBody {
    OutputBody::new(sample_rate, &MESH_BODY_MODES, 1.0)
}

#[cfg(test)]
mod tests;
