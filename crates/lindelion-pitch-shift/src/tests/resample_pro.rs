use lindelion_dsp_utils::{
    analysis::{
        dft_magnitude_at, estimate_f0_autocorrelation, fitted_sine_rms_error,
        high_frequency_artifact_ratio, peak_abs, rms, rms_difference, sampled_high_frequency_ratio,
        windowed_dft_magnitude_at,
    },
    db_to_gain,
};
use lindelion_test_allocator::assert_no_allocations;

use super::{constant_pitch_contour, markers, sine_wave};
use crate::{PitchShiftAnalysisConfig, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios};

mod alias_rejection;
mod retained_paths;
mod transient_slice;
mod wind_fixture;

#[test]
fn resample_pro_cache_is_constructed_with_regular_stft_frames() {
    let sample_rate = 48_000;
    let audio = sine_wave(440.0, sample_rate, sample_rate as usize / 2);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, 440.0, &[0, 4_800]);
    let pro = &cache.resample_pro;

    assert_resample_pro_cache_shape(pro, sample_rate);
    assert_resample_pro_transient_frames(pro);
}

fn assert_resample_pro_cache_shape(pro: &crate::ResampleProCache, sample_rate: u32) {
    assert_resample_pro_config(pro, sample_rate);
    assert_resample_pro_window_shape(pro);
    assert_resample_pro_frame_shape(pro);
}

fn assert_resample_pro_config(pro: &crate::ResampleProCache, sample_rate: u32) {
    assert_eq!(pro.sample_rate, sample_rate);
    assert_eq!(pro.fft_size, 4096);
    assert_eq!(pro.analysis_hop, 512); // 87.5% overlap (fft_size / 8)
    assert_eq!(pro.synthesis_hop, 512);
}

fn assert_resample_pro_window_shape(pro: &crate::ResampleProCache) {
    assert_eq!(pro.window.len(), pro.fft_size);
}

fn assert_resample_pro_frame_shape(pro: &crate::ResampleProCache) {
    assert!(pro.frames.len() > 8);
    assert_eq!(pro.frames[0].magnitudes.len(), pro.bin_count());
    assert_eq!(pro.frames[0].phases.len(), pro.bin_count());
    assert_eq!(
        pro.frames[0].instantaneous_frequency_rad_per_sample.len(),
        pro.bin_count()
    );
    assert_eq!(pro.frames[0].peak_owner_by_bin.len(), pro.bin_count());
}

fn assert_resample_pro_transient_frames(pro: &crate::ResampleProCache) {
    assert!(pro.transient_frames.contains(&0));
    assert!(
        pro.transient_frames
            .iter()
            .any(|frame_index| *frame_index > 0)
    );
}

#[test]
fn resample_pro_cache_is_deterministic_for_fixed_inputs() {
    let sample_rate = 48_000;
    let audio = low_band_harmonic_stack(220.0, sample_rate, sample_rate as usize / 2);
    let first = analyze_resample_pro_cache(&audio, sample_rate, 220.0, &[0, 2_400]);
    let second = analyze_resample_pro_cache(&audio, sample_rate, 220.0, &[0, 2_400]);

    assert_eq!(first.resample_pro, second.resample_pro);
}

#[test]
fn resample_pro_cache_tracks_sine_instantaneous_frequency() {
    let sample_rate = 48_000;
    let source_hz = 440.0;
    let audio = sine_wave(source_hz, sample_rate, sample_rate as usize / 2);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_hz, &[0]);
    let frame = &cache.resample_pro.frames[2];
    let peak_bin = frame
        .magnitudes
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(bin, _)| bin)
        .unwrap();
    let estimated_hz = frame.instantaneous_frequency_rad_per_sample[peak_bin] * sample_rate as f64
        / std::f64::consts::TAU;

    assert!(
        (estimated_hz - source_hz as f64).abs() < 5.0,
        "estimated_hz={estimated_hz}"
    );
}

