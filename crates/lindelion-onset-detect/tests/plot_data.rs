//! Export onset-detection demo CSVs used by tools/dsp-plot.

use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use lindelion_onset_detect::{
    ConfiguredOnsetDetector, DetectionAlgorithm, DetectionConfig, OnsetDetectionInput,
    OnsetDetector,
};

const SAMPLE_RATE_U32: u32 = 48_000;
const SAMPLE_RATE: f32 = 48_000.0;

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root")
        .join("docs")
        .join("plots")
        .join("data")
}

#[test]
fn export_onset_detection_demo() {
    let dir = output_dir();
    create_dir_all(&dir).expect("create data dir");

    let duration_seconds = 1.0_f32;
    let n_samples = (duration_seconds * SAMPLE_RATE) as usize;
    let mut audio = vec![0.0_f32; n_samples];

    let burst_duration_seconds = 0.06_f32;
    let burst_samples = (burst_duration_seconds * SAMPLE_RATE) as usize;
    let burst_starts = [0.1_f32, 0.3, 0.55, 0.85];
    let frequency_hz = 200.0_f32;

    for &start_s in &burst_starts {
        let start_idx = (start_s * SAMPLE_RATE) as usize;
        for i in 0..burst_samples {
            let t = i as f32 / SAMPLE_RATE;
            let normalized_t = t / burst_duration_seconds;
            let envelope = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * normalized_t).cos());
            let phase = 2.0 * std::f32::consts::PI * frequency_hz * t;
            let target = start_idx + i;
            if target < n_samples {
                audio[target] = envelope * phase.sin() * 0.95;
            }
        }
    }

    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::EnergyTransient,
        sensitivity: 0.7,
        min_slice_ms: 50.0,
        ..DetectionConfig::default()
    };
    let input = OnsetDetectionInput::new(&audio, SAMPLE_RATE_U32);
    let markers = ConfiguredOnsetDetector.detect(input, config);

    let decimation = 12_usize;
    let mut signal_file = File::create(dir.join("onset_signal.csv")).expect("create csv");
    writeln!(signal_file, "time_s,value").unwrap();
    for i in (0..n_samples).step_by(decimation) {
        let t = i as f32 / SAMPLE_RATE;
        writeln!(signal_file, "{:.6},{:.6}", t, audio[i]).unwrap();
    }

    let mut markers_file = File::create(dir.join("onset_markers.csv")).expect("create csv");
    writeln!(markers_file, "position_seconds").unwrap();
    for marker in &markers {
        let t = marker.position_samples as f32 / SAMPLE_RATE;
        writeln!(markers_file, "{:.6}", t).unwrap();
    }
}
