//! Export deterministic CSV data files used by tools/dsp-plot to render
//! DSP module response plots in docs/dsp/.
//!
//! These tests have no assertions; their effect is the CSVs they write to
//! `docs/plots/data/`. `make ci` follows `cargo test` with a
//! `git diff --exit-code` against the data directory so drift surfaces in
//! review.

use std::fs::{File, create_dir_all};
use std::io::{Result as IoResult, Write};
use std::path::PathBuf;

use lindelion_dsp_utils::envelope::{Adsr, AdsrState};
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients, OnePoleLowpass};

const SAMPLE_RATE: f32 = 48_000.0;

fn output_dir() -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root")
        .join("docs")
        .join("plots")
        .join("data")
}

fn ensure_output_dir() -> IoResult<PathBuf> {
    let dir = output_dir();
    create_dir_all(&dir)?;
    Ok(dir)
}

fn dft_complex_at(signal: &[f32], sample_rate: f32, freq_hz: f32) -> (f32, f32) {
    use std::f32::consts::TAU;
    let n = signal.len() as f32;
    let omega = TAU * freq_hz / sample_rate;
    let mut real = 0.0_f32;
    let mut imag = 0.0_f32;
    for (i, x) in signal.iter().enumerate() {
        let phi = omega * i as f32;
        real += x * phi.cos();
        imag -= x * phi.sin();
    }
    let mag = (real * real + imag * imag).sqrt() * 2.0 / n;
    let phase_deg = imag.atan2(real).to_degrees();
    (mag, phase_deg)
}

fn mag_db(mag: f32) -> f32 {
    20.0 * (mag + 1.0e-12).log10()
}

fn log_frequencies(n: usize, fmin: f32, fmax: f32) -> Vec<f32> {
    let log_min = fmin.log10();
    let log_max = fmax.log10();
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            10.0_f32.powf(log_min + t * (log_max - log_min))
        })
        .collect()
}

fn impulse_response<F: FnMut(f32) -> f32>(n_samples: usize, mut step: F) -> Vec<f32> {
    let mut out = Vec::with_capacity(n_samples);
    out.push(step(1.0));
    for _ in 1..n_samples {
        out.push(step(0.0));
    }
    out
}

#[test]
fn export_one_pole_lowpass_freqz() {
    let dir = ensure_output_dir().expect("create data dir");
    let cutoffs = [100.0_f32, 1_000.0, 5_000.0, 20_000.0];
    let freqs = log_frequencies(256, 20.0, 22_000.0);

    let responses: Vec<Vec<f32>> = cutoffs
        .iter()
        .map(|&fc| {
            let mut filter = OnePoleLowpass::new(fc, SAMPLE_RATE);
            impulse_response(4096, |x| filter.process(x))
        })
        .collect();

    let mut mag = File::create(dir.join("onepolelowpass_mag.csv")).expect("create csv");
    write!(mag, "freq_hz").unwrap();
    for &fc in &cutoffs {
        write!(mag, ",fc_{}_hz", fc as u32).unwrap();
    }
    writeln!(mag).unwrap();

    let mut phase = File::create(dir.join("onepolelowpass_phase.csv")).expect("create csv");
    write!(phase, "freq_hz").unwrap();
    for &fc in &cutoffs {
        write!(phase, ",fc_{}_hz", fc as u32).unwrap();
    }
    writeln!(phase).unwrap();

    for &f in &freqs {
        write!(mag, "{:.6}", f).unwrap();
        write!(phase, "{:.6}", f).unwrap();
        for response in &responses {
            let (m, p) = dft_complex_at(response, SAMPLE_RATE, f);
            write!(mag, ",{:.6}", mag_db(m)).unwrap();
            write!(phase, ",{:.6}", p).unwrap();
        }
        writeln!(mag).unwrap();
        writeln!(phase).unwrap();
    }
}

#[test]
fn export_one_pole_lowpass_impulse() {
    let dir = ensure_output_dir().expect("create data dir");
    let path = dir.join("onepolelowpass_impulse.csv");
    let cutoffs = [100.0_f32, 1_000.0, 5_000.0];
    let n_samples = 1024;

    let responses: Vec<Vec<f32>> = cutoffs
        .iter()
        .map(|&fc| {
            let mut filter = OnePoleLowpass::new(fc, SAMPLE_RATE);
            impulse_response(n_samples, |x| filter.process(x))
        })
        .collect();

    let mut file = File::create(&path).expect("create csv");
    write!(file, "time_s").unwrap();
    for &fc in &cutoffs {
        write!(file, ",fc_{}_hz", fc as u32).unwrap();
    }
    writeln!(file).unwrap();

    for i in 0..n_samples {
        let t = i as f32 / SAMPLE_RATE;
        write!(file, "{:.6}", t).unwrap();
        for response in &responses {
            write!(file, ",{:.6}", response[i]).unwrap();
        }
        writeln!(file).unwrap();
    }
}

