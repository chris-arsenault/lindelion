//! Galad — standalone Windows realtime VST3 host application.
//!
//! Live microphone device → an ordered chain of arbitrary standard VST3 plugins → output device,
//! with full device management. This is the *host* side of VST3 (distinct from the workspace's
//! plugins, which are the guest side). Windows-only and target-gated; excluded from the
//! Linux/macOS `make ci` path (see [ADR-0022](../docs/adr/0022-windows-vst3-host.md)).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod diagnostics;
mod session;
mod ui;
mod vst3_host;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

pub(crate) const RELAUNCH_ARG: &str = "--galad-relaunched";

fn main() {
    diagnostics::init();
    let args: Vec<String> = std::env::args().collect();
    diagnostics::log(format!(
        "main: start args={args:?} exe={:?} cwd={:?}",
        std::env::current_exe(),
        std::env::current_dir()
    ));

    let already_relaunched = args.iter().skip(1).any(|arg| arg == RELAUNCH_ARG);
    let filtered_args: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|arg| arg.as_str() != RELAUNCH_ARG)
        .cloned()
        .collect();

    let mut rest = filtered_args.into_iter();
    match rest.next().as_deref() {
        Some("spike") => run_spike_command(rest.next()),
        Some("passthrough") => run_passthrough_command(rest.next(), rest.next()),
        Some("chain") => run_chain_command(rest.collect()),
        Some("session") => run_session_command(rest.next()),
        Some("editor") => run_editor_command(rest.collect()),
        Some("diagnostics") => print_diagnostics_path(),
        None | Some("ui") => run_ui_command(already_relaunched),
        _ => print_usage(),
    }
}

fn print_diagnostics_path() {
    println!("{}", diagnostics::log_path().display());
}

/// Print the command-line usage.
fn print_usage() {
    println!("Galad — Windows realtime VST3 host");
    println!("usage:");
    println!("  galad                                       # launch the host UI (Windows) (M6)");
    println!("  galad spike <path-to.vst3>                  # host-side VST3 spike (M1)");
    println!("  galad passthrough <in-id> <out-id>          # live mic→output (M2)");
    println!("  galad chain <in-id> <out-id> <plugin.vst3>... [--save <file>]  # chain (M3/M4)");
    println!("  galad session <file>                        # restore a saved session (M4)");
    println!("  galad editor <plugin.vst3>...               # open plugin editor windows (M5)");
    println!("  galad diagnostics                           # print startup diagnostic log path");
}

/// `galad` (no subcommand) / `galad ui` — launch the standalone Vizia host UI (M6). Windows-only.
#[cfg(windows)]
fn run_ui_command(already_relaunched: bool) {
    diagnostics::log(format!(
        "main: run_ui_command enter already_relaunched={already_relaunched}"
    ));
    if diagnostics::relaunch_without_explorer_startup(already_relaunched) {
        diagnostics::log("main: run_ui_command relaunched child; exiting parent");
        return;
    }
    ui::run();
    diagnostics::log("main: run_ui_command exit");
}

#[cfg(not(windows))]
fn run_ui_command(_already_relaunched: bool) {
    eprintln!("the Galad host UI is Windows-only; on this platform use the subcommands:\n");
    print_usage();
}

/// `galad editor <plugin.vst3>...` — open each plugin's native editor window and run a message loop.
#[cfg(windows)]
fn run_editor_command(args: Vec<String>) {
    use std::path::Path;

    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, TranslateMessage,
    };

    use crate::vst3_host::EditorHost;

    if args.is_empty() {
        eprintln!("usage: galad editor <plugin.vst3>...");
        std::process::exit(2);
    }

    let mut host = EditorHost::with_quit_on_last_close();
    for path in &args {
        if let Err(error) = host.open(Path::new(path)) {
            eprintln!("failed to open editor for {path}: {error:?}");
        }
    }
    if host.is_empty() {
        eprintln!("no editors opened");
        std::process::exit(1);
    }

    println!("editor(s) open — close all windows to exit");
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            host.apply_resizes();
        }
    }
}

#[cfg(not(windows))]
fn run_editor_command(_args: Vec<String>) {
    eprintln!("editor hosting is Windows-only");
    std::process::exit(2);
}

