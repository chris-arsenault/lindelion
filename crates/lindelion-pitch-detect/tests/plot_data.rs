//! Export pitch-detection demo CSV used by tools/dsp-plot.

use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use lindelion_pitch_detect::{PitchDetector, SwiftF0Detector};

const SAMPLE_RATE: u32 = 48_000;
const SAMPLE_RATE_F: f32 = 48_000.0;

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
fn export_pitch_tracking_demo() {
    let dir = output_dir();
    create_dir_all(&dir).expect("create data dir");

    let frequencies = [220.0_f32, 330.0, 440.0, 660.0, 880.0];
    let note_duration = 0.4_f32;
    let ramp_duration = 0.02_f32;
    let total_duration = note_duration * frequencies.len() as f32;
    let n_samples = (total_duration * SAMPLE_RATE_F) as usize;
    let ramp_samples = (ramp_duration * SAMPLE_RATE_F) as usize;
    let note_samples = (note_duration * SAMPLE_RATE_F) as usize;

    let mut audio = vec![0.0_f32; n_samples];
    let mut phase = 0.0_f32;
    let two_pi = 2.0 * std::f32::consts::PI;

    for (note_idx, &freq) in frequencies.iter().enumerate() {
        let start_idx = note_idx * note_samples;
        let end_idx = ((note_idx + 1) * note_samples).min(n_samples);
        let span = end_idx - start_idx;

        for (local_i, sample) in audio[start_idx..end_idx].iter_mut().enumerate() {
            let envelope = if local_i < ramp_samples {
                0.5 * (1.0 - (std::f32::consts::PI * local_i as f32 / ramp_samples as f32).cos())
            } else if local_i + ramp_samples > span {
                let remaining = span - local_i;
                0.5 * (1.0 - (std::f32::consts::PI * remaining as f32 / ramp_samples as f32).cos())
            } else {
                1.0
            };

            *sample = envelope * phase.sin() * 0.5;
            phase += two_pi * freq / SAMPLE_RATE_F;
            if phase > two_pi {
                phase -= two_pi;
            }
        }
    }

    let detector = SwiftF0Detector::default();
    let contour = detector
        .detect(&audio, SAMPLE_RATE)
        .expect("pitch detection");

    let mut file = File::create(dir.join("pitch_tracking.csv")).expect("create csv");
    writeln!(file, "time_s,detected_hz,true_hz").unwrap();
    for frame in &contour.frames {
        let t = frame.timestamp_seconds;
        let detected = frame.f0_hz.unwrap_or(0.0);
        let note_idx = (t / note_duration) as usize;
        let true_hz = if note_idx < frequencies.len() {
            frequencies[note_idx]
        } else {
            0.0
        };
        writeln!(file, "{:.6},{:.6},{:.6}", t, detected, true_hz).unwrap();
    }
}