#[test]
fn resample_pro_unity_stretch_reconstructs_source() {
    let sample_rate = 48_000;
    let audio = low_band_harmonic_stack(220.0, sample_rate, sample_rate as usize);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, 220.0, &[0]);
    let reconstructed = PitchShiftEngine
        .render_resample_pro_unity_stretch(&cache)
        .unwrap();
    let trim = cache.resample_pro.fft_size.min(audio.len() / 4);
    let source = &audio[trim..audio.len() - trim];
    let rendered = &reconstructed[trim..reconstructed.len() - trim];
    let relative_error = rms_difference(source, rendered) / rms(source).max(1.0e-9);

    assert_eq!(reconstructed.len(), audio.len());
    assert!(
        relative_error <= db_to_gain(-70.0),
        "unity Resample Pro stretch should reconstruct below -70 dB after trimming; relative_error={relative_error}"
    );
    assert!(
        rendered.iter().all(|sample| sample.is_finite()),
        "unity Resample Pro stretch should only produce finite samples"
    );
}

#[test]
fn resample_pro_variable_stretch_preserves_sine_pitch() {
    let sample_rate = 48_000;
    let source_hz = 440.0;
    let stretch_ratio = 1.5;
    let audio = sine_wave(source_hz, sample_rate, sample_rate as usize);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_hz, &[0]);
    let rendered = PitchShiftEngine
        .render_resample_pro_stretch(&cache, stretch_ratio)
        .unwrap();
    let steady = steady_region(&rendered, cache.resample_pro.fft_size * 2);
    let f0 = estimate_f0_autocorrelation(steady, sample_rate as f32, 300.0, 600.0).unwrap();
    let fitted_error = fitted_sine_rms_error(steady, sample_rate as f32, source_hz);
    let relative_error = fitted_error / rms(steady).max(1.0e-9);

    assert_eq!(
        rendered.len(),
        crate::resample_pro_stretch::stretched_output_len(audio.len(), stretch_ratio)
    );
    assert!(
        (f0 - source_hz).abs() < 2.0,
        "variable stretch should preserve sine pitch; f0={f0}"
    );
    assert!(
        relative_error <= db_to_gain(-42.0),
        "variable stretch sine residual should stay below -42 dB; relative_error={relative_error}"
    );
}

#[test]
fn resample_pro_variable_stretch_preserves_harmonic_stack_pitch() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let stretch_ratio = 0.75;
    let audio = low_band_harmonic_stack(source_f0_hz, sample_rate, sample_rate as usize);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_f0_hz, &[0]);
    let rendered = PitchShiftEngine
        .render_resample_pro_stretch(&cache, stretch_ratio)
        .unwrap();
    let steady = steady_region(&rendered, cache.resample_pro.fft_size * 2);
    let f0 = estimate_f0_autocorrelation(steady, sample_rate as f32, 120.0, 320.0).unwrap();
    let preserved = dft_magnitude_at(steady, sample_rate as f32, source_f0_hz);
    let naive_time_scale_artifact = dft_magnitude_at(
        steady,
        sample_rate as f32,
        source_f0_hz / stretch_ratio as f32,
    );

    assert!(
        (f0 - source_f0_hz).abs() < 4.0,
        "variable stretch should preserve harmonic-stack pitch; f0={f0}"
    );
    assert!(
        preserved > naive_time_scale_artifact * 3.0,
        "phase-vocoder stretch should not behave like naive resampling; preserved={preserved}, artifact={naive_time_scale_artifact}"
    );
}