/// `galad passthrough <input-id> <output-id>` — live WASAPI mic → output passthrough.
#[cfg(windows)]
fn run_passthrough_command(input: Option<String>, output: Option<String>) {
    use crate::audio::{AudioDirection, AudioEngine};
    use crate::session::DeviceRef;

    let (Some(input_id), Some(output_id)) = (input, output) else {
        eprintln!("usage: galad passthrough <input-id> <output-id>\n");
        print_devices("input", AudioDirection::Input);
        print_devices("output", AudioDirection::Output);
        std::process::exit(2);
    };

    let input = DeviceRef {
        id: input_id.clone(),
        name: input_id,
    };
    let output = DeviceRef {
        id: output_id.clone(),
        name: output_id,
    };

    match AudioEngine::start(input, output) {
        Ok(mut engine) => {
            let latency = engine.measured_latency();
            println!(
                "passthrough running — round-trip latency ≈ {:.1} ms",
                latency.total_ms
            );
            println!("press Enter to stop...");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            engine.stop();
        }
        Err(error) => {
            eprintln!("passthrough failed: {error:?}");
            std::process::exit(1);
        }
    }
}

/// Print the available endpoints for `direction` as a usage aid.
#[cfg(windows)]
fn print_devices(label: &str, direction: crate::audio::AudioDirection) {
    match crate::audio::enumerate(direction) {
        Ok(devices) => {
            println!("available {label} devices:");
            for device in devices {
                println!("  {}  [{}]", device.name, device.id);
            }
        }
        Err(error) => eprintln!("  (failed to enumerate {label} devices: {error:?})"),
    }
}

#[cfg(not(windows))]
fn run_passthrough_command(_input: Option<String>, _output: Option<String>) {
    eprintln!("audio passthrough is Windows-only");
    std::process::exit(2);
}

/// `galad chain <in-id> <out-id> <plugin.vst3>... [--save <file>]` — live mic → a VST3 chain →
/// output; with `--save`, capture the session (devices + plugins + order + state + bypass) on exit.
#[cfg(windows)]
fn run_chain_command(args: Vec<String>) {
    use std::path::Path;
    use std::sync::Arc;

    use vst3::Steinberg::Vst::IHostApplication;

    use crate::audio::AudioEngine;
    use crate::session::{AppSettings, DeviceRef};
    use crate::vst3_host::{
        ChainProcessor, HostContext, LoadedModule, PluginInstance, ProcessDriver, SessionSlot,
        capture_session, load_module,
    };

    // Parse an optional `--save <file>` from anywhere in the args; the rest are positional.
    let mut save: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "--save" {
            save = it.next();
        } else {
            positional.push(arg);
        }
    }
    if positional.len() < 3 {
        eprintln!("usage: galad chain <input-id> <output-id> <plugin.vst3>... [--save <file>]");
        std::process::exit(2);
    }
    let input = DeviceRef {
        id: positional[0].clone(),
        name: positional[0].clone(),
    };
    let output = DeviceRef {
        id: positional[1].clone(),
        name: positional[1].clone(),
    };
    let plugin_paths: Vec<String> = positional[2..].to_vec();

    let host = HostContext::new()
        .to_com_ptr::<IHostApplication>()
        .expect("host exposes IHostApplication");

    // Prepare the plugins at the input device's actual sample rate (the rate the engine runs at) so
    // plugins are told the true rate — the host declares it, it does not resample. 4096-frame blocks.
    let sample_rate = crate::audio::device_sample_rate(&input)
        .map(|rate| rate as f64)
        .unwrap_or(48_000.0);
    let driver = ProcessDriver::new(sample_rate, 4096);

    // The modules + instances must outlive the engine/chain (the modules own the DLLs; the instances
    // are shared with the chain via `Arc`), so keep them in this outer scope — they drop *after* the
    // engine below.
    let mut modules: Vec<LoadedModule> = Vec::new();
    let mut instances: Vec<Arc<PluginInstance>> = Vec::new();
    for path in &plugin_paths {
        let module = match load_module(Path::new(path)) {
            Ok(module) => module,
            Err(error) => {
                eprintln!("failed to load {path}: {error:?}");
                std::process::exit(1);
            }
        };
        match PluginInstance::from_factory(module.factory(), &host) {
            Ok(instance) => {
                let instance = Arc::new(instance);
                if let Err(error) = driver.prepare(&instance) {
                    eprintln!("failed to prepare {path}: {error:?}");
                    std::process::exit(1);
                }
                instances.push(instance);
                modules.push(module);
            }
            Err(error) => {
                eprintln!("failed to instantiate {path}: {error:?}");
                std::process::exit(1);
            }
        }
    }

    let bypass = vec![false; instances.len()];
    let chain = Box::new(ChainProcessor::new(
        instances.iter().cloned().collect(),
        bypass.clone(),
        4096,
    ));

    match AudioEngine::start_with_chain(input.clone(), output.clone(), chain) {
        Ok(mut engine) => {
            let latency = engine.measured_latency();
            println!(
                "chain running — round-trip ≈ {:.1} ms (chain {:.1} ms)",
                latency.total_ms, latency.chain_latency_ms
            );
            println!("press Enter to stop...");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);

            engine.stop();
            if let Some(file) = save {
                // Capture each plugin's state from the still-alive instances (the pool outlives the
                // engine).
                let slots: Vec<SessionSlot> = plugin_paths
                    .iter()
                    .zip(&instances)
                    .enumerate()
                    .map(|(i, (path, instance))| SessionSlot {
                        plugin_path: std::path::PathBuf::from(path),
                        component: instance.component(),
                        bypassed: bypass[i],
                    })
                    .collect();
                let session =
                    capture_session(&slots, Some(input), Some(output), AppSettings::default());
                match session.save(&file) {
                    Ok(()) => println!("saved session to {file}"),
                    Err(error) => eprintln!("failed to save session: {error:?}"),
                }
            }
        }
        Err(error) => {
            eprintln!("chain failed: {error:?}");
            std::process::exit(1);
        }
    }
    // `modules` drops here, after the engine and its chain have been torn down above.
}

