// Heavy render-stability checks: each renders multi-second QA clips through the full
// ResonatorSynth. Gated behind the `integration-tests` feature (run via `make test-integration`)
// so they stay out of the fast `make ci` unit path. Included into the `plugin_tests` module, so
// helpers (`render_qa_clip`, `assert_all_finite`) resolve from the shared module scope.

#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn ableton_style_four_bar_clip_renders_inside_expected_bounds() {
    let rendered = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);

    assert_all_finite(&rendered.left);
    assert_all_finite(&rendered.right);
    assert!(
        rendered.rms > 0.000_01,
        "QA clip should render audible output"
    );
    assert!(
        rendered.peak < 8.0,
        "QA clip peak should remain bounded, peak={}",
        rendered.peak
    );
}

#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn render_clip_is_stable_across_buffer_sizes_and_sample_rates() {
    for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
        for block_size in [32, 64, 128, 512] {
            let rendered = render_qa_clip(sample_rate, block_size, ProcessMode::Realtime);

            assert_all_finite(&rendered.left);
            assert_all_finite(&rendered.right);
            assert!(
                rendered.rms > 0.000_001,
                "clip should not be silent at sample_rate={sample_rate}, block_size={block_size}"
            );
            assert!(
                rendered.peak < 8.0,
                "clip peak should stay bounded at sample_rate={sample_rate}, block_size={block_size}, peak={}",
                rendered.peak
            );
        }
    }
}

#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn offline_render_matches_realtime_for_fixed_clip() {
    let realtime = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);
    let offline = render_qa_clip(48_000.0, 128, ProcessMode::Offline);
    let mut max_diff = 0.0_f32;

    for (realtime_sample, offline_sample) in realtime
        .left
        .iter()
        .copied()
        .zip(offline.left.iter().copied())
    {
        max_diff = max_diff.max((realtime_sample - offline_sample).abs());
    }

    assert!(
        max_diff < 1.0e-6,
        "offline render should match realtime render, max diff {max_diff}"
    );
}
