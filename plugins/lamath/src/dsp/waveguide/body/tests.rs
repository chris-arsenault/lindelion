use super::*;
use lindelion_dsp_utils::analysis::{assert_all_finite, audio_window_metrics, rms_difference};

#[test]
fn body_renders_finite_decaying_impulse() {
    let output = render_body(WaveguideParams::default(), 2_048);

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output[0..512], 48_000.0).rms > 0.000_001);
    assert!(audio_window_metrics(&output[1_536..], 48_000.0).peak_abs < 0.01);
}

#[test]
fn body_profiles_make_string_and_tube_spectrally_distinct() {
    let string = render_body(
        WaveguideParams {
            style: WaveguideStyle::String,
            frequency_hz: 220.0,
            loop_filter_cutoff: 12_000.0,
            ..WaveguideParams::default()
        },
        4_096,
    );
    let tube = render_body(
        WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz: 220.0,
            loop_filter_cutoff: 12_000.0,
            boundary_reflection: 0.8,
            ..WaveguideParams::default()
        },
        4_096,
    );
    let string_centroid = audio_window_metrics(&string[0..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let tube_centroid = audio_window_metrics(&tube[0..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();

    assert_all_finite(&string);
    assert_all_finite(&tube);
    assert!(rms_difference(&string, &tube) > 0.000_001);
    assert!((string_centroid - tube_centroid).abs() > 20.0);
}

#[test]
fn body_reset_clears_filter_state() {
    let mut body = WaveguideBody::new(48_000.0);
    for index in 0..512 {
        body.process_sample((index == 0) as u8 as f32, WaveguideParams::default());
    }
    body.reset();

    let output = (0..256)
        .map(|_| body.process_sample(0.0, WaveguideParams::default()))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output, 48_000.0).peak_abs < 0.000_001);
}

#[test]
fn body_profile_derived_once_for_constant_params() {
    let mut body = WaveguideBody::new(48_000.0);
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: 196.0,
        loop_filter_cutoff: 9_000.0,
        ..WaveguideParams::default()
    };

    for index in 0..1_024 {
        body.process_sample((index == 0) as u8 as f32, params);
    }

    assert_eq!(
        body.recompute_count, 1,
        "body profile recomputed per sample"
    );
}

#[test]
fn body_profile_recomputes_when_params_move() {
    let mut body = WaveguideBody::new(48_000.0);
    let base = WaveguideParams {
        style: WaveguideStyle::String,
        loop_filter_cutoff: 9_000.0,
        ..WaveguideParams::default()
    };
    let moved = WaveguideParams {
        loop_filter_cutoff: 4_000.0,
        ..base
    };

    for _ in 0..16 {
        body.process_sample(0.0, base);
    }
    for _ in 0..16 {
        body.process_sample(0.0, moved);
    }

    assert_eq!(body.recompute_count, 2);
}

fn render_body(params: WaveguideParams, sample_count: usize) -> Vec<f32> {
    let mut body = WaveguideBody::new(48_000.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        output.push(body.process_sample((index == 0) as u8 as f32, params));
    }
    output
}

fn render_reduced_body(family: BodyFamily, sample_count: usize) -> Vec<f32> {
    let mut body = ReducedBody::new(48_000.0, family);
    (0..sample_count)
        .map(|index| body.process_sample((index == 0) as u8 as f32))
        .collect()
}

#[test]
fn reduced_body_families_are_spectrally_distinct() {
    let guitar = render_reduced_body(BodyFamily::Guitar, 8_192);
    let violin = render_reduced_body(BodyFamily::Violin, 8_192);

    assert_all_finite(&guitar);
    assert_all_finite(&violin);
    assert!(rms_difference(&guitar, &violin) > 0.000_01);

    // In the strong modal ringing window (after the broadband impulse, before
    // the level drops into numerical noise) the violin's higher signature modes
    // put its energy distinctly higher than the guitar's air/plate-dominated body.
    let guitar_centroid = audio_window_metrics(&guitar[256..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let violin_centroid = audio_window_metrics(&violin[256..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    assert!(
        violin_centroid > guitar_centroid + 50.0,
        "guitar={guitar_centroid} violin={violin_centroid}"
    );
}

#[test]
fn reduced_body_is_finite_and_decays() {
    for family in [BodyFamily::Guitar, BodyFamily::Violin] {
        // An impulse-driven body rings down (its modes decay). A physical body's
        // air/signature modes ring for tens to hundreds of ms, so check a
        // relative decay over a long render rather than an absolute floor.
        let mut body = ReducedBody::new(48_000.0, family);
        let output: Vec<f32> = (0..16_384)
            .map(|index| body.process_sample((index == 0) as u8 as f32))
            .collect();
        assert_all_finite(&output);
        let early = audio_window_metrics(&output[0..1_024], 48_000.0).rms;
        let late = audio_window_metrics(&output[12_288..], 48_000.0).rms;
        assert!(early > 1.0e-6);
        assert!(
            late < early * 0.2,
            "{family:?} did not decay: early={early} late={late}"
        );
    }
}
