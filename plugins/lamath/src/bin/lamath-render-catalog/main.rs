mod catalog;
mod cli;
mod manifest;
mod render;
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
        cli::Command::Render(selection) => {
            let catalog = catalog::catalog_cases();
            let selected =
                catalog::selected_cases(&catalog, &selection).map_err(|error| error.to_string())?;
            let report = writer::write_catalog(&options.out_dir, &selection, &selected)
                .map_err(|error| error.to_string())?;
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
}
