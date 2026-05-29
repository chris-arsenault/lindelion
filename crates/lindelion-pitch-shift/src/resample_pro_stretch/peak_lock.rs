//! Active peak-locking phase propagation and its bin-level transient handling (M3).

use lindelion_dsp_utils::phase;

use super::{ResampleProStretchState, peak_owner};
use crate::ResampleProCache;

/// How a frame's synthesis phase should be (re)initialised before propagation.
#[derive(Debug, Clone, Copy)]
pub(super) enum PhaseReset {
    /// No reset: continue propagating from the previous frame (steady state).
    None,
    /// Full-frame reset to the analysis phase (the first synthesis frame has no history).
    FullFrame,
    /// Transient frame: propagate steady bins, but reinitialise only the bins whose energy sits
    /// at or after the attack (bin-level COG), keeping sustained bins phase-coherent. The field
    /// is the attack position in samples relative to the analysis frame centre.
    Transient { attack_from_center: f64 },
}

impl PhaseReset {
    /// Whether any reset occurs. The RTPGHI path has no bin-level transient mode, so it treats a
    /// transient frame as a plain reset.
    pub(super) fn is_reset(self) -> bool {
        !matches!(self, PhaseReset::None)
    }
}

/// How a transient frame is reinitialised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransientHandling {
    /// Reset every bin to the analysis phase. Active default.
    WholeFrame,
    /// Bin-level COG reset (M3): reinitialise only the bins whose energy sits at or after the
    /// attack. Intentionally retained but not constructed by the active config.
    #[allow(dead_code)]
    BinLevelCog,
}

/// Compile-time selection of transient handling.
///
/// Whole-frame reset is the active path. The bin-level COG reset ([`ResampleProStretchState::reset_transient_bins`])
/// is implemented and validated but **intentionally not the default**: a bake-off found it cannot
/// beat whole-frame reset on this engine. The frame where a transient sits near the analysis
/// window centre is COG-inseparable (a sustained tone and the attack both have their energy
/// centred), so selective reset drops about half the attack's broadband bins and collapses the
/// onset crest factor (~7.9 → ~2.4), while any inclusive threshold reverts to whole-frame. It is
/// kept for future work on a better transient-bin criterion (e.g. cross-frame COG tracking) and
/// can be made active by switching this constant.
pub(super) const RESAMPLE_PRO_TRANSIENT_HANDLING: TransientHandling = TransientHandling::WholeFrame;

/// Bins must exceed this fraction of the frame's peak magnitude to be eligible for transient
/// reinitialisation, so group-delay noise in near-silent bins cannot trigger spurious resets.
const TRANSIENT_RESET_MAGNITUDE_FLOOR: f64 = 1.0e-3;

impl ResampleProStretchState {
    /// Override transient handling. Test-only: production is governed solely by
    /// [`RESAMPLE_PRO_TRANSIENT_HANDLING`]; this exercises the retained bin-level COG path.
    #[cfg(test)]
    pub(crate) fn set_transient_handling(&mut self, handling: TransientHandling) {
        self.transient_handling = handling;
    }

    /// Propagate synthesis phase by Laroche–Dolson identity peak phase-locking (active default).
    /// Peak bins advance by their instantaneous frequency; every other bin is rigidly locked to
    /// its region peak by the analysis phase offset, preserving intra-region coherence. A
    /// transient frame is reinitialised per [`Self::transient_handling`]: whole-frame reset
    /// (active) resets every bin; bin-level COG (retained, inactive) runs the steady propagation
    /// first and then reinitialises only the attack bins so sustained content stays coherent.
    pub(super) fn propagate_phase_locked_frame(
        &mut self,
        cache: &ResampleProCache,
        reset: PhaseReset,
    ) {
        let whole_frame_reset = matches!(reset, PhaseReset::FullFrame)
            || matches!(
                (reset, self.transient_handling),
                (PhaseReset::Transient { .. }, TransientHandling::WholeFrame)
            );
        if whole_frame_reset {
            self.bin_phases.copy_from_slice(&self.analysis_phases);
            self.peak_phases.copy_from_slice(&self.analysis_phases);
            return;
        }

        let hop = cache.synthesis_hop.max(1) as f64;
        for bin in 0..self.peak_phases.len() {
            if peak_owner(&self.peak_owner_by_bin, bin) == bin {
                self.peak_phases[bin] =
                    phase::principal_angle(self.peak_phases[bin] + self.inst_freqs[bin] * hop);
            }
        }

        for bin in 0..self.bin_phases.len() {
            let owner = peak_owner(&self.peak_owner_by_bin, bin);
            if owner >= self.bin_phases.len() {
                self.bin_phases[bin] =
                    phase::principal_angle(self.bin_phases[bin] + self.inst_freqs[bin] * hop);
                continue;
            }
            let local_offset =
                phase::principal_angle(self.analysis_phases[bin] - self.analysis_phases[owner]);
            self.bin_phases[bin] = phase::principal_angle(self.peak_phases[owner] + local_offset);
        }

        if let PhaseReset::Transient { attack_from_center } = reset {
            self.reset_transient_bins(cache, attack_from_center);
        }
    }

    /// Reinitialise the phase of bins whose energy sits at or after the attack, leaving steady
    /// bins on their propagated phase. Each bin's centre of gravity within the analysis frame is
    /// read from the frequency-direction phase gradient `∆_fφ` (local group delay): with the
    /// analysis window referenced to the frame start, an impulse at frame offset `n0` gives
    /// `∆_fφ = -2π·n0/N`, so demodulating the half-frame window ramp (`+π`) yields the COG
    /// relative to the frame centre. Reinitialising the attack bins to the analysis phase
    /// reconstructs a sharp onset while sustained bins stay coherent (Röbel, DAFx-03).
    fn reset_transient_bins(&mut self, cache: &ResampleProCache, attack_from_center: f64) {
        self.compute_frequency_gradient();
        let samples_per_radian = cache.fft_size as f64 / std::f64::consts::TAU;
        let magnitude_floor = self.magnitudes.iter().copied().fold(0.0_f64, f64::max)
            * TRANSIENT_RESET_MAGNITUDE_FLOOR;
        for bin in 0..self.bin_phases.len() {
            if self.magnitudes[bin] <= magnitude_floor {
                continue;
            }
            let centered_gradient = phase::principal_angle(self.fgrad[bin] + std::f64::consts::PI);
            let cog_from_center = -centered_gradient * samples_per_radian;
            if cog_from_center >= attack_from_center {
                self.bin_phases[bin] = self.analysis_phases[bin];
                self.peak_phases[bin] = self.analysis_phases[bin];
            }
        }
    }
}