#[cfg(not(windows))]
fn run_chain_command(_args: Vec<String>) {
    eprintln!("the VST3 chain is Windows-only");
    std::process::exit(2);
}

/// `galad session <file>` — restore a saved session (devices + plugins + order + state + bypass) and
/// run it.
#[cfg(windows)]
fn run_session_command(file: Option<String>) {
    use vst3::Steinberg::Vst::IHostApplication;

    use crate::audio::AudioEngine;
    use crate::session::HostSession;
    use crate::vst3_host::{ChainProcessor, HostContext, ProcessDriver, restore_pool};

    let Some(file) = file else {
        eprintln!("usage: galad session <session.toml>");
        std::process::exit(2);
    };
    let session = match HostSession::load(&file) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("failed to load {file}: {error:?}");
            std::process::exit(1);
        }
    };
    let (Some(input), Some(output)) = (session.input.clone(), session.output.clone()) else {
        eprintln!("session {file} is missing input/output devices");
        std::process::exit(1);
    };

    let host = HostContext::new()
        .to_com_ptr::<IHostApplication>()
        .expect("host exposes IHostApplication");

    // The pool (modules + instances) must outlive the engine/chain; bind first so it drops *after* the
    // engine. Prepare at the input device's actual rate (the rate the engine runs at), not a default.
    let sample_rate = crate::audio::device_sample_rate(&input)
        .map(|rate| rate as f64)
        .unwrap_or(48_000.0);
    let pool = match restore_pool(&session, &host) {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to restore session: {error:?}");
            std::process::exit(1);
        }
    };
    let driver = ProcessDriver::new(sample_rate, 4096);
    for slot in &pool {
        if let Err(error) = driver.prepare(&slot.instance) {
            eprintln!("failed to prepare plugin: {error:?}");
            std::process::exit(1);
        }
    }
    let bypass: Vec<bool> = session.chain.iter().map(|slot| slot.bypassed).collect();
    let chain = ChainProcessor::new(
        pool.iter().map(|slot| slot.instance.clone()).collect(),
        bypass,
        4096,
    );

    match AudioEngine::start_with_chain(input, output, Box::new(chain)) {
        Ok(mut engine) => {
            let latency = engine.measured_latency();
            println!(
                "session restored — round-trip ≈ {:.1} ms (chain {:.1} ms)",
                latency.total_ms, latency.chain_latency_ms
            );
            println!("press Enter to stop...");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            engine.stop();
        }
        Err(error) => {
            eprintln!("session failed: {error:?}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn run_session_command(_file: Option<String>) {
    eprintln!("session restore is Windows-only");
    std::process::exit(2);
}

/// `galad spike <path.vst3>` — load a real module and report the host-side protocol smoke results.
fn run_spike_command(path: Option<String>) {
    let Some(path) = path else {
        eprintln!("usage: galad spike <path-to.vst3>");
        std::process::exit(2);
    };
    match vst3_host::run_spike(std::path::Path::new(&path)) {
        Ok(report) => {
            println!("loaded audio class : {}", report.class_name);
            println!("latency (samples)  : {}", report.latency_samples);
            println!("silence stays silent: {}", report.silence_ok);
            println!("sine passes bit-exact: {}", report.sine_ok);
        }
        Err(error) => {
            eprintln!("spike failed: {error:?}");
            std::process::exit(1);
        }
    }
}