#[test]
fn resample_pro_variable_stretch_resets_transient_phase() {
    let sample_rate = 48_000;
    let impulse_position = 12_000usize;
    let stretch_ratio = 1.5;
    let mut audio = vec![0.0; sample_rate as usize];
    audio[impulse_position] = 1.0;
    let cache = analyze_resample_pro_cache(&audio, sample_rate, 220.0, &[0, impulse_position]);
    let rendered = PitchShiftEngine
        .render_resample_pro_stretch(&cache, stretch_ratio)
        .unwrap();
    let expected = (impulse_position as f64 * stretch_ratio).round() as usize;
    let peak_index = peak_abs_index(&rendered).unwrap();
    let timing_error = peak_index.abs_diff(expected);
    let peak = peak_abs(&rendered);

    assert!(
        timing_error <= cache.resample_pro.synthesis_hop,
        "transient phase reset should keep stretched impulse near expected position; expected={expected}, peak_index={peak_index}, timing_error={timing_error}"
    );
    assert!(
        peak > 0.1,
        "stretched impulse peak should survive; peak={peak}"
    );
}

#[test]
fn resample_pro_variable_stretch_state_renders_without_allocating_after_initialization() {
    let sample_rate = 48_000;
    let source_hz = 330.0;
    let stretch_ratio = 1.25;
    let audio = sine_wave(source_hz, sample_rate, sample_rate as usize / 2);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_hz, &[0]);
    let output_len = crate::resample_pro_stretch::stretched_output_len(audio.len(), stretch_ratio);
    let mut output = vec![0.0; output_len];
    let mut state =
        crate::resample_pro_stretch::ResampleProStretchState::new(&cache, output_len).unwrap();

    assert_no_allocations("resample pro variable stretch render_to", || {
        state.render_to(&cache, stretch_ratio, &mut output).unwrap();
    });
}

#[test]
fn resample_pro_one_cent_sine_has_no_broadband_crunch() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(0.0, 1.0).pitch_ratio;
    let audio = sine_wave(source_f0_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro(&audio, sample_rate, source_f0_hz, pitch_ratio);
    let target_hz = source_f0_hz * pitch_ratio;
    let steady = steady_region(&rendered, 4_096);
    let fitted_error = fitted_sine_rms_error(steady, sample_rate as f32, target_hz);
    let relative_error = fitted_error / rms(steady).max(1.0e-9);
    let high_artifact_ratio = high_frequency_artifact_ratio(steady, sample_rate as f32, target_hz);

    assert!(
        relative_error <= db_to_gain(-55.0),
        "1-cent Resample Pro sine residual should be below -55 dB; relative_error={relative_error}"
    );
    assert!(
        high_artifact_ratio <= db_to_gain(-70.0),
        "1-cent Resample Pro sine high-frequency artifact ratio should be below -70 dB; ratio={high_artifact_ratio}"
    );
}

#[test]
fn resample_pro_one_cent_harmonic_stack_does_not_invent_high_band_energy() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(0.0, 1.0).pitch_ratio;
    let audio = low_band_harmonic_stack(source_f0_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro(&audio, sample_rate, source_f0_hz, pitch_ratio);
    let source_high = sampled_high_frequency_ratio(
        steady_region(&audio, 4_096),
        sample_rate as f32,
        6_000.0,
        100.0,
    );
    let rendered_high = sampled_high_frequency_ratio(
        steady_region(&rendered, 4_096),
        sample_rate as f32,
        6_000.0,
        100.0,
    );

    assert!(
        rendered_high <= source_high * 2.0 + 1.0e-6,
        "1-cent Resample Pro harmonic stack should not create high-band energy; source={source_high}, rendered={rendered_high}"
    );
}

#[test]
fn resample_pro_one_cent_track_formant_does_not_invent_high_band_energy() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(0.0, 1.0).pitch_ratio;
    let audio = low_band_harmonic_stack(source_f0_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro_with_ratios(
        &audio,
        sample_rate,
        source_f0_hz,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: Some(pitch_ratio),
        },
    );
    let source_high = sampled_high_frequency_ratio(
        steady_region(&audio, 4_096),
        sample_rate as f32,
        6_000.0,
        100.0,
    );
    let rendered_high = sampled_high_frequency_ratio(
        steady_region(&rendered, 4_096),
        sample_rate as f32,
        6_000.0,
        100.0,
    );

    assert!(
        rendered_high <= source_high * 2.0 + 1.0e-6,
        "1-cent Resample Pro track-formant harmonic stack should not create high-band energy; source={source_high}, rendered={rendered_high}"
    );
}

