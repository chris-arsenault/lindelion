//! Real-material fidelity coverage for the active Resample Pro engine.
//!
//! Wires the committed real fixtures (`testdata/audio/`, see `FIXTURES.md`) into the test
//! battery: a fast CI guard that the active engine renders real instruments / vocals / mixes
//! finite, level-bounded, and at the correct shifted pitch, plus an on-demand baseline
//! generator that dumps objective metrics + rendered WAVs for A/B listening. Fixtures are
//! 44.1 kHz (unlike the synthetic 48 kHz battery), exercised at their own sample rate.

use std::fs::{File, create_dir_all};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use lindelion_dsp_utils::analysis::{
    estimate_f0_autocorrelation, gain_fitted_rms_difference, high_frequency_artifact_ratio,
    peak_abs, rms,
};
use lindelion_sample_library::decode_wav_mono;

use super::{constant_pitch_contour, markers};
use crate::{PitchShiftAnalysisConfig, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios};

/// A real fixture and how to interpret it.
struct RealFixture {
    file: &'static str,
    kind: Kind,
}

enum Kind {
    /// Pitched single source; `(lo, hi)` bound the fundamental search.
    Pitched(f32, f32),
    /// Broadband / percussive (no stable fundamental).
    Percussive,
    /// Polyphonic ensemble / mix (no single fundamental).
    Mix,
}

const FIXTURES: &[RealFixture] = &[
    RealFixture {
        file: "iowa_doublebass_E2.wav",
        kind: Kind::Pitched(60.0, 120.0),
    },
    RealFixture {
        file: "iowa_tuba_E2.wav",
        kind: Kind::Pitched(60.0, 120.0),
    },
    RealFixture {
        file: "iowa_cello_C3.wav",
        kind: Kind::Pitched(100.0, 180.0),
    },
    RealFixture {
        file: "iowa_bassoon_C3.wav",
        kind: Kind::Pitched(100.0, 180.0),
    },
    RealFixture {
        file: "iowa_horn_C3.wav",
        kind: Kind::Pitched(100.0, 180.0),
    },
    RealFixture {
        file: "iowa_viola_C4.wav",
        kind: Kind::Pitched(210.0, 320.0),
    },
    RealFixture {
        file: "iowa_marimba_C4.wav",
        kind: Kind::Pitched(210.0, 320.0),
    },
    RealFixture {
        file: "iowa_clarinet_G4.wav",
        kind: Kind::Pitched(320.0, 470.0),
    },
    RealFixture {
        file: "iowa_violin_A4.wav",
        kind: Kind::Pitched(360.0, 540.0),
    },
    RealFixture {
        file: "iowa_oboe_A4.wav",
        kind: Kind::Pitched(360.0, 540.0),
    },
    RealFixture {
        file: "iowa_trumpet_C5.wav",
        kind: Kind::Pitched(430.0, 640.0),
    },
    RealFixture {
        file: "iowa_vibraphone_C5.wav",
        kind: Kind::Pitched(430.0, 640.0),
    },
    RealFixture {
        file: "iowa_flute_A5.wav",
        kind: Kind::Pitched(720.0, 1080.0),
    },
    RealFixture {
        file: "iowa_cymbal_crash.wav",
        kind: Kind::Percussive,
    },
    RealFixture {
        file: "iowa_tambourine.wav",
        kind: Kind::Percussive,
    },
    RealFixture {
        file: "vocal_sung.wav",
        kind: Kind::Pitched(300.0, 640.0),
    },
    RealFixture {
        file: "vocal_spoken.wav",
        kind: Kind::Pitched(80.0, 320.0),
    },
    RealFixture {
        file: "usaf_jazz_bassmix.wav",
        kind: Kind::Mix,
    },
    RealFixture {
        file: "usaf_jazz_sustained.wav",
        kind: Kind::Mix,
    },
    RealFixture {
        file: "usaf_jazz_bright.wav",
        kind: Kind::Mix,
    },
    RealFixture {
        file: "usaf_jazz_transient.wav",
        kind: Kind::Mix,
    },
];

