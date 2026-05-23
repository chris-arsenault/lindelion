use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    process::{Command, Stdio},
};

const RUST_FILE_LINE_LIMIT: usize = 600;

#[cfg(test)]
mod tests;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("workspace") | Some("check") => run_ci(),
        Some("lint-sizes") => run_size_lint(),
        Some("bundle") => run_bundle(args.collect()),
        Some("validator") => {
            println!("Validator integration is not implemented yet.");
            println!(
                "Next step: locate Steinberg validator and run it against built .vst3 bundles."
            );
            ExitCode::SUCCESS
        }
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
    println!("  validator             Placeholder for Steinberg validator integration");
}

fn run_bundle(args: Vec<String>) -> ExitCode {
    let options = match BundleOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            print_help();
            return ExitCode::from(2);
        }
    };
    let spec = match BundleSpec::from_plugin(&options.plugin) {
        Some(spec) => spec,
        None => {
            eprintln!("Unsupported plugin for bundling: {}", options.plugin);
            return ExitCode::from(2);
        }
    };
    if !options.target_is_macos() {
        eprintln!("The VST3 bundle task currently targets macOS only.");
        eprintln!("Pass --target aarch64-apple-darwin or run on macOS without --target.");
        return ExitCode::from(2);
    }

    if let Err(error) = build_release(&spec, &options) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    match create_macos_vst3_bundle(&spec, &options) {
        Ok(bundle) => {
            println!("Built {}", bundle.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to create VST3 bundle: {error}");
            ExitCode::FAILURE
        }
    }
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

#[derive(Debug)]
struct BundleOptions {
    plugin: String,
    target: Option<String>,
    bundle_dir: Option<PathBuf>,
}

impl BundleOptions {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut plugin = None;
        let mut target = None;
        let mut bundle_dir = None;
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--target" => {
                    target = Some(
                        iter.next()
                            .ok_or_else(|| "--target requires a Rust target triple".to_string())?,
                    );
                }
                "--bundle-dir" => {
                    bundle_dir =
                        Some(PathBuf::from(iter.next().ok_or_else(|| {
                            "--bundle-dir requires an output directory".to_string()
                        })?));
                }
                "-h" | "--help" => return Err("bundle help requested".to_string()),
                value if value.starts_with('-') => {
                    return Err(format!("unknown bundle option: {value}"));
                }
                value => {
                    if plugin.replace(value.to_string()).is_some() {
                        return Err("bundle accepts at most one plugin name".to_string());
                    }
                }
            }
        }

        Ok(Self {
            plugin: plugin.unwrap_or_else(|| "lamath".to_string()),
            target,
            bundle_dir,
        })
    }

    fn target_is_macos(&self) -> bool {
        self.target
            .as_deref()
            .is_some_and(|target| target.contains("apple-darwin"))
            || (self.target.is_none() && cfg!(target_os = "macos"))
    }

    fn cargo_target_dir(&self) -> PathBuf {
        let mut dir = env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target"));
        if let Some(target) = &self.target {
            dir.push(target);
        }
        dir.push("release");
        dir
    }

    fn bundle_dir(&self) -> PathBuf {
        self.bundle_dir
            .clone()
            .or_else(|| env::var_os("LINDELION_BUNDLE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("target").join("bundles"))
    }
}

#[derive(Debug, Clone, Copy)]
struct BundleSpec {
    package: &'static str,
    bundle_name: &'static str,
    executable_name: &'static str,
    bundle_identifier: &'static str,
    library_stem: &'static str,
    sub_categories: &'static [&'static str],
    processor_cid: [u32; 4],
    controller_cid: [u32; 4],
    version: &'static str,
}

impl BundleSpec {
    fn from_plugin(plugin: &str) -> Option<Self> {
        match plugin {
            "lamath" => Some(Self {
                package: "lamath",
                bundle_name: "Lamath",
                executable_name: "Lamath",
                bundle_identifier: "com.ahara.lamath",
                library_stem: "lamath",
                sub_categories: &["Instrument", "Synth"],
                processor_cid: [0x4B410E03, 0x80AD49B6, 0x9B7D5479, 0xF4A9B0D1],
                controller_cid: [0x15C8B012, 0xF4B64F5E, 0x93D9AA38, 0x69383E3B],
                version: env!("CARGO_PKG_VERSION"),
            }),
            "glirdir" => Some(Self {
                package: "glirdir",
                bundle_name: "Glirdir",
                executable_name: "Glirdir",
                bundle_identifier: "com.ahara.glirdir",
                library_stem: "glirdir",
                sub_categories: &["Fx"],
                processor_cid: [0x7C2E2B8A, 0xB1C44F0D, 0xA6F92427, 0x6C9E0D5B],
                controller_cid: [0x0D0466D2, 0x53E446E5, 0x8E90CF13, 0x25B5E241],
                version: env!("CARGO_PKG_VERSION"),
            }),
            _ => None,
        }
    }
}

