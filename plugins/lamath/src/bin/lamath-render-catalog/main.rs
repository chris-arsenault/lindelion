mod catalog;
mod cli;
mod manifest;
mod render;
mod variant;
mod writer;

use std::process::ExitCode;

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
            print!("{}", catalog::catalog_listing());
            Ok(())
        }
        cli::Command::AnalyzeTubeTaps(case_id) => {
            let catalog = catalog::catalog_cases();
            let selected =
                catalog::selected_cases(&catalog, &cli::RenderSelection::Case(case_id.clone()))
                    .map_err(|error| error.to_string())?;
            let report =
                render::analyze_tube_taps(&selected[0]).map_err(|error| error.to_string())?;
            println!("{}", report.summary());
            Ok(())
        }
        cli::Command::AnalyzeBowDiagnostic(case_id) => {
            let catalog = catalog::catalog_cases();
            let selected =
                catalog::selected_cases(&catalog, &cli::RenderSelection::Case(case_id.clone()))
                    .map_err(|error| error.to_string())?;
            let report =
                render::analyze_bow_diagnostic(&selected[0]).map_err(|error| error.to_string())?;
            println!("{}", report.summary());
            Ok(())
        }
        cli::Command::Render(selection) => {
            let catalog = catalog::catalog_cases();
            let selected =
                catalog::selected_cases(&catalog, &selection).map_err(|error| error.to_string())?;
            let report = writer::write_catalog(&options.out_dir, &selection, &selected)
                .map_err(|error| error.to_string())?;
            println!("{}", report.summary());
            Ok(())
        }
        cli::Command::RegenManifest => {
            let report =
                writer::regen_manifest(&options.out_dir).map_err(|error| error.to_string())?;
            println!("{}", report.summary());
            Ok(())
        }
    }
}

fn print_help() {
    println!("Lamath render catalog");
    println!();
    println!("Usage:");
    println!("  lamath-render-catalog --list");
    println!("  lamath-render-catalog --all [--out <dir>]");
    println!("  lamath-render-catalog --group <group-id> [--out <dir>]");
    println!("  lamath-render-catalog --tag <tag> [--out <dir>]");
    println!("  lamath-render-catalog --case <case-id> [--out <dir>]");
    println!("  lamath-render-catalog --tube-tap-analysis <case-id>");
    println!("  lamath-render-catalog --bow-diagnostic <case-id>");
    println!("  lamath-render-catalog --regen-manifest [--out <dir>]");
}
