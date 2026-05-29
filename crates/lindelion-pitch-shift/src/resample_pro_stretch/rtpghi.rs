//! RTPGHI phase-gradient heap integration for the Resample Pro time-stretch path.
//!
//! The **active** phase-propagation path (see [`super::RESAMPLE_PRO_PHASE_PROPAGATION`] for the
//! real-material bake-off that selected it over peak-locking at 87.5 % overlap). This module
//! holds the magnitude-ordered 2-D phase-gradient integration (Průša & Holighaus, "Phase Vocoder
//! Done Right"). It renders at setup time (off the audio thread; see ADR-0001 /
//! docs/performance.md), so heap allocation is permitted.

use std::cmp::Ordering;

use lindelion_dsp_utils::phase;

use super::{ResampleProCache, ResampleProStretchState};

/// Phase-propagation strategy for the variable-rate (time-stretch) path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhasePropagation {
    /// Laroche–Dolson identity peak phase-locking. Retained and selectable, but not the active
    /// default; flip [`RESAMPLE_PRO_PHASE_PROPAGATION`] to use.
    #[allow(dead_code)]
    PeakLocked,
    /// RTPGHI 2-D phase-gradient heap integration (Průša & Holighaus). Active default.
    Rtpghi,
}

/// Compile-time selection of the phase-propagation strategy.
///
/// RTPGHI is the active path. The choice was settled by re-running the bake-off on the **real**
/// fixture library at 87.5 % overlap — the synthetic battery sat at the inter-partial measurement
/// floor and could not discriminate. On real tonal/vocal material RTPGHI lowers the phasiness
/// floor vs peak-locking (e.g. cello −129 → −144 dB, sung vocal −68 → −73 dB) and, crucially,
/// **resolves the high-frequency-artifact regression that had kept it inactive at 75 % overlap**:
/// at +12 st it is now cleaner than peak-locking (sax −20.8 → −25.3 dB, sung vocal −14.2 →
/// −26.7 dB HF artifact). Peak-locking ([`ResampleProStretchState::propagate_phase_locked_frame`])
/// is retained and selectable by switching this constant.
pub(crate) const RESAMPLE_PRO_PHASE_PROPAGATION: PhasePropagation = PhasePropagation::Rtpghi;

/// Only bins at least this fraction of the frame's peak magnitude are eligible to be a
/// time-integration anchor. A windowed sinusoid has a local magnitude maximum at every
/// sidelobe; without this gate each one would time-integrate from its own (unreliable, low
/// SNR) instantaneous frequency and drift against the main lobe, smearing pure tones. Weaker
/// bins instead inherit phase by frequency integration from a significant anchor, which
/// carries the correct across-bin relationship (the ±π sidelobe sign) over the nulls between
/// lobes.
const RTPGHI_ANCHOR_RELATIVE: f64 = 1.0e-1;

/// Heap entry for magnitude-ordered phase-gradient integration. `prev_frame` marks an entry
/// to time-integrate from the previous frame; otherwise it frequency-integrates to neighbours.
#[derive(Debug, Clone, Copy)]
pub(super) struct HeapEntry {
    magnitude: f64,
    bin: u32,
    prev_frame: bool,
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.magnitude
            .total_cmp(&other.magnitude)
            .then(self.bin.cmp(&other.bin))
            .then(self.prev_frame.cmp(&other.prev_frame))
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for HeapEntry {}

impl ResampleProStretchState {
    /// Override the phase-propagation strategy. Test-only: production is governed solely by
    /// [`RESAMPLE_PRO_PHASE_PROPAGATION`]; this exercises the retained RTPGHI path.
    #[cfg(test)]
    pub(crate) fn set_phase_propagation(&mut self, propagation: PhasePropagation) {
        self.phase_propagation = propagation;
    }

    /// Propagate synthesis phase by RTPGHI (Průša & Holighaus). Instead of locking every bin
    /// to its region peak, integrate the 2-D phase gradient with a magnitude-ordered heap:
    /// the loudest bins anchor via time integration (instantaneous frequency, horizontal
    /// coherence) and their neighbourhoods fill via frequency integration of the across-bin
    /// gradient `∆_fφ` (vertical coherence), which peak-locking structurally discards.
    ///
    /// The active phase-propagation path (see [`super::RESAMPLE_PRO_PHASE_PROPAGATION`]).
    pub(super) fn propagate_rtpghi_frame(&mut self, cache: &ResampleProCache, reset_phase: bool) {
        let hop = cache.synthesis_hop.max(1) as f64;
        let bins = self.bin_phases.len();
        for bin in 0..bins {
            self.tgrad[bin] = self.inst_freqs[bin] * hop;
        }

        if reset_phase {
            self.bin_phases.copy_from_slice(&self.analysis_phases);
            self.prev_tgrad.copy_from_slice(&self.tgrad);
            self.prev_magnitudes.copy_from_slice(&self.magnitudes);
            return;
        }

        self.compute_frequency_gradient();

        let max_magnitude = self.magnitudes.iter().copied().fold(0.0_f64, f64::max);
        self.seed_time_anchors(bins, max_magnitude * RTPGHI_ANCHOR_RELATIVE);
        while let Some(entry) = self.heap.pop() {
            let bin = entry.bin as usize;
            if entry.prev_frame {
                self.try_time_anchor(bin);
            } else {
                self.integrate_frequency_up(bin, bins);
                self.integrate_frequency_down(bin);
            }
        }
        self.fill_isolated_bins(bins);

        self.bin_phases.copy_from_slice(&self.synth_phase);
        self.prev_tgrad.copy_from_slice(&self.tgrad);
        self.prev_magnitudes.copy_from_slice(&self.magnitudes);
    }

