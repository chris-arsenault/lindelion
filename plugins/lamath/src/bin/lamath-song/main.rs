//! Lamath song renderer: composes a 32-bar piece for the four Lamath
//! instrument families (Modal, String, Tube, Mesh), renders every track to its
//! own stem WAV, then sums the stems through a simple mixer: per-stem cleanup
//! highpass, per-track leveling (active-RMS or peak target), constant-power
//! pan, per-section fader rides, and master peak normalization into one mix.
//!
//! Usage: `lamath-song [--out <dir>]` (default `review/lamath-song`).
//! Stems land in `<out>/tracks/`, the mix at `<out>/lamath-song.wav`.

mod composition;
mod render;
mod score;

use crate::composition::Level;
use lindelion_dsp_utils::filters::{Svf, SvfMode};
use lindelion_sample_library::write_wav_stereo_pcm16;
use std::path::PathBuf;
use std::process::ExitCode;

/// Stems are trimmed so an RMS-normalized track can never clip on its own.
const STEM_PEAK_CEILING: f32 = 0.85;
/// The summed mix is normalized to just below full scale.
const MIX_PEAK_CEILING: f32 = 0.989;
/// Cap on per-track makeup gain so a near-silent track is not blown up.
const MAX_TRACK_GAIN: f32 = 16.0;
/// Frames quieter than this on both channels do not count toward active RMS.
const ACTIVE_FLOOR: f32 = 1.0e-4;
/// Rumble cleanup on the summed mix.
const MIX_HIGHPASS_HZ: f32 = 30.0;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let out_dir = parse_out_dir()?;
    let tracks_dir = out_dir.join("tracks");
    std::fs::create_dir_all(&tracks_dir)
        .map_err(|error| format!("failed to create {}: {error}", tracks_dir.display()))?;

    let frames = composition::total_frames();
    let specs = composition::tracks();
    let mut mix_left = vec![0.0f32; frames];
    let mut mix_right = vec![0.0f32; frames];

    println!(
        "Rendering {} tracks, {} bars at {} BPM ({:.1}s each)...",
        specs.len(),
        composition::TOTAL_BARS,
        score::BPM,
        frames as f32 / render::SAMPLE_RATE as f32,
    );

    for spec in &specs {
        let (mut left, mut right) = render::render_voice(spec.voice, &spec.notes, frames);
        highpass(&mut left, spec.mix.highpass_hz);
        highpass(&mut right, spec.mix.highpass_hz);
        let gain = track_gain(&left, &right, spec.mix.level);
        let (pan_left, pan_right) = pan_gains(spec.mix.pan);
        apply_ride(&mut left, gain * pan_left, spec.mix.ride);
        apply_ride(&mut right, gain * pan_right, spec.mix.ride);
        accumulate(&mut mix_left, &left);
        accumulate(&mut mix_right, &right);

        let path = tracks_dir.join(format!("{}.wav", spec.name));
        let metrics = write_wav_stereo_pcm16(&path, &left, &right, render::SAMPLE_RATE)
            .map_err(|error| format!("failed to write {}: {error:?}", path.display()))?;
        println!(
            "  {:<18} {:>3} notes  gain {:+5.1} dB  peak {:6.1} dBFS  rms {:6.1} dBFS",
            spec.name,
            spec.notes.len(),
            to_db(gain),
            metrics.peak_dbfs,
            metrics.rms_dbfs,
        );
    }

    highpass(&mut mix_left, MIX_HIGHPASS_HZ);
    highpass(&mut mix_right, MIX_HIGHPASS_HZ);
    let master = normalize(&mut mix_left, &mut mix_right);
    let mix_path = out_dir.join("lamath-song.wav");
    let metrics = write_wav_stereo_pcm16(&mix_path, &mix_left, &mix_right, render::SAMPLE_RATE)
        .map_err(|error| format!("failed to write {}: {error:?}", mix_path.display()))?;
    println!(
        "Mix: {}  master {:+.1} dB  peak {:.1} dBFS  rms {:.1} dBFS  {:.1}s",
        mix_path.display(),
        to_db(master),
        metrics.peak_dbfs,
        metrics.rms_dbfs,
        metrics.duration_seconds,
    );
    Ok(())
}