#[test]
fn resample_pro_fifty_cent_shift_suppresses_original_pitch_leakage() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(0.0, 50.0).pitch_ratio;
    let audio = low_band_harmonic_stack(source_f0_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro(&audio, sample_rate, source_f0_hz, pitch_ratio);
    let steady = steady_region(&rendered, 4_096);
    let target_hz = source_f0_hz * pitch_ratio;
    let target = windowed_dft_magnitude_at(steady, sample_rate as f32, target_hz);
    let leaked_source = windowed_dft_magnitude_at(steady, sample_rate as f32, source_f0_hz);

    assert!(
        leaked_source <= target * db_to_gain(-40.0),
        "Resample Pro should keep original-pitch leakage at least 40 dB below shifted target; target={target}, leaked={leaked_source}"
    );
}

#[test]
fn resample_pro_formant_preserve_keeps_vowel_envelope() {
    let sample_rate = 48_000;
    let source_f0_hz = 110.0;
    let formant_hz = 1_000.0;
    let pitch_ratio = 2.0;
    let audio =
        harmonic_stack_with_formant(source_f0_hz, formant_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro_with_ratios(
        &audio,
        sample_rate,
        source_f0_hz,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: None,
        },
    );
    let peak = rendered_envelope_peak_hz(&rendered, sample_rate, source_f0_hz * pitch_ratio);

    assert!(
        (peak - formant_hz).abs() < 350.0,
        "preserve mode should keep formant near original frequency; peak={peak}"
    );
}

#[test]
fn resample_pro_formant_track_moves_vowel_envelope_with_pitch() {
    let sample_rate = 48_000;
    let source_f0_hz = 110.0;
    let formant_hz = 900.0;
    let pitch_ratio = 2.0;
    let audio =
        harmonic_stack_with_formant(source_f0_hz, formant_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro_with_ratios(
        &audio,
        sample_rate,
        source_f0_hz,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: Some(pitch_ratio),
        },
    );
    let peak = rendered_envelope_peak_hz(&rendered, sample_rate, source_f0_hz * pitch_ratio);

    assert!(
        (peak - formant_hz * pitch_ratio).abs() < 500.0,
        "track mode should move formant with pitch; peak={peak}"
    );
}

#[test]
fn resample_pro_seven_semitone_shift_hits_target_pitch() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(7.0, 0.0).pitch_ratio;
    let audio = sine_wave(source_f0_hz, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro(&audio, sample_rate, source_f0_hz, pitch_ratio);
    let target_hz = source_f0_hz * pitch_ratio;
    let steady = steady_region(&rendered, 8_192);
    let f0 = estimate_f0_autocorrelation(steady, sample_rate as f32, 300.0, 400.0).unwrap();
    let fitted_error = fitted_sine_rms_error(steady, sample_rate as f32, target_hz);
    let relative_error = fitted_error / rms(steady).max(1.0e-9);

    assert!(
        (f0 - target_hz).abs() < 3.0,
        "7-semitone Resample Pro shift should hit target pitch; target={target_hz}, f0={f0}"
    );
    assert!(
        relative_error <= db_to_gain(-45.0),
        "7-semitone Resample Pro sine residual should stay below -45 dB; relative_error={relative_error}"
    );
}

#[test]
fn resample_pro_octave_up_rejects_unshiftable_high_frequency_input() {
    let sample_rate = 48_000;
    let source_f0_hz = 440.0;
    let audio = sine_wave(16_000.0, sample_rate, sample_rate as usize);
    let rendered = render_resample_pro(&audio, sample_rate, source_f0_hz, 2.0);
    let source_level = rms(steady_region(&audio, 4_096));
    let rendered_level = rms(steady_region(&rendered, 4_096));

    assert!(
        rendered_level <= source_level * db_to_gain(-40.0),
        "octave-up Resample Pro should reject input that would shift above Nyquist; source={source_level}, rendered={rendered_level}"
    );
}