/// CI guard: the active engine must render representative real fixtures finite, level-bounded,
/// and (for steady single-note instruments) at the correct shifted fundamental. This is the
/// real-material counterpart to the synthetic `resample_pro` quality-contract tests — it catches
/// gross breakage (silence, blow-ups, octave errors, dropped pitch) on actual instruments,
/// voice, and mixes. A representative subset keeps it fast; full coverage is in the ignored
/// baseline generator. Pitch is checked over the same steady window of source and output (so
/// the shift *ratio* is verified), with the output search constrained near the target to avoid
/// harmonic/octave mis-detection.
#[test]
fn real_fixtures_pitch_shift_is_finite_bounded_and_pitched() {
    let ratio = PitchShiftRatios::from_semitones_cents(7.0, 0.0).pitch_ratio;

    // Steady single-note instruments low/mid/high: verify the shift ratio precisely.
    for (file, lo, hi) in [
        ("iowa_doublebass_E2.wav", 60.0_f32, 120.0_f32),
        ("iowa_oboe_A4.wav", 360.0, 540.0),
        ("iowa_flute_A5.wav", 720.0, 1080.0),
    ] {
        let (full, sample_rate) = load(file);
        let audio = center_slice(&full, 24_000);
        let source_f0 = estimate_f0_autocorrelation(steady(&audio), sample_rate as f32, lo, hi)
            .unwrap_or_else(|| panic!("{file}: no source fundamental"));
        let rendered = render(
            &audio,
            sample_rate,
            source_f0,
            PitchShiftRatios {
                pitch_ratio: ratio,
                formant_ratio: None,
            },
        );
        assert_finite_and_bounded(file, &audio, &rendered);

        let target = source_f0 * ratio;
        let output_f0 = estimate_f0_autocorrelation(
            steady(&rendered),
            sample_rate as f32,
            target * 0.85,
            target * 1.15,
        )
        .unwrap_or_else(|| panic!("{file}: no output fundamental near target"));
        let error_cents = 1_200.0 * (output_f0 / target).log2();
        assert!(
            error_cents.abs() < 35.0,
            "{file}: pitch off by {error_cents:.1} cents (source {source_f0:.1} -> output {output_f0:.1}, target {target:.1})"
        );
    }

    // Voice + mix (no stable single fundamental): smoke-check the render is finite and bounded.
    for file in ["vocal_sung.wav", "usaf_jazz_bright.wav"] {
        let (full, sample_rate) = load(file);
        let audio = center_slice(&full, 24_000);
        let f0 = estimate_f0_autocorrelation(steady(&audio), sample_rate as f32, 80.0, 1_000.0)
            .unwrap_or(220.0);
        let rendered = render(
            &audio,
            sample_rate,
            f0,
            PitchShiftRatios {
                pitch_ratio: ratio,
                formant_ratio: None,
            },
        );
        assert_finite_and_bounded(file, &audio, &rendered);
    }
}

fn assert_finite_and_bounded(file: &str, source: &[f32], rendered: &[f32]) {
    assert!(
        rendered.iter().all(|sample| sample.is_finite()),
        "{file}: non-finite output"
    );
    assert!(
        peak_abs(rendered) <= peak_abs(source) * 1.8 + 0.05,
        "{file}: output peak unbounded ({} vs source {})",
        peak_abs(rendered),
        peak_abs(source)
    );
    assert!(
        rms(rendered) > rms(source) * 0.2,
        "{file}: output collapsed in level"
    );
}