fn parse_out_dir() -> Result<PathBuf, String> {
    let mut args = std::env::args().skip(1);
    let mut out_dir = PathBuf::from("review/lamath-song");
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--out" => {
                out_dir = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| "--out requires a directory argument".to_string())?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(out_dir)
}

/// Gain that brings the track to its level target: active-region RMS (capped
/// so the stem peak stays under `STEM_PEAK_CEILING`) or absolute peak.
fn track_gain(left: &[f32], right: &[f32], level: Level) -> f32 {
    match level {
        Level::ActiveRms(target_dbfs) => {
            let rms = active_rms(left, right);
            if rms <= 0.0 {
                return 1.0;
            }
            let mut gain = (db_to_gain(target_dbfs) / rms).min(MAX_TRACK_GAIN);
            let peak = peak_abs(left, right);
            if peak * gain > STEM_PEAK_CEILING {
                gain = STEM_PEAK_CEILING / peak;
            }
            gain
        }
        Level::Peak(target_dbfs) => {
            let peak = peak_abs(left, right);
            if peak <= 0.0 {
                return 1.0;
            }
            db_to_gain(target_dbfs) / peak
        }
    }
}

/// RMS over frames where either channel is audibly active, so sparse tracks
/// (a crash every few bars) are leveled by what they play, not their silence.
fn active_rms(left: &[f32], right: &[f32]) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for (l, r) in left.iter().zip(right) {
        if l.abs().max(r.abs()) > ACTIVE_FLOOR {
            sum += 0.5 * f64::from(l * l + r * r);
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    ((sum / count as f64) as f32).sqrt()
}

fn to_db(gain: f32) -> f32 {
    20.0 * gain.max(1.0e-6).log10()
}

fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// In-place 12 dB/oct TPT highpass (Butterworth Q) for stem/mix cleanup.
fn highpass(samples: &mut [f32], cutoff_hz: f32) {
    let mut svf = Svf::new(render::SAMPLE_RATE as f32);
    svf.set_params_q(
        cutoff_hz,
        std::f32::consts::FRAC_1_SQRT_2,
        SvfMode::Highpass,
    );
    for sample in samples {
        *sample = svf.process(*sample);
    }
}

/// Applies the static gain shaped by the track's fader ride over song bars.
fn apply_ride(samples: &mut [f32], base_gain: f32, ride: &[(f32, f32)]) {
    if ride.is_empty() {
        scale(samples, base_gain);
        return;
    }
    let frames_per_bar = score::beats_to_seconds(score::BEATS_PER_BAR) * render::SAMPLE_RATE as f32;
    for (index, sample) in samples.iter_mut().enumerate() {
        let bar = index as f32 / frames_per_bar + 1.0;
        *sample *= base_gain * db_to_gain(ride_db_at(ride, bar));
    }
}

/// Linear interpolation over (bar, dB) breakpoints; constant outside them.
fn ride_db_at(ride: &[(f32, f32)], bar: f32) -> f32 {
    let (first_bar, first_db) = ride[0];
    if bar <= first_bar {
        return first_db;
    }
    for pair in ride.windows(2) {
        let [(bar0, db0), (bar1, db1)] = [pair[0], pair[1]];
        if bar <= bar1 {
            return db0 + (db1 - db0) * ((bar - bar0) / (bar1 - bar0).max(f32::EPSILON));
        }
    }
    ride[ride.len() - 1].1
}

fn peak_abs(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .chain(right)
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

/// Constant-power pan in [-1, 1].
fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

fn scale(samples: &mut [f32], gain: f32) {
    for sample in samples {
        *sample *= gain;
    }
}

fn accumulate(mix: &mut [f32], stem: &[f32]) {
    for (out, sample) in mix.iter_mut().zip(stem) {
        *out += sample;
    }
}

/// Scales the mix (up or down) so its peak sits at the ceiling, using the
/// headroom the per-track leveling left on the bus.
fn normalize(left: &mut [f32], right: &mut [f32]) -> f32 {
    let peak = peak_abs(left, right);
    if peak <= 0.0 {
        return 1.0;
    }
    let master = MIX_PEAK_CEILING / peak;
    scale(left, master);
    scale(right, master);
    master
}