    /// Seed the heap with the significant peaks as time-anchor candidates. Sub-threshold bins
    /// are neither anchored nor barriers: frequency integration flows into them from a
    /// significant anchor, so the whole spectrum stays phase-coherent with its peaks.
    fn seed_time_anchors(&mut self, bins: usize, anchor_threshold: f64) {
        self.assigned.iter_mut().for_each(|flag| *flag = false);
        self.heap.clear();
        for bin in 0..bins {
            if self.magnitudes[bin] > anchor_threshold {
                self.heap.push(HeapEntry {
                    magnitude: self.prev_magnitudes[bin],
                    bin: bin as u32,
                    prev_frame: true,
                });
            }
        }
    }

    /// Anchor a bin via time integration (if not already frequency-assigned) and queue it as a
    /// frequency-integration source for its neighbours.
    fn try_time_anchor(&mut self, bin: usize) {
        if !self.assigned[bin] {
            self.synth_phase[bin] = self.time_integrated_phase(bin);
            self.assigned[bin] = true;
            self.push_current(bin);
        }
    }

    /// Frequency-integrate from `bin` up to `bin + 1`, preserving the across-bin analysis phase
    /// difference `∆_fφ`.
    fn integrate_frequency_up(&mut self, bin: usize, bins: usize) {
        let target = bin + 1;
        if target < bins && !self.assigned[target] {
            self.synth_phase[target] =
                phase::principal_angle(self.synth_phase[bin] + self.fgrad[bin]);
            self.assigned[target] = true;
            self.push_current(target);
        }
    }

    /// Frequency-integrate from `bin` down to `bin - 1`.
    fn integrate_frequency_down(&mut self, bin: usize) {
        if bin >= 1 && !self.assigned[bin - 1] {
            let target = bin - 1;
            self.synth_phase[target] =
                phase::principal_angle(self.synth_phase[bin] - self.fgrad[target]);
            self.assigned[target] = true;
            self.push_current(target);
        }
    }

    /// Queue a now-assigned bin as a current-frame frequency-integration source.
    fn push_current(&mut self, bin: usize) {
        self.heap.push(HeapEntry {
            magnitude: self.magnitudes[bin],
            bin: bin as u32,
            prev_frame: false,
        });
    }

    /// Genuinely isolated bins (no significant anchor reached them) fall back to plain per-bin
    /// time propagation.
    fn fill_isolated_bins(&mut self, bins: usize) {
        for bin in 0..bins {
            if !self.assigned[bin] {
                self.synth_phase[bin] = self.time_integrated_phase(bin);
            }
        }
    }

    /// Trapezoidal time integration of one bin: advance the previous synthesis phase by the
    /// average instantaneous frequency of the previous and current frames.
    fn time_integrated_phase(&self, bin: usize) -> f64 {
        phase::principal_angle(
            self.bin_phases[bin] + 0.5 * (self.prev_tgrad[bin] + self.tgrad[bin]),
        )
    }

    /// Frequency-direction phase gradient `∆_fφ`: the one-step forward difference of the
    /// analysis phase across bins. A centered difference would alias the ±π per-bin sign
    /// alternation of a windowed sinusoid's main lobe to zero; the forward difference
    /// reproduces every adjacent-bin relationship exactly, so frequency integration preserves
    /// the analysis vertical phase structure. `fgrad[k] = wrap(phase[k+1] - phase[k])`.
    ///
    /// Shared with the active peak-locking transient handling (M3), which reads `self.fgrad`
    /// as a per-bin local group delay.
    pub(super) fn compute_frequency_gradient(&mut self) {
        let bins = self.fgrad.len();
        for bin in 0..bins.saturating_sub(1) {
            self.fgrad[bin] =
                phase::principal_angle(self.analysis_phases[bin + 1] - self.analysis_phases[bin]);
        }
        if let Some(last) = self.fgrad.last_mut() {
            *last = 0.0;
        }
    }
}