/// On-demand baseline generator over the real fixtures: writes a metrics table and rendered
/// WAVs under `docs/plots/data/resample_pro_fidelity/real/`. Not a per-commit gate (the guard
/// above is); run with:
/// `cargo test -p lindelion-pitch-shift -- --ignored real_fixtures_fidelity_baseline`.
#[test]
#[ignore = "baseline generator: cargo test -p lindelion-pitch-shift -- --ignored real_fixtures_fidelity_baseline"]
fn real_fixtures_fidelity_baseline() {
    let dir = output_dir();
    create_dir_all(&dir).expect("create real fixture data dir");
    let mut table = String::new();
    table.push_str("# Resample Pro fidelity baseline — real fixtures\n\n");
    table
        .push_str("Active engine. Lower dB = cleaner. See `../../FIXTURES.md` for provenance.\n\n");
    table.push_str("| fixture | shift | metric | value |\n| --- | --- | --- | --- |\n");

    for fixture in FIXTURES {
        let (audio, sample_rate) = load(fixture.file);
        let f0 = match fixture.kind {
            Kind::Pitched(lo, hi) => {
                estimate_f0_autocorrelation(&audio, sample_rate as f32, lo, hi).unwrap_or(220.0)
            }
            _ => 220.0,
        };
        let source_crest = peak_abs(&audio) / rms(&audio).max(1.0e-9);
        write_wav(
            &dir.join(format!("{}_source.wav", stem(fixture.file))),
            &audio,
            sample_rate,
        );

        for semitones in [7.0_f32, 12.0] {
            let ratio = PitchShiftRatios::from_semitones_cents(semitones, 0.0).pitch_ratio;
            let rendered = render(
                &audio,
                sample_rate,
                f0,
                PitchShiftRatios {
                    pitch_ratio: ratio,
                    formant_ratio: None,
                },
            );
            let label = format!("+{semitones:.0}st");
            let crest = peak_abs(&rendered) / rms(&rendered).max(1.0e-9);
            push(
                &mut table,
                fixture.file,
                &label,
                "crest_ratio",
                crest / source_crest.max(1.0e-9),
            );
            if let Kind::Pitched(..) = fixture.kind {
                let hf = to_db(high_frequency_artifact_ratio(
                    steady(&rendered),
                    sample_rate as f32,
                    f0 * ratio,
                ));
                push(&mut table, fixture.file, &label, "hf_artifact_dB", hf);
            }
            if semitones == 7.0 {
                write_wav(
                    &dir.join(format!("{}_+7st.wav", stem(fixture.file))),
                    &rendered,
                    sample_rate,
                );
            }
        }

        // Round-trip reconstruction (+7 then -7) — the best single fidelity proxy.
        let up = render(&audio, sample_rate, f0, ratios(7.0));
        let back = render(&up, sample_rate, f0 * ratios(7.0).pitch_ratio, ratios(-7.0));
        let len = audio.len().min(back.len());
        let recon = to_db(
            gain_fitted_rms_difference(&audio[..len], &back[..len])
                / rms(&audio[..len]).max(1.0e-9),
        );
        push(&mut table, fixture.file, "+7/-7", "roundtrip_dB", recon);
    }

    let mut file = File::create(dir.join("real_baseline.md")).expect("create real_baseline.md");
    file.write_all(table.as_bytes())
        .expect("write real_baseline.md");
}

// --- helpers ---

fn load(name: &str) -> (Vec<f32>, u32) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|_| panic!("missing fixture {name}"));
    let decoded = decode_wav_mono(&path).unwrap_or_else(|_| panic!("decode {name}"));
    (decoded.samples, decoded.sample_rate)
}

fn render(audio: &[f32], sample_rate: u32, f0: f32, ratios: PitchShiftRatios) -> Vec<f32> {
    let contour = constant_pitch_contour(sample_rate, f0, audio.len());
    let cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();
    PitchShiftEngine
        .render_resample_pro_pitch_shift(&cache, ratios)
        .unwrap()
}

fn ratios(semitones: f32) -> PitchShiftRatios {
    PitchShiftRatios {
        pitch_ratio: PitchShiftRatios::from_semitones_cents(semitones, 0.0).pitch_ratio,
        formant_ratio: None,
    }
}

fn steady(samples: &[f32]) -> &[f32] {
    let len = 4_096.min(samples.len());
    let start = samples.len().saturating_sub(len) / 2;
    &samples[start..start + len]
}

/// Centered window of up to `len` samples with short cosine edge fades — keeps the CI guard's
/// renders short while avoiding hard-cut edge transients that would otherwise inflate the
/// output peak and mask the engine's actual behaviour.
fn center_slice(samples: &[f32], len: usize) -> Vec<f32> {
    let len = len.min(samples.len());
    let start = samples.len().saturating_sub(len) / 2;
    let mut window = samples[start..start + len].to_vec();
    let fade = 441.min(len / 2); // ~10 ms at 44.1 kHz
    for index in 0..fade {
        let gain = 0.5 - 0.5 * (std::f32::consts::PI * index as f32 / fade as f32).cos();
        window[index] *= gain;
        let tail = len - 1 - index;
        window[tail] *= gain;
    }
    window
}

fn to_db(ratio: f32) -> f32 {
    20.0 * ratio.max(1.0e-12).log10()
}

fn stem(file: &str) -> &str {
    file.strip_suffix(".wav").unwrap_or(file)
}

fn push(table: &mut String, fixture: &str, shift: &str, metric: &str, value: f32) {
    assert!(value.is_finite(), "{fixture}/{metric} non-finite");
    table.push_str(&format!(
        "| {fixture} | {shift} | {metric} | {value:.3} |\n"
    ));
}

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("docs/plots/data/resample_pro_fidelity/real")
}

/// Minimal 16-bit PCM mono WAV writer at an arbitrary sample rate.
fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) {
    let data_bytes = (samples.len() * 2) as u32;
    let mut bytes: Vec<u8> = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for &sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}
