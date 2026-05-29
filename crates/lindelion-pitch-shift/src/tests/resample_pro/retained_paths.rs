//! Guards for the explicitly-selectable Resample Pro paths exercised via the test-only config
//! setters: RTPGHI phase propagation (now the active default) and bin-level COG transient
//! handling (retained, inactive). Keeps both functional rather than silently bit-rotting.

use lindelion_dsp_utils::analysis::{fitted_sine_rms_error, rms};

use super::{analyze_resample_pro_cache, sine_wave};

#[test]
fn retained_rtpghi_path_reconstructs_stretched_sine() {
    // RTPGHI is the active phase-propagation path (RESAMPLE_PRO_PHASE_PROPAGATION); selected
    // explicitly here, it must time-stretch a pure tone without losing pitch or dissolving into
    // broadband noise. (The forward-difference frequency gradient is load-bearing here: the
    // centered-difference version aliased the main-lobe ±pi sign alternation to zero and pushed
    // this residual past 0.2.)
    let sample_rate = 48_000;
    let source_hz = 330.0;
    let stretch_ratio = 1.5;
    let audio = sine_wave(source_hz, sample_rate, sample_rate as usize / 2);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_hz, &[0]);
    let output_len = crate::resample_pro_stretch::stretched_output_len(audio.len(), stretch_ratio);
    let mut output = vec![0.0; output_len];
    let mut state =
        crate::resample_pro_stretch::ResampleProStretchState::new(&cache, output_len).unwrap();
    state.set_phase_propagation(crate::resample_pro_stretch::PhasePropagation::Rtpghi);
    state.render_to(&cache, stretch_ratio, &mut output).unwrap();

    let steady = &output[4_096..output.len() - 4_096];
    let estimated = lindelion_dsp_utils::analysis::estimate_frequency_zero_crossings(
        steady,
        sample_rate as f32,
    )
    .unwrap();
    let residual =
        fitted_sine_rms_error(steady, sample_rate as f32, source_hz) / rms(steady).max(1.0e-9);
    assert!(
        (estimated - source_hz).abs() < 2.0,
        "RTPGHI stretch should preserve pitch; got {estimated} Hz"
    );
    assert!(
        residual < 0.05,
        "RTPGHI stretch sine residual should stay clean; residual={residual}"
    );
}

#[test]
fn retained_bin_level_cog_transient_renders_cleanly() {
    // Bin-level COG transient handling is retained but not active (RESAMPLE_PRO_TRANSIENT_HANDLING
    // = WholeFrame). This is an anti-bit-rot guard: selected explicitly, the path must still
    // render a tone + marked transient finite and with the sustained tone intact (bounded
    // residual). It is *not* asserted to beat whole-frame reset — the real-fixture bake-off
    // showed bin-level COG is mixed and, at the active 87.5% overlap, no longer the cleaner
    // option (it collapses the attack crest on centred-transient frames; see the module docs).
    let sample_rate = 48_000;
    let f0 = 330.0;
    let len = sample_rate as usize / 2;
    let onset = 12_000usize;
    let mut audio = sine_wave(f0, sample_rate, len);
    audio[onset] += 1.0;
    let cache = analyze_resample_pro_cache(&audio, sample_rate, f0, &[0, onset]);
    let stretch = 1.5;
    let output_len = crate::resample_pro_stretch::stretched_output_len(audio.len(), stretch);

    let mut output = vec![0.0; output_len];
    let mut state =
        crate::resample_pro_stretch::ResampleProStretchState::new(&cache, output_len).unwrap();
    state.set_transient_handling(crate::resample_pro_stretch::TransientHandling::BinLevelCog);
    state.render_to(&cache, stretch, &mut output).unwrap();

    assert!(
        output.iter().all(|sample| sample.is_finite()),
        "bin-level COG transient render must stay finite"
    );
    let mapped = (onset as f64 * stretch) as usize;
    let region = &output[mapped.saturating_sub(4_096)..(mapped + 4_096).min(output.len())];
    let residual = fitted_sine_rms_error(region, sample_rate as f32, f0) / rms(region).max(1.0e-9);
    assert!(
        residual < 0.85,
        "bin-level COG should keep the sustained tone intact through the onset; residual={residual}"
    );
}
