# Pitch-Shift Fidelity Techniques

Catalog of the DSP techniques evaluated for the Resample Pro pitch-shift / time-stretch engine
(`crates/lindelion-pitch-shift/`). Each technique is either active in the shipping engine,
implemented and retained behind a compile-time constant, or backlogged. Decision rationale and
the measured findings behind the active choices live in
[ADR-0010](../adr/0010-resample-pro-fidelity-strategy.md); the algorithm overview is in
[resample-pro.md](resample-pro.md); deferred work is in the
[workspace backlog](../backlog.md#pitch-shift-fidelity-resample-pro).

Measurement runs on two batteries plus crunch-validated metrics: a synthetic battery
(`tests/fidelity_battery.rs`) and a real-fixture battery (`tests/real_fixtures.rs`, over the
`testdata/audio/` library). On synthetic tonal material the inter-partial floor saturates near
−230 dB and cannot discriminate phase strategies, so the real fixtures are authoritative; the
inter-partial-floor metric is the reliable crunch detector (the `>6 kHz` ratio misses mid-band
roughness).

## Active

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| True-Envelope formant estimator | `spectral.rs` (`true_envelope_magnitudes`) | Iterative peak-riding envelope with Hamming lifter; replaced moving-RMS box. Locates formants accurately while holding the pure-tone residual / dominance contracts. |
| RTPGHI phase propagation | `resample_pro_stretch/rtpghi.rs` (`RESAMPLE_PRO_PHASE_PROPAGATION = Rtpghi`) | Magnitude-ordered 2-D phase-gradient heap integration. Active at 87.5 % overlap, where it lowers the phasiness floor and resolves the +12 st HF artifact ([ADR-0010](../adr/0010-resample-pro-fidelity-strategy.md)). |
| 87.5 % STFT overlap | `resample_pro_analysis.rs` (`OVERLAP_FACTOR = 8`) | `analysis_hop = fft_size / 8`. Lowers the inter-partial phasiness floor 8–15 dB vs 75 % on real tonal/vocal material; offline-only cost. |
| FFT window size 4096 | `PitchShiftAnalysisConfig::frame_size` | Time/frequency balance; larger windows help bass but halve time resolution. |
| Per-bin spectral-envelope formant gain | `synthesis_support.rs` (`spectral_envelope_formant_gain`) | Source/target envelope ratio per bin; shared by the spectral-peak and Resample Pro paths. |
| Whole-frame transient re-initialization + reposition | `resample_pro_stretch/peak_lock.rs`, `resample_pro_stretch/mod.rs` (`RESAMPLE_PRO_TRANSIENT_HANDLING = WholeFrame`) | Resets phase at marked transients and repositions the frame to the mapped output sample. |
| Direct-transient splice | `resample_pro_render.rs` (`apply_direct_transient_layer`) | Splices raw source over detected transients on the source-region render path. |
| Windowed-sinc resampler | `resample_pro_render.rs` resampler stage | 384-tap Blackman-Harris bandlimited resample after the phase-vocoder stretch. |
| Objective fidelity batteries | `tests/fidelity_battery.rs`, `tests/real_fixtures.rs` | Synthetic + real metric batteries (on-demand generators) and a real-material CI guard. |

## Retained, selectable (not active)

| Technique | Implementation | Note |
| ---- | ---- | ---- |
| Identity peak phase-locking | `resample_pro_stretch/peak_lock.rs` (`RESAMPLE_PRO_PHASE_PROPAGATION = PeakLocked`) | Laroche–Dolson region peak-locking, the prior default. Near-optimal on clean periodic tones; matched or beaten by RTPGHI on real material at 87.5 % overlap. |
| Bin-level COG transient handling | `resample_pro_stretch/peak_lock.rs` (`reset_transient_bins`; `RESAMPLE_PRO_TRANSIENT_HANDLING = BinLevelCog`) | Reinitializes only bins whose center of gravity sits at/after the attack. Modest real-material gain at 75 % overlap, reverses at the active 87.5 %; centered-transient frames are COG-inseparable. |

## Backlogged

These are evaluated-but-unbuilt or deferred-tier techniques. See the
[backlog](../backlog.md#pitch-shift-fidelity-resample-pro).

| Technique | Reason deferred |
| ---- | ---- |
| Multiresolution / dual-window STFT | The clearest remaining upside: a long window at low frequencies recovers the ~15 dB bass-resolution headroom that a single 4096 window trades away, without the transient cost of a globally larger FFT. |
| HPSS dual-path transient separation | An architectural transient approach for drums/mixes; no demonstrable transient gap on the available real material justified the cost. Requires a drum-kit fixture to evaluate. |
| Signalsmith weighted multi-prediction phase | An alternative phase-coherence method to RTPGHI; moot while RTPGHI is the active phase path and meets the contracts. |
| Noise morphing | Re-excites the stochastic component with fresh phase for breathy/metallic material; no evidence yet of a noise-quality problem on the real fixtures. |
| Drum-kit test fixture | Needed to evaluate transient-side techniques (bin-level COG, HPSS) and to harden the "no transient softening at 87.5 %" finding on percussive material. |
