use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use lindelion_plugin_metadata::{Vst3BundleMetadata, metadata_for_package};

pub(crate) fn run_bundle(args: Vec<String>) -> ExitCode {
    let options = match BundleOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
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

pub(crate) fn run_plugin_info(args: Vec<String>) -> ExitCode {
    let options = match PluginInfoOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let spec = match BundleSpec::from_plugin(&options.plugin) {
        Some(spec) => spec,
        None => {
            eprintln!("Unsupported plugin: {}", options.plugin);
            return ExitCode::from(2);
        }
    };

    if let Some(field) = options.field {
        let Some(value) = plugin_info_field(spec, &field) else {
            eprintln!("Unknown plugin-info field: {field}");
            return ExitCode::from(2);
        };
        println!("{value}");
    } else {
        print_plugin_info(spec);
    }

    ExitCode::SUCCESS
}

pub(crate) fn run_validator(args: Vec<String>) -> ExitCode {
    let options = match ValidatorOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let spec = match BundleSpec::from_plugin(&options.plugin) {
        Some(spec) => spec,
        None => {
            eprintln!("Unsupported plugin for validation: {}", options.plugin);
            return ExitCode::from(2);
        }
    };
    let bundle = options.bundle_path(spec);
    if !bundle.exists() {
        eprintln!("VST3 bundle does not exist: {}", bundle.display());
        return ExitCode::from(2);
    }

    let validator = options.validator_path();
    let status = Command::new(&validator)
        .arg(&bundle)
        .stdin(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("validator failed with status {status}");
            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "validator executable not found: {}. Set VST3_VALIDATOR or pass --validator.",
                validator.display()
            );
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("failed to run validator: {error}");
            ExitCode::FAILURE
        }
    }
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
        bundle_dir(self.bundle_dir.clone())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BundleSpec {
    pub(crate) metadata: Vst3BundleMetadata,
    pub(crate) version: &'static str,
}

impl BundleSpec {
    pub(crate) fn from_plugin(plugin: &str) -> Option<Self> {
        let metadata = metadata_for_package(plugin)?;
        Some(Self {
            metadata,
            version: env!("CARGO_PKG_VERSION"),
        })
    }
}

fn build_release(spec: &BundleSpec, options: &BundleOptions) -> Result<(), String> {
    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        spec.metadata.package.to_string(),
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
        .join(format!("{}.vst3", spec.metadata.bundle_name));
    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let executable = macos.join(spec.metadata.executable_name);

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
        .join(format!("lib{}.dylib", spec.metadata.library_stem))
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
        executable = escape_plist(spec.metadata.executable_name),
        identifier = escape_plist(spec.metadata.bundle_identifier),
        name = escape_plist(spec.metadata.bundle_name),
        version = escape_plist(spec.version),
    )
}

pub(crate) fn module_info(spec: &BundleSpec) -> String {
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
      "Name": "{controller_name}",
      "Vendor": "Ahara",
      "Version": "{version}",
      "SDKVersion": "VST 3.8.0",
      "Class Flags": 0,
      "Cardinality": 2147483647
    }}
  ]
}}
"#,
        name = escape_json(spec.metadata.bundle_name),
        version = escape_json(spec.version),
        processor_cid = cid_hex(spec.metadata.processor_cid),
        controller_cid = cid_hex(spec.metadata.controller_cid),
        controller_name = escape_json(spec.metadata.controller_name),
        sub_categories = sub_categories_json(spec.metadata.module_sub_categories),
    )
}

fn sub_categories_json(sub_categories: &[&str]) -> String {
    sub_categories
        .iter()
        .map(|category| format!("        \"{}\"", escape_json(category)))
        .collect::<Vec<_>>()
        .join(",\n")
}