#[test]
fn resample_pro_pitch_shift_state_renders_without_allocating_after_initialization() {
    let sample_rate = 48_000;
    let source_hz = 220.0;
    let ratios = PitchShiftRatios::from_semitones_cents(0.0, 50.0);
    let audio = sine_wave(source_hz, sample_rate, sample_rate as usize / 2);
    let cache = analyze_resample_pro_cache(&audio, sample_rate, source_hz, &[0]);
    let mut output = vec![0.0; audio.len()];
    let mut state =
        crate::resample_pro_render::ResampleProRenderState::new(&cache, ratios.pitch_ratio as f64)
            .unwrap();

    assert_no_allocations("resample pro pitch-shift render_to", || {
        state
            .render_pitch_shift_to(&cache, ratios, &mut output)
            .unwrap();
    });
}

fn render_resample_pro(
    audio: &[f32],
    sample_rate: u32,
    source_f0_hz: f32,
    pitch_ratio: f32,
) -> Vec<f32> {
    render_resample_pro_with_ratios(
        audio,
        sample_rate,
        source_f0_hz,
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: None,
        },
    )
}

fn render_resample_pro_with_ratios(
    audio: &[f32],
    sample_rate: u32,
    source_f0_hz: f32,
    ratios: PitchShiftRatios,
) -> Vec<f32> {
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();

    PitchShiftEngine
        .render_resample_pro_pitch_shift(&source_cache, ratios)
        .unwrap()
}

fn analyze_resample_pro_cache(
    audio: &[f32],
    sample_rate: u32,
    source_f0_hz: f32,
    marker_positions: &[usize],
) -> crate::PitchShiftSourceCache {
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(audio, sample_rate, &contour, &markers(marker_positions))
    .unwrap()
}

fn low_band_harmonic_stack(f0_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (1..=8)
                .map(|harmonic| {
                    let frequency = f0_hz * harmonic as f32;
                    let phase =
                        std::f32::consts::TAU * frequency * index as f32 / sample_rate as f32;
                    phase.sin() * 0.5 / harmonic as f32
                })
                .sum::<f32>()
        })
        .collect()
}

fn harmonic_stack_with_formant(
    f0_hz: f32,
    formant_hz: f32,
    sample_rate: u32,
    len: usize,
) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (1..24)
                .map(|harmonic| {
                    let frequency = f0_hz * harmonic as f32;
                    let formant_gain = gaussian(frequency, formant_hz, 180.0);
                    let phase =
                        std::f32::consts::TAU * frequency * index as f32 / sample_rate as f32;
                    phase.sin() * formant_gain
                })
                .sum::<f32>()
                * 0.15
        })
        .collect()
}

fn rendered_envelope_peak_hz(rendered: &[f32], sample_rate: u32, f0_hz: f32) -> f32 {
    let contour = constant_pitch_contour(sample_rate, f0_hz, rendered.len());
    let cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(rendered, sample_rate, &contour, &markers(&[0]))
    .unwrap();
    envelope_peak_hz(&cache.frames[2].spectral_envelope)
}

fn envelope_peak_hz(envelope: &crate::SpectralEnvelope) -> f32 {
    envelope
        .points
        .iter()
        .max_by(|left, right| left.magnitude.total_cmp(&right.magnitude))
        .map(|point| point.frequency_hz)
        .unwrap_or(0.0)
}

fn gaussian(value: f32, center: f32, width: f32) -> f32 {
    let normalized = (value - center) / width;
    (-0.5 * normalized * normalized).exp()
}

fn steady_region(samples: &[f32], trim: usize) -> &[f32] {
    let trim = trim.min(samples.len() / 4);
    &samples[trim..samples.len() - trim]
}

fn peak_abs_index(samples: &[f32]) -> Option<usize> {
    samples
        .iter()
        .copied()
        .enumerate()
        .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
        .map(|(index, _)| index)
}
