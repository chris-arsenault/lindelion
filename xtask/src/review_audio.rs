use std::{
    ffi::OsStr,
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const DEFAULT_ENCODER: &str = "ffmpeg";
const DEFAULT_BITRATE: &str = "192k";

pub(crate) fn run_compress_review_audio(args: Vec<String>) -> ExitCode {
    match CompressOptions::parse(args).and_then(compress_review_audio) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CompressError::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn print_help() {
    eprintln!(
        "usage: xtask compress-review-audio --source <dir> --out <dir> \
         [--encoder ffmpeg] [--bitrate 192k] [--dry-run]"
    );
}

fn compress_review_audio(options: CompressOptions) -> Result<(), CompressError> {
    if !options.source.exists() {
        return Err(CompressError::SourceMissing(options.source));
    }
    if !options.source.is_dir() {
        return Err(CompressError::SourceNotDirectory(options.source));
    }

    let wavs = collect_wavs(&options.source)?;
    let jobs = compression_jobs(&options.source, &options.out, &wavs)?;
    if jobs.is_empty() {
        return Err(CompressError::NoWavs(options.source));
    }

    if options.dry_run {
        print_summary(&options, jobs.len(), 0);
        return Ok(());
    }

    let mut total_bytes = 0;
    for job in &jobs {
        encode_mp3(&options, job)?;
        total_bytes += fs::metadata(&job.output).map_err(CompressError::Io)?.len();
    }
    print_summary(&options, jobs.len(), total_bytes);
    Ok(())
}

fn print_summary(options: &CompressOptions, files: usize, total_bytes: u64) {
    println!("Review audio compression");
    println!("source: {}", options.source.display());
    println!("output: {}", options.out.display());
    println!("format: mp3");
    println!("encoder: {}", options.encoder);
    println!("bitrate: {}", options.bitrate);
    println!("dry run: {}", options.dry_run);
    println!("files: {files}");
    if !options.dry_run {
        println!("total bytes: {total_bytes}");
    }
}

fn encode_mp3(options: &CompressOptions, job: &CompressionJob) -> Result<(), CompressError> {
    if let Some(parent) = job.output.parent() {
        fs::create_dir_all(parent).map_err(CompressError::Io)?;
    }
    let status = Command::new(&options.encoder)
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(&job.input)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg(&options.bitrate)
        .arg(&job.output)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| CompressError::EncoderSpawn {
            encoder: options.encoder.clone(),
            error,
        })?;
    if !status.success() {
        return Err(CompressError::EncoderFailed {
            input: job.input.clone(),
            output: job.output.clone(),
            status,
        });
    }
    let metadata = fs::metadata(&job.output).map_err(CompressError::Io)?;
    if metadata.len() == 0 {
        return Err(CompressError::EmptyOutput(job.output.clone()));
    }
    Ok(())
}

fn collect_wavs(root: &Path) -> Result<Vec<PathBuf>, CompressError> {
    let mut wavs = Vec::new();
    collect_wavs_inner(root, &mut wavs)?;
    wavs.sort();
    Ok(wavs)
}

fn collect_wavs_inner(dir: &Path, wavs: &mut Vec<PathBuf>) -> Result<(), CompressError> {
    for entry in fs::read_dir(dir).map_err(CompressError::Io)? {
        let entry = entry.map_err(CompressError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(CompressError::Io)?;
        if file_type.is_dir() {
            collect_wavs_inner(&path, wavs)?;
        } else if is_wav(&path) {
            wavs.push(path);
        }
    }
    Ok(())
}

fn compression_jobs(
    source: &Path,
    out: &Path,
    wavs: &[PathBuf],
) -> Result<Vec<CompressionJob>, CompressError> {
    wavs.iter()
        .map(|wav| {
            Ok(CompressionJob {
                input: wav.clone(),
                output: preview_path(source, out, wav)?,
            })
        })
        .collect()
}

fn preview_path(source: &Path, out: &Path, wav: &Path) -> Result<PathBuf, CompressError> {
    let mut relative = wav
        .strip_prefix(source)
        .map_err(|_| CompressError::OutsideSource(wav.to_path_buf()))?
        .to_path_buf();
    relative.set_extension("mp3");
    Ok(out.join(relative))
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompressOptions {
    source: PathBuf,
    out: PathBuf,
    encoder: String,
    bitrate: String,
    dry_run: bool,
}

impl CompressOptions {
    fn parse(args: Vec<String>) -> Result<Self, CompressError> {
        let mut source = None;
        let mut out = None;
        let mut encoder = DEFAULT_ENCODER.to_string();
        let mut bitrate = DEFAULT_BITRATE.to_string();
        let mut dry_run = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => return Err(CompressError::Help),
                "--source" => {
                    source = Some(PathBuf::from(value_after(&args, index, "--source")?));
                    index += 1;
                }
                "--out" => {
                    out = Some(PathBuf::from(value_after(&args, index, "--out")?));
                    index += 1;
                }
                "--encoder" => {
                    encoder = value_after(&args, index, "--encoder")?;
                    index += 1;
                }
                "--bitrate" => {
                    bitrate = value_after(&args, index, "--bitrate")?;
                    index += 1;
                }
                "--dry-run" => dry_run = true,
                other => return Err(CompressError::UnknownArgument(other.to_string())),
            }
            index += 1;
        }

        Ok(Self {
            source: source.ok_or(CompressError::MissingValue("--source"))?,
            out: out.ok_or(CompressError::MissingValue("--out"))?,
            encoder,
            bitrate,
            dry_run,
        })
    }
}

fn value_after(args: &[String], index: usize, flag: &'static str) -> Result<String, CompressError> {
    let Some(value) = args.get(index + 1) else {
        return Err(CompressError::MissingValue(flag));
    };
    if value.starts_with("--") {
        return Err(CompressError::MissingValue(flag));
    }
    Ok(value.clone())
}

#[derive(Debug)]
enum CompressError {
    Help,
    MissingValue(&'static str),
    UnknownArgument(String),
    SourceMissing(PathBuf),
    SourceNotDirectory(PathBuf),
    NoWavs(PathBuf),
    OutsideSource(PathBuf),
    EmptyOutput(PathBuf),
    EncoderSpawn {
        encoder: String,
        error: std::io::Error,
    },
    EncoderFailed {
        input: PathBuf,
        output: PathBuf,
        status: std::process::ExitStatus,
    },
    Io(std::io::Error),
}

impl fmt::Display for CompressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => write!(formatter, "help requested"),
            Self::MissingValue(flag) => write!(formatter, "{flag} requires a value"),
            Self::UnknownArgument(argument) => write!(formatter, "unknown argument: {argument}"),
            Self::SourceMissing(path) => write!(
                formatter,
                "source directory does not exist: {}",
                path.display()
            ),
            Self::SourceNotDirectory(path) => {
                write!(formatter, "source is not a directory: {}", path.display())
            }
            Self::NoWavs(path) => write!(
                formatter,
                "source contains no WAV files: {}",
                path.display()
            ),
            Self::OutsideSource(path) => write!(
                formatter,
                "WAV path is outside source root: {}",
                path.display()
            ),
            Self::EmptyOutput(path) => write!(
                formatter,
                "encoder wrote an empty output: {}",
                path.display()
            ),
            Self::EncoderSpawn { encoder, error } => {
                write!(formatter, "failed to run encoder {encoder}: {error}")
            }
            Self::EncoderFailed {
                input,
                output,
                status,
            } => write!(
                formatter,
                "encoder failed for {} -> {} with status {status}",
                input.display(),
                output.display()
            ),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompressionJob {
    input: PathBuf,
    output: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_paths_and_optional_encoder_settings() {
        let options = CompressOptions::parse(vec![
            "--source".into(),
            "review/source".into(),
            "--out".into(),
            "review/out".into(),
            "--encoder".into(),
            "ffmpeg-custom".into(),
            "--bitrate".into(),
            "128k".into(),
            "--dry-run".into(),
        ])
        .unwrap();

        assert_eq!(options.source, PathBuf::from("review/source"));
        assert_eq!(options.out, PathBuf::from("review/out"));
        assert_eq!(options.encoder, "ffmpeg-custom");
        assert_eq!(options.bitrate, "128k");
        assert!(options.dry_run);
    }

    #[test]
    fn parse_rejects_missing_required_paths() {
        assert!(matches!(
            CompressOptions::parse(vec!["--source".into(), "review/source".into()]),
            Err(CompressError::MissingValue("--out"))
        ));
        assert!(matches!(
            CompressOptions::parse(vec!["--out".into(), "review/out".into()]),
            Err(CompressError::MissingValue("--source"))
        ));
    }

    #[test]
    fn preview_paths_preserve_relative_directories_and_use_mp3_extension() {
        let source = Path::new("review/source");
        let out = Path::new("review/out");
        let wav = Path::new("review/source/01_group/example.wav");

        assert_eq!(
            preview_path(source, out, wav).unwrap(),
            PathBuf::from("review/out/01_group/example.mp3")
        );
    }

    #[test]
    fn wav_detection_is_case_insensitive() {
        assert!(is_wav(Path::new("a/b/c.wav")));
        assert!(is_wav(Path::new("a/b/c.WAV")));
        assert!(!is_wav(Path::new("a/b/c.mp3")));
    }
}
