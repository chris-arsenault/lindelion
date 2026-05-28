use lindelion_dsp_utils::analysis::{
    estimate_frequency_zero_crossings, fitted_sine_rms_error, high_frequency_artifact_ratio,
    max_adjacent_delta,
};
use lindelion_plugin_shell::{ControlEvent, MidiEvent};

use super::{RuntimeFixture, note_on};
use crate::{
    LinnodPatch, PitchOffset,
    patch::{PitchShiftAlgorithm, PlaybackMode},
};

#[test]
fn pad_mode_one_cent_pitch_offset_keeps_sine_clean() {
    let mut fixture = RuntimeFixture::new();
    let mut identity_left = [0.0; 4_800];
    let mut identity_right = [0.0; 4_800];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut identity_left,
        &mut identity_right,
    );

    let mut fixture = RuntimeFixture::new();
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    let mut left = [0.0; 4_800];
    let mut right = [0.0; 4_800];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    let steady = &left[512..4_608];
    let expected_hz = 220.0
        * (PitchOffset {
            semitones: 0,
            cents: 1.0,
        })
        .ratio();
    let identity_steady = &identity_left[512..4_608];
    let estimated_hz = estimate_frequency_zero_crossings(steady, 48_000.0).unwrap();
    let baseline_error = fitted_sine_rms_error(identity_steady, 48_000.0, 220.0);
    let fitted_error = fitted_sine_rms_error(steady, 48_000.0, expected_hz);
    let baseline_high_artifact_ratio =
        high_frequency_artifact_ratio(identity_steady, 48_000.0, 220.0);
    let high_artifact_ratio = high_frequency_artifact_ratio(steady, 48_000.0, expected_hz);

    assert!(
        (estimated_hz - expected_hz).abs() < 0.5,
        "expected {expected_hz:.3} Hz, got {estimated_hz:.3} Hz"
    );
    assert!(
        fitted_error <= baseline_error + 0.002,
        "1-cent pad output should not add sine distortion beyond identity playback; baseline={baseline_error}, shifted={fitted_error}"
    );
    assert!(
        high_artifact_ratio <= baseline_high_artifact_ratio + 0.002,
        "1-cent pad output added high-frequency artifact ratio; baseline={baseline_high_artifact_ratio}, shifted={high_artifact_ratio}"
    );
}

#[test]
fn looped_pad_one_cent_pitch_offset_does_not_click_at_wrap() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.playback.mode = PlaybackMode::Looped;
    let mut identity_left = [0.0; 9_600];
    let mut identity_right = [0.0; 9_600];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut identity_left,
        &mut identity_right,
    );

    let mut fixture = RuntimeFixture::new();
    fixture.patch.playback.mode = PlaybackMode::Looped;
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    let mut left = [0.0; 9_600];
    let mut right = [0.0; 9_600];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    let baseline_jump = max_adjacent_delta(&identity_left);
    let shifted_jump = max_adjacent_delta(&left);

    assert!(
        shifted_jump <= baseline_jump * 1.2 + 0.002,
        "1-cent pitch-shifted loop should not add a click-sized discontinuity; baseline={baseline_jump}, shifted={shifted_jump}"
    );
}

#[test]
fn resample_stretch_one_cent_pitch_offset_keeps_sine_clean() {
    let identity_left = render_resample_stretch_sine(
        PitchOffset {
            semitones: 0,
            cents: 0.0,
        },
        48_000,
    );
    let left = render_resample_stretch_sine(
        PitchOffset {
            semitones: 0,
            cents: 1.0,
        },
        48_000,
    );
    let pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    let expected_hz = 220.0 * pitch.ratio();
    let identity_steady = &identity_left[8_192..40_960];
    let steady = &left[8_192..40_960];
    let estimated_hz = estimate_frequency_zero_crossings(steady, 48_000.0).unwrap();
    let baseline_error = fitted_sine_rms_error(identity_steady, 48_000.0, 220.0);
    let fitted_error = fitted_sine_rms_error(steady, 48_000.0, expected_hz);
    let baseline_high_artifact_ratio =
        high_frequency_artifact_ratio(identity_steady, 48_000.0, 220.0);
    let high_artifact_ratio = high_frequency_artifact_ratio(steady, 48_000.0, expected_hz);

    assert!(
        (estimated_hz - expected_hz).abs() < 0.5,
        "expected {expected_hz:.3} Hz, got {estimated_hz:.3} Hz"
    );
    assert!(
        fitted_error <= baseline_error + 0.004,
        "Resample Stretch 1-cent sine residual should stay near identity playback; baseline={baseline_error}, shifted={fitted_error}"
    );
    assert!(
        high_artifact_ratio <= baseline_high_artifact_ratio + 0.001,
        "Resample Stretch 1-cent sine output added high-frequency artifacts; baseline={baseline_high_artifact_ratio}, shifted={high_artifact_ratio}"
    );
}

#[test]
fn resample_stretch_pitch_bend_ramp_stays_bounded() {
    let mut fixture = sine_fixture(16_384);
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.patch.playback.mode = PlaybackMode::Looped;
    fixture.prepare_current_patch();
    let mut rendered = Vec::new();

    for block_index in 0..24 {
        let bend = -1.0 + (2.0 * block_index as f32 / 23.0);
        let bend_event = MidiEvent::Control(ControlEvent::PitchBend {
            channel: 0,
            semitones: bend,
        });
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        if block_index == 0 {
            fixture.processor.process(
                &fixture.patch,
                Some(&fixture.analysis),
                &[note_on(0, 36, 1.0), bend_event],
                &mut left,
                &mut right,
            );
        } else {
            fixture.processor.process(
                &fixture.patch,
                Some(&fixture.analysis),
                &[bend_event],
                &mut left,
                &mut right,
            );
        }
        rendered.extend_from_slice(&left);
    }

    let steady = &rendered[512..];
    let peak = super::peak_abs(steady);
    let max_delta = max_adjacent_delta(steady);

    assert!(peak > 0.05, "pitch-bend ramp should keep audible output");
    assert!(
        max_delta <= peak * 0.6 + 0.03,
        "Resample Stretch pitch-bend ramp should not create a click-sized jump; peak={peak}, max_delta={max_delta}"
    );
}

fn render_resample_stretch_sine(pitch: PitchOffset, len: usize) -> Vec<f32> {
    let mut fixture = sine_fixture(len);
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.patch.slices[0].pitch = pitch;
    fixture.prepare_current_patch();
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    left
}

fn sine_fixture(len: usize) -> RuntimeFixture {
    let sample_rate = 48_000;
    let analysis = super::source_analysis_from_samples(
        super::sine_wave(220.0, sample_rate, len),
        sample_rate,
        vec![lindelion_onset_detect::SliceMarker {
            position_samples: 0,
            kind: lindelion_onset_detect::MarkerKind::Auto,
        }],
        220.0,
        "long_sine.wav",
    );
    let patch = LinnodPatch::default();
    let mut processor = super::LinnodProcessor::new(sample_rate as f32);
    processor.prepare_source_analysis(&patch, &analysis);
    RuntimeFixture {
        processor,
        patch,
        analysis,
    }
}
