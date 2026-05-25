use std::{
    fs, io,
    path::Path,
    process::ExitCode,
    process::{Command, Stdio},
};

const RUST_FILE_LINE_LIMIT: usize = 600;

mod bundle;
#[cfg(test)]
mod tests;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("workspace") | Some("check") => run_ci(),
        Some("lint-sizes") => run_size_lint(),
        Some("bundle") => bundle::run_bundle(args.collect()),
        Some("plugin-info") => bundle::run_plugin_info(args.collect()),
        Some("validator") => bundle::run_validator(args.collect()),
        Some("help") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown xtask command: {other}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn print_help() {
    println!("Lindelion xtask");
    println!();
    println!("Commands:");
    println!("  check|workspace       Run Rust fmt, clippy, and tests");
    println!("  lint-sizes            Check Rust source file size limits");
    println!("  bundle [plugin] [--target <triple>] [--bundle-dir <dir>]");
    println!("                           Build a macOS .vst3 bundle");
    println!("  plugin-info [plugin] [--field <name>]");
    println!("                           Print shared plugin bundle metadata");
    println!("  validator [plugin] [--bundle <path>] [--validator <path>]");
    println!("                           Run Steinberg validator against a built .vst3");
}

fn run_ci() -> ExitCode {
    let commands = [
        CargoCommand {
            label: "rustfmt",
            args: &["fmt", "--all", "--", "--check"],
        },
        CargoCommand {
            label: "clippy",
            args: &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--release",
                "--",
                "-D",
                "warnings",
                "-W",
                "clippy::cognitive_complexity",
                "-W",
                "clippy::too_many_lines",
            ],
        },
    ];

    for command in commands {
        println!("Running {}...", command.label);
        let status = Command::new("cargo")
            .args(command.args)
            .stdin(Stdio::null())
            .status();

        match status {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("{} failed with status {status}", command.label);
                return ExitCode::from(status.code().unwrap_or(1) as u8);
            }
            Err(error) => {
                eprintln!("failed to run {}: {error}", command.label);
                return ExitCode::FAILURE;
            }
        }
    }

    println!("Running size lint...");
    if let Err(error) = lint_rust_file_sizes(Path::new(".")) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    println!("Running test...");
    let status = Command::new("cargo")
        .args(["test", "--workspace"])
        .stdin(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("test failed with status {status}");
            return ExitCode::from(status.code().unwrap_or(1) as u8);
        }
        Err(error) => {
            eprintln!("failed to run test: {error}");
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn run_size_lint() -> ExitCode {
    match lint_rust_file_sizes(Path::new(".")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn lint_rust_file_sizes(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for dir in ["crates", "plugins", "xtask"] {
        collect_rust_file_size_violations(&root.join(dir), root, &mut violations)
            .map_err(|error| format!("failed to lint Rust file sizes: {error}"))?;
    }

    if violations.is_empty() {
        return Ok(());
    }

    let mut message = String::from("Rust file size lint failed:\n");
    for violation in violations {
        message.push_str(&format!(
            "  {} has {} lines; limit is {}\n",
            violation.path, violation.lines, violation.limit
        ));
    }
    Err(message)
}

fn collect_rust_file_size_violations(
    dir: &Path,
    root: &Path,
    violations: &mut Vec<FileSizeViolation>,
) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_rust_file_size_violations(&path, root, violations)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            check_rust_file_size(&path, root, violations)?;
        }
    }

    Ok(())
}

fn check_rust_file_size(
    path: &Path,
    root: &Path,
    violations: &mut Vec<FileSizeViolation>,
) -> io::Result<()> {
    let relative = relative_slash_path(path, root);
    let limit = rust_file_line_limit(&relative);
    let lines = fs::read_to_string(path)?.lines().count();
    if lines > limit {
        violations.push(FileSizeViolation {
            path: relative,
            lines,
            limit,
        });
    }
    Ok(())
}

fn rust_file_line_limit(_path: &str) -> usize {
    RUST_FILE_LINE_LIMIT
}

fn relative_slash_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

struct FileSizeViolation {
    path: String,
    lines: usize,
    limit: usize,
}

struct CargoCommand {
    label: &'static str,
    args: &'static [&'static str],
}