fn build_release(spec: &BundleSpec, options: &BundleOptions) -> Result<(), String> {
    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        spec.package.to_string(),
        "--release".to_string(),
    ];
    if let Some(target) = &options.target {
        args.push("--target".to_string());
        args.push(target.clone());
    }

    let status = Command::new("cargo")
        .args(&args)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| format!("failed to run cargo build: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo build failed with status {status}"))
    }
}

fn create_macos_vst3_bundle(spec: &BundleSpec, options: &BundleOptions) -> io::Result<PathBuf> {
    let bundle = options
        .bundle_dir()
        .join(format!("{}.vst3", spec.bundle_name));
    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let executable = macos.join(spec.executable_name);

    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    fs::create_dir_all(&macos)?;
    fs::create_dir_all(&resources)?;

    fs::copy(source_library_path(spec, options), &executable)?;
    fs::write(contents.join("Info.plist"), info_plist(spec))?;
    fs::write(contents.join("PkgInfo"), "BNDL????")?;
    fs::write(resources.join("moduleinfo.json"), module_info(spec))?;
    sign_bundle_if_available(&bundle);

    Ok(bundle)
}

fn source_library_path(spec: &BundleSpec, options: &BundleOptions) -> PathBuf {
    options
        .cargo_target_dir()
        .join(format!("lib{}.dylib", spec.library_stem))
}

fn info_plist(spec: &BundleSpec) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>{executable}</string>
  <key>CFBundleIdentifier</key>
  <string>{identifier}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>{name}</string>
  <key>CFBundlePackageType</key>
  <string>BNDL</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
  <key>CFBundleSignature</key>
  <string>????</string>
  <key>CFBundleSupportedPlatforms</key>
  <array>
    <string>MacOSX</string>
  </array>
  <key>CFBundleVersion</key>
  <string>{version}</string>
  <key>CSResourcesFileMapped</key>
  <true/>
</dict>
</plist>
"#,
        executable = escape_plist(spec.executable_name),
        identifier = escape_plist(spec.bundle_identifier),
        name = escape_plist(spec.bundle_name),
        version = escape_plist(spec.version),
    )
}

fn module_info(spec: &BundleSpec) -> String {
    format!(
        r#"{{
  "Name": "{name}",
  "Version": "{version}",
  "Factory Info": {{
    "Vendor": "Ahara",
    "URL": "https://ahara.io",
    "E-Mail": "",
    "Flags": {{
      "Unicode": true,
      "Classes Discardable": false,
      "Component Non Discardable": false
    }}
  }},
  "Classes": [
    {{
      "CID": "{processor_cid}",
      "Category": "Audio Module Class",
      "Name": "{name}",
      "Vendor": "Ahara",
      "Version": "{version}",
      "SDKVersion": "VST 3.8.0",
      "Sub Categories": [
{sub_categories}
      ],
      "Class Flags": 1,
      "Cardinality": 2147483647
    }},
    {{
      "CID": "{controller_cid}",
      "Category": "Component Controller Class",
      "Name": "{name} Controller",
      "Vendor": "Ahara",
      "Version": "{version}",
      "SDKVersion": "VST 3.8.0",
      "Class Flags": 0,
      "Cardinality": 2147483647
    }}
  ]
}}
"#,
        name = escape_json(spec.bundle_name),
        version = escape_json(spec.version),
        processor_cid = cid_hex(spec.processor_cid),
        controller_cid = cid_hex(spec.controller_cid),
        sub_categories = sub_categories_json(spec.sub_categories),
    )
}

fn sub_categories_json(sub_categories: &[&str]) -> String {
    sub_categories
        .iter()
        .map(|category| format!("        \"{}\"", escape_json(category)))
        .collect::<Vec<_>>()
        .join(",\n")
}

fn cid_hex(words: [u32; 4]) -> String {
    format!(
        "{:08X}{:08X}{:08X}{:08X}",
        words[0], words[1], words[2], words[3]
    )
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn escape_plist(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sign_bundle_if_available(bundle: &Path) {
    match Command::new("codesign")
        .args(["--force", "--sign", "-", "--timestamp=none"])
        .arg(bundle)
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(_) => {
            eprintln!(
                "codesign failed; bundle was left unsigned at {}",
                bundle.display()
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!(
                "codesign not found; bundle was left unsigned at {}",
                bundle.display()
            );
        }
        Err(error) => {
            eprintln!("failed to run codesign: {error}");
        }
    }
}