pub(crate) fn cid_hex(words: [u32; 4]) -> String {
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

#[derive(Debug)]
struct PluginInfoOptions {
    plugin: String,
    field: Option<String>,
}

impl PluginInfoOptions {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut plugin = None;
        let mut field = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--field" => {
                    field = Some(
                        iter.next()
                            .ok_or_else(|| "--field requires a field name".to_string())?,
                    );
                }
                "-h" | "--help" => return Err("plugin-info help requested".to_string()),
                value if value.starts_with('-') => {
                    return Err(format!("unknown plugin-info option: {value}"));
                }
                value => {
                    if plugin.replace(value.to_string()).is_some() {
                        return Err("plugin-info accepts at most one plugin name".to_string());
                    }
                }
            }
        }

        Ok(Self {
            plugin: plugin.unwrap_or_else(|| "lamath".to_string()),
            field,
        })
    }
}

fn print_plugin_info(spec: BundleSpec) {
    for field in [
        "package",
        "bundle-name",
        "bundle-file",
        "executable",
        "identifier",
        "library-stem",
        "vst3-sub-categories",
        "module-sub-categories",
        "processor-cid",
        "controller-cid",
        "controller-name",
    ] {
        if let Some(value) = plugin_info_field(spec, field) {
            println!("{field}: {value}");
        }
    }
}

fn plugin_info_field(spec: BundleSpec, field: &str) -> Option<String> {
    let metadata = spec.metadata;
    match field {
        "package" => Some(metadata.package.to_string()),
        "bundle-name" => Some(metadata.bundle_name.to_string()),
        "bundle-file" => Some(format!("{}.vst3", metadata.bundle_name)),
        "executable" => Some(metadata.executable_name.to_string()),
        "identifier" => Some(metadata.bundle_identifier.to_string()),
        "library-stem" => Some(metadata.library_stem.to_string()),
        "vst3-sub-categories" => Some(metadata.vst3_sub_categories.to_string()),
        "module-sub-categories" => Some(metadata.module_sub_categories.join("|")),
        "processor-cid" => Some(cid_hex(metadata.processor_cid)),
        "controller-cid" => Some(cid_hex(metadata.controller_cid)),
        "controller-name" => Some(metadata.controller_name.to_string()),
        _ => None,
    }
}

#[derive(Debug)]
struct ValidatorOptions {
    plugin: String,
    bundle: Option<PathBuf>,
    bundle_dir: Option<PathBuf>,
    validator: Option<PathBuf>,
}

impl ValidatorOptions {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut plugin = None;
        let mut bundle = None;
        let mut bundle_dir = None;
        let mut validator = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--bundle" => {
                    bundle =
                        Some(PathBuf::from(iter.next().ok_or_else(|| {
                            "--bundle requires a .vst3 path".to_string()
                        })?));
                }
                "--bundle-dir" => {
                    bundle_dir =
                        Some(PathBuf::from(iter.next().ok_or_else(|| {
                            "--bundle-dir requires a bundle directory".to_string()
                        })?));
                }
                "--validator" => {
                    validator =
                        Some(PathBuf::from(iter.next().ok_or_else(|| {
                            "--validator requires an executable path".to_string()
                        })?));
                }
                "-h" | "--help" => return Err("validator help requested".to_string()),
                value if value.starts_with('-') => {
                    return Err(format!("unknown validator option: {value}"));
                }
                value => {
                    if plugin.replace(value.to_string()).is_some() {
                        return Err("validator accepts at most one plugin name".to_string());
                    }
                }
            }
        }

        Ok(Self {
            plugin: plugin.unwrap_or_else(|| "lamath".to_string()),
            bundle,
            bundle_dir,
            validator,
        })
    }

    fn bundle_path(&self, spec: BundleSpec) -> PathBuf {
        self.bundle.clone().unwrap_or_else(|| {
            bundle_dir(self.bundle_dir.clone()).join(format!("{}.vst3", spec.metadata.bundle_name))
        })
    }

    fn validator_path(&self) -> PathBuf {
        self.validator
            .clone()
            .or_else(|| env::var_os("VST3_VALIDATOR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("validator"))
    }
}

fn bundle_dir(explicit: Option<PathBuf>) -> PathBuf {
    explicit
        .or_else(|| env::var_os("LINDELION_BUNDLE_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("target").join("bundles"))
}
