use std::{env, fmt, path::PathBuf};

pub(crate) const DEFAULT_OUTPUT_DIR: &str = "review/lamath-render-catalog";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub(crate) command: Command,
    pub(crate) out_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    List,
    AnalyzeTubeTaps(String),
    AnalyzeBowDiagnostic(String),
    Render(RenderSelection),
    /// Re-emit `manifest.toml`/`index.md` from the static catalog, reusing the metrics already
    /// stored for each rendered WAV. Writes no audio, so file timestamps are preserved — used to
    /// upgrade the manifest to a new schema without a destructive re-render.
    RegenManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenderSelection {
    All,
    Group(String),
    Case(String),
    /// All cases carrying the given tag (e.g. `mesh`, `tube`, `scale`).
    Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliError {
    MissingValue(&'static str),
    MultipleSelections,
    UnknownArgument(String),
}

impl CliOptions {
    pub(crate) fn parse_env() -> Result<Self, CliError> {
        let args: Vec<String> = env::args().skip(1).collect();
        let env_out = env::var_os("LAMATH_REVIEW_DIR").map(PathBuf::from);
        parse_strings(args, env_out)
    }
}

fn parse_strings(args: Vec<String>, env_out: Option<PathBuf>) -> Result<CliOptions, CliError> {
    let mut command = None;
    let mut out_dir = env_out.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_DIR));
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => set_command(&mut command, Command::Help)?,
            "--list" => set_command(&mut command, Command::List)?,
            "--tube-tap-analysis" => {
                let case = value_after(&args, index, "--tube-tap-analysis")?;
                index += 1;
                set_command(&mut command, Command::AnalyzeTubeTaps(case))?;
            }
            "--bow-diagnostic" => {
                let case = value_after(&args, index, "--bow-diagnostic")?;
                index += 1;
                set_command(&mut command, Command::AnalyzeBowDiagnostic(case))?;
            }
            "--all" => set_command(&mut command, Command::Render(RenderSelection::All))?,
            "--regen-manifest" => set_command(&mut command, Command::RegenManifest)?,
            "--group" => {
                let group = value_after(&args, index, "--group")?;
                index += 1;
                set_command(&mut command, Command::Render(RenderSelection::Group(group)))?;
            }
            "--case" => {
                let case = value_after(&args, index, "--case")?;
                index += 1;
                set_command(&mut command, Command::Render(RenderSelection::Case(case)))?;
            }
            "--tag" => {
                let tag = value_after(&args, index, "--tag")?;
                index += 1;
                set_command(&mut command, Command::Render(RenderSelection::Tag(tag)))?;
            }
            "--out" => {
                out_dir = PathBuf::from(value_after(&args, index, "--out")?);
                index += 1;
            }
            other => return Err(CliError::UnknownArgument(other.to_string())),
        }
        index += 1;
    }

    Ok(CliOptions {
        command: command.unwrap_or(Command::Render(RenderSelection::All)),
        out_dir,
    })
}

fn set_command(slot: &mut Option<Command>, command: Command) -> Result<(), CliError> {
    if slot.is_some() {
        return Err(CliError::MultipleSelections);
    }
    *slot = Some(command);
    Ok(())
}

fn value_after(args: &[String], index: usize, flag: &'static str) -> Result<String, CliError> {
    let Some(value) = args.get(index + 1) else {
        return Err(CliError::MissingValue(flag));
    };
    if value.starts_with("--") {
        return Err(CliError::MissingValue(flag));
    }
    Ok(value.clone())
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue(flag) => write!(formatter, "{flag} requires a value"),
            Self::MultipleSelections => write!(
                formatter,
                "choose only one of --list, --tube-tap-analysis, --bow-diagnostic, --all, --group, --tag, or --case"
            ),
            Self::UnknownArgument(argument) => write!(formatter, "unknown argument: {argument}"),
        }
    }
}

impl fmt::Display for RenderSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(formatter, "all"),
            Self::Group(group) => write!(formatter, "group {group}"),
            Self::Case(case) => write!(formatter, "case {case}"),
            Self::Tag(tag) => write!(formatter, "tag {tag}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_selection_modes() {
        assert_eq!(parse(&["--list"]).unwrap().command, Command::List);
        assert_eq!(
            parse(&["--all"]).unwrap().command,
            Command::Render(RenderSelection::All)
        );
        assert_eq!(
            parse(&["--group", "baseline_dynamics"]).unwrap().command,
            Command::Render(RenderSelection::Group("baseline_dynamics".to_string()))
        );
        assert_eq!(
            parse(&["--case", "baseline_modal_c4_v100"])
                .unwrap()
                .command,
            Command::Render(RenderSelection::Case("baseline_modal_c4_v100".to_string()))
        );
        assert_eq!(
            parse(&["--tag", "mesh"]).unwrap().command,
            Command::Render(RenderSelection::Tag("mesh".to_string()))
        );
        assert_eq!(
            parse(&["--bow-diagnostic", "driver_string_bow_smooth_c4_v100"])
                .unwrap()
                .command,
            Command::AnalyzeBowDiagnostic("driver_string_bow_smooth_c4_v100".to_string())
        );
    }

    #[test]
    fn cli_parses_output_roots() {
        let out = parse(&["--all", "--out", "review/custom"]).unwrap();
        assert_eq!(out.out_dir, PathBuf::from("review/custom"));
        let env_out = parse_with_env(&["--all"], Some("review/from-env")).unwrap();
        assert_eq!(env_out.out_dir, PathBuf::from("review/from-env"));
        assert_eq!(
            parse_with_env(&["--all"], None).unwrap().out_dir,
            PathBuf::from(DEFAULT_OUTPUT_DIR)
        );
    }

    #[test]
    fn cli_rejects_ambiguous_or_incomplete_arguments() {
        assert_eq!(
            parse(&["--list", "--all"]).unwrap_err(),
            CliError::MultipleSelections
        );
        assert_eq!(
            parse(&["--group", "baseline_dynamics", "--case", "x"]).unwrap_err(),
            CliError::MultipleSelections
        );
        assert_eq!(
            parse(&["--out"]).unwrap_err(),
            CliError::MissingValue("--out")
        );
    }

    fn parse(args: &[&str]) -> Result<CliOptions, CliError> {
        parse_with_env(args, None)
    }

    fn parse_with_env(args: &[&str], env_out: Option<&str>) -> Result<CliOptions, CliError> {
        parse_strings(
            args.iter().map(|arg| (*arg).to_string()).collect(),
            env_out.map(PathBuf::from),
        )
    }
}
