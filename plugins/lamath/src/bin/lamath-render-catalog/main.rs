mod cli;

use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    process::ExitCode,
};

use lamath::{ModalConfig, ModalPreset, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch};
use lindelion_plugin_shell::{
    AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ProcessContext, ProcessMode, ProcessSetup,
};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 512;
const RENDER_SECONDS: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
struct CatalogCase {
    id: &'static str,
    title: &'static str,
    group: &'static str,
    tags: &'static [&'static str],
    relative_wav: &'static str,
    note: u8,
    velocity: f32,
    patch: fn() -> ResonatorSynthPatch,
}

const CASES: &[CatalogCase] = &[
    CatalogCase {
        id: "modal_marimba_c4_v100",
        title: "Modal Marimba C4 Velocity 100",
        group: "modal_baseline",
        tags: &["modal", "marimba", "baseline"],
        relative_wav: "modal/modal_marimba_c4_v100.wav",
        note: 60,
        velocity: 100.0 / 127.0,
        patch: default_modal_patch,
    },
    CatalogCase {
        id: "modal_bell_c5_v100",
        title: "Modal Bell C5 Velocity 100",
        group: "modal_baseline",
        tags: &["modal", "bell", "baseline"],
        relative_wav: "modal/modal_bell_c5_v100.wav",
        note: 72,
        velocity: 100.0 / 127.0,
        patch: bell_modal_patch,
    },
    CatalogCase {
        id: "dual_modal_body_color_c4_v100",
        title: "Dual Modal Body Color C4 Velocity 100",
        group: "modal_routing",
        tags: &["modal", "body-color", "routing"],
        relative_wav: "modal/dual_modal_body_color_c4_v100.wav",
        note: 60,
        velocity: 100.0 / 127.0,
        patch: body_color_patch,
    },
];

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
    let options = cli::CliOptions::parse_env().map_err(|error| error.to_string())?;
    match options.command {
        cli::Command::Help => {
            print_help();
            Ok(())
        }
        cli::Command::List => {
            print!("{}", catalog_listing());
            Ok(())
        }
        cli::Command::Render(selection) => {
            let selected = selected_cases(&selection)?;
            fs::create_dir_all(&options.out_dir)
                .map_err(|error| format!("create {}: {error}", options.out_dir.display()))?;
            for case in selected {
                let path = options.out_dir.join(case.relative_wav);
                render_case(case, &path)?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
    }
}

fn print_help() {
    println!("Lamath modal render catalog");
    println!();
    println!("Usage:");
    println!("  lamath-render-catalog --list");
    println!("  lamath-render-catalog --all [--out <dir>]");
    println!("  lamath-render-catalog --group <group-id> [--out <dir>]");
    println!("  lamath-render-catalog --tag <tag> [--out <dir>]");
    println!("  lamath-render-catalog --case <case-id> [--out <dir>]");
}

fn catalog_listing() -> String {
    let mut listing = String::new();
    for case in CASES {
        listing.push_str(case.id);
        listing.push_str(" | ");
        listing.push_str(case.group);
        listing.push_str(" | ");
        listing.push_str(case.title);
        listing.push('\n');
    }
    listing
}

fn selected_cases(selection: &cli::RenderSelection) -> Result<Vec<&'static CatalogCase>, String> {
    let selected = CASES
        .iter()
        .filter(|case| match selection {
            cli::RenderSelection::All => true,
            cli::RenderSelection::Group(group) => case.group == group,
            cli::RenderSelection::Case(id) => case.id == id,
            cli::RenderSelection::Tag(tag) => case.tags.iter().any(|candidate| candidate == tag),
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        Err(format!("no modal render cases matched {selection}"))
    } else {
        Ok(selected)
    }
}

fn render_case(case: &CatalogCase, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let setup = ProcessSetup {
        sample_rate: f64::from(SAMPLE_RATE),
        max_block_size: BLOCK_SIZE,
        mode: ProcessMode::Offline,
    };
    let total_samples = (SAMPLE_RATE * RENDER_SECONDS).round() as usize;
    let mut synth = ResonatorSynth::default();
    synth.set_patch_with_loaded_excitations((case.patch)(), Vec::new());
    synth.reset(setup);

    let mut rendered_left = Vec::with_capacity(total_samples);
    let mut rendered_right = Vec::with_capacity(total_samples);
    let mut left = vec![0.0; BLOCK_SIZE];
    let mut right = vec![0.0; BLOCK_SIZE];
    let mut remaining = total_samples;
    let mut first = true;
    while remaining > 0 {
        let len = remaining.min(BLOCK_SIZE);
        let events = if first {
            first = false;
            [MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: case.note,
                velocity: case.velocity,
            })]
        } else {
            [MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note: case.note,
                velocity: 0.0,
            })]
        };
        synth.process(ProcessContext::new(
            setup,
            AudioBuffer {
                left: &mut left[..len],
                right: &mut right[..len],
            },
            if remaining == total_samples {
                &events[..1]
            } else {
                &[]
            },
        ));
        rendered_left.extend_from_slice(&left[..len]);
        rendered_right.extend_from_slice(&right[..len]);
        remaining -= len;
    }

    write_wav16(path, &rendered_left, &rendered_right)
}

fn write_wav16(path: &Path, left: &[f32], right: &[f32]) -> Result<(), String> {
    let len = left.len().min(right.len());
    let data_bytes = (len * 2 * 2) as u32;
    let mut writer = BufWriter::new(
        File::create(path).map_err(|error| format!("create {}: {error}", path.display()))?,
    );
    writer
        .write_all(b"RIFF")
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(36 + data_bytes).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(b"WAVEfmt ")
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&16_u32.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&1_u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&2_u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(SAMPLE_RATE as u32).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(SAMPLE_RATE as u32 * 2 * 2).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&4_u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&16_u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(b"data")
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&data_bytes.to_le_bytes())
        .map_err(|error| error.to_string())?;
    for index in 0..len {
        writer
            .write_all(&sample_to_i16(left[index]).to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&sample_to_i16(right[index]).to_le_bytes())
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn sample_to_i16(sample: f32) -> i16 {
    let sample = if sample.is_finite() { sample } else { 0.0 };
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn default_modal_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch::default()
}

fn bell_modal_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ModalConfig {
            preset: ModalPreset::Bell,
            decay_global: 2.0,
            brightness: 0.8,
            ..ModalConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}

fn body_color_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ModalConfig {
            preset: ModalPreset::Marimba,
            brightness: 0.4,
            ..ModalConfig::default()
        },
        resonator_b: ModalConfig {
            preset: ModalPreset::Bell,
            decay_global: 2.0,
            brightness: 0.85,
            ..ModalConfig::default()
        },
        routing: ResonatorRouting::BodyColor {
            mix_a: 1.0,
            mix_b: 1.0,
        },
        ..ResonatorSynthPatch::default()
    }
}