#[test]
fn export_biquad_freqz() {
    let dir = ensure_output_dir().expect("create data dir");
    let cutoff = 1_000.0_f32;
    let q = 0.707_f32;

    let mut lp = Biquad::new(BiquadCoefficients::lowpass(SAMPLE_RATE, cutoff, q));
    let mut hp = Biquad::new(BiquadCoefficients::highpass(SAMPLE_RATE, cutoff, q));
    let mut bp = Biquad::new(BiquadCoefficients::bandpass(SAMPLE_RATE, cutoff, q));

    let lp_response = impulse_response(4096, |x| lp.process(x));
    let hp_response = impulse_response(4096, |x| hp.process(x));
    let bp_response = impulse_response(4096, |x| bp.process(x));

    let freqs = log_frequencies(256, 20.0, 22_000.0);

    let mut mag = File::create(dir.join("biquad_mag.csv")).expect("create csv");
    writeln!(mag, "freq_hz,lowpass,highpass,bandpass").unwrap();
    let mut phase = File::create(dir.join("biquad_phase.csv")).expect("create csv");
    writeln!(phase, "freq_hz,lowpass,highpass,bandpass").unwrap();

    for &f in &freqs {
        let (lp_mag, lp_phase) = dft_complex_at(&lp_response, SAMPLE_RATE, f);
        let (hp_mag, hp_phase) = dft_complex_at(&hp_response, SAMPLE_RATE, f);
        let (bp_mag, bp_phase) = dft_complex_at(&bp_response, SAMPLE_RATE, f);
        writeln!(
            mag,
            "{:.6},{:.6},{:.6},{:.6}",
            f,
            mag_db(lp_mag),
            mag_db(hp_mag),
            mag_db(bp_mag),
        )
        .unwrap();
        writeln!(
            phase,
            "{:.6},{:.6},{:.6},{:.6}",
            f, lp_phase, hp_phase, bp_phase
        )
        .unwrap();
    }
}

#[test]
fn export_biquad_impulse() {
    let dir = ensure_output_dir().expect("create data dir");
    let path = dir.join("biquad_impulse.csv");
    let cutoff = 1_000.0_f32;
    let q = 0.707_f32;
    let n_samples = 512;

    let mut lp = Biquad::new(BiquadCoefficients::lowpass(SAMPLE_RATE, cutoff, q));
    let mut hp = Biquad::new(BiquadCoefficients::highpass(SAMPLE_RATE, cutoff, q));
    let mut bp = Biquad::new(BiquadCoefficients::bandpass(SAMPLE_RATE, cutoff, q));

    let lp_response = impulse_response(n_samples, |x| lp.process(x));
    let hp_response = impulse_response(n_samples, |x| hp.process(x));
    let bp_response = impulse_response(n_samples, |x| bp.process(x));

    let mut file = File::create(&path).expect("create csv");
    writeln!(file, "time_s,lowpass,highpass,bandpass").unwrap();
    for i in 0..n_samples {
        let t = i as f32 / SAMPLE_RATE;
        writeln!(
            file,
            "{:.6},{:.6},{:.6},{:.6}",
            t, lp_response[i], hp_response[i], bp_response[i]
        )
        .unwrap();
    }
}

#[test]
fn export_biquad_coefficients() {
    let dir = ensure_output_dir().expect("create data dir");
    let path = dir.join("biquad_ba.csv");
    let cutoff = 1_000.0_f32;
    let q = 0.707_f32;

    let lp = BiquadCoefficients::lowpass(SAMPLE_RATE, cutoff, q);
    let hp = BiquadCoefficients::highpass(SAMPLE_RATE, cutoff, q);
    let bp = BiquadCoefficients::bandpass(SAMPLE_RATE, cutoff, q);

    let mut file = File::create(&path).expect("create csv");
    writeln!(file, "name,b0,b1,b2,a1,a2").unwrap();
    for (name, c) in [("lowpass", lp), ("highpass", hp), ("bandpass", bp)] {
        writeln!(
            file,
            "{},{:.6},{:.6},{:.6},{:.6},{:.6}",
            name, c.b0, c.b1, c.b2, c.a1, c.a2
        )
        .unwrap();
    }
}

#[test]
fn export_adsr_step() {
    let dir = ensure_output_dir().expect("create data dir");
    let path = dir.join("adsr_step.csv");

    let adsr = Adsr {
        attack_ms: 20.0,
        decay_ms: 100.0,
        sustain: 0.5,
        release_ms: 200.0,
    };
    let note_on_samples = (0.4 * SAMPLE_RATE) as usize;
    let release_samples = (0.4 * SAMPLE_RATE) as usize;
    let total = note_on_samples + release_samples;

    let mut state = AdsrState::default();
    state.note_on();
    let mut values = Vec::with_capacity(total);
    for i in 0..total {
        if i == note_on_samples {
            state.note_off();
        }
        values.push(state.next_sample(adsr, SAMPLE_RATE));
    }

    let mut file = File::create(&path).expect("create csv");
    writeln!(file, "time_s,value").unwrap();
    for (i, v) in values.iter().enumerate() {
        let t = i as f32 / SAMPLE_RATE;
        writeln!(file, "{:.6},{:.6}", t, v).unwrap();
    }
}
