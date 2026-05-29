use super::*;

#[test]
fn superflux_adaptive_threshold_detects_quiet_onset_before_loud_onsets() {
    // Three quiet bursts followed by three loud bursts. A single global mean+std
    // threshold is dominated by the loud bursts and misses the quiet ones; the
    // causal local moving-window threshold detects both.
    let audio = ramped_burst_signal();
    let detector = SuperFluxDetector;
    let config = DetectionConfig::superflux(0.5, 80.0, OnsetDetectionProfile::default());

    let markers = detector.detect(OnsetDetectionInput::new(&audio, 48_000), config);

    let first_quiet_burst = 7_200i64; // 150 ms gap at 48 kHz
    let tolerance = 4_800i64; // 100 ms
    assert!(
        markers
            .iter()
            .any(|marker| (marker.position_samples as i64 - first_quiet_burst).abs() < tolerance),
        "expected an onset near the first quiet burst at {first_quiet_burst}; markers={:?}",
        markers
            .iter()
            .map(|marker| marker.position_samples)
            .collect::<Vec<_>>()
    );
}

#[test]
fn superflux_batch_and_streaming_agree() {
    // Steps 3+4: batch and streaming share one scale-invariant local threshold,
    // so identical audio yields the same onset positions (within a small
    // tolerance for the batch-only dedupe pass).
    let audio = ramped_burst_signal();
    let config = DetectionConfig::superflux(0.5, 80.0, OnsetDetectionProfile::default());

    let mut batch: Vec<usize> = SuperFluxDetector
        .detect(OnsetDetectionInput::new(&audio, 48_000), config)
        .iter()
        .map(|marker| marker.position_samples)
        .collect();

    let mut streaming_detector = StreamingSuperFluxDetector::new(48_000, config);
    let mut streaming = Vec::new();
    for block in audio.chunks(1_000) {
        streaming.extend(
            streaming_detector
                .next_block(block)
                .iter()
                .map(|marker| marker.position_samples),
        );
    }
    streaming.extend(
        streaming_detector
            .finish()
            .iter()
            .map(|marker| marker.position_samples),
    );

    batch.sort_unstable();
    streaming.sort_unstable();
    assert_eq!(
        batch.len(),
        streaming.len(),
        "batch and streaming marker counts differ: batch={batch:?}, streaming={streaming:?}"
    );
    for (batch_position, streaming_position) in batch.iter().zip(streaming.iter()) {
        assert!(
            batch_position.abs_diff(*streaming_position) <= 256,
            "batch and streaming markers diverge: batch={batch:?}, streaming={streaming:?}"
        );
    }
}

#[test]
fn complex_flux_novelty_independent_of_group_delay_weight() {
    // Step 5: the complex-domain ODF is group_delay_weight * sum(|X_t - X_pred|),
    // so the normalized novelty is independent of the weight. The previous code
    // added an unweighted half-wave magnitude-flux term on top (double-counting
    // the magnitude deviation already inside the complex distance), which made the
    // normalized shape depend on the weight.
    let audio = amplitude_then_phase_event(48_000);
    let low = crate::spectral_flux::complex_flux(&audio, 1024, 256, 1.0);
    let high = crate::spectral_flux::complex_flux(&audio, 1024, 256, 8.0);

    assert_eq!(low.len(), high.len());
    let max_diff = low
        .iter()
        .zip(high.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_diff < 1e-4,
        "complex-flux novelty depends on group_delay_weight (max diff {max_diff}); the magnitude term is double-counted"
    );
}

fn ramped_burst_signal() -> Vec<f32> {
    let gap_samples = 7_200; // 150 ms at 48 kHz
    let burst_samples = 2_400; // 50 ms
    let amplitudes = [0.08, 0.08, 0.08, 1.0, 1.0, 1.0];
    let mut audio = Vec::new();
    for amplitude in amplitudes {
        audio.resize(audio.len() + gap_samples, 0.0);
        for index in 0..burst_samples {
            let phase = std::f32::consts::TAU * 440.0 * index as f32 / 48_000.0;
            audio.push(phase.sin() * amplitude);
        }
    }
    audio
}

fn amplitude_then_phase_event(sample_rate: u32) -> Vec<f32> {
    let dt = std::f32::consts::TAU * 440.0 / sample_rate as f32;
    let mut audio = Vec::new();
    let mut phase = 0.0f32;
    // Steady quiet sine.
    for _ in 0..9_600 {
        audio.push(phase.sin() * 0.2);
        phase += dt;
    }
    // Amplitude jump: magnitude-dominated event, phase continuous.
    for _ in 0..9_600 {
        audio.push(phase.sin() * 0.9);
        phase += dt;
    }
    // Phase reset: phase-dominated event, amplitude unchanged.
    phase += std::f32::consts::PI;
    for _ in 0..9_600 {
        audio.push(phase.sin() * 0.9);
        phase += dt;
    }
    audio
}
