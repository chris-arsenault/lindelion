#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct EditorTelemetry {
    pub(super) left_peak: f32,
    pub(super) right_peak: f32,
    pub(super) left_rms: f32,
    pub(super) right_rms: f32,
    pub(super) active_voices: f32,
    pub(super) sidechain_required: bool,
    pub(super) sidechain_input_detected: bool,
    pub(super) sidechain_signal_active: bool,
    pub(super) audio_note_detected: bool,
    pub(super) audio_note_pitch_confidence: f32,
}

impl From<ResonatorTelemetry> for EditorTelemetry {
    fn from(value: ResonatorTelemetry) -> Self {
        Self {
            left_peak: value.left_peak,
            right_peak: value.right_peak,
            left_rms: value.left_rms,
            right_rms: value.right_rms,
            active_voices: value.active_voices as f32,
            sidechain_required: value.sidechain.required,
            sidechain_input_detected: value.sidechain.input_detected,
            sidechain_signal_active: value.sidechain.signal_active,
            audio_note_detected: value.sidechain.note_detected,
            audio_note_pitch_confidence: value.sidechain.pitch_confidence,
        }
    }
}

pub(super) fn encode_telemetry(telemetry: ResonatorTelemetry) -> String {
    format!(
        "{:.8},{:.8},{:.8},{:.8},{},{},{},{},{},{:.8}",
        telemetry.left_peak,
        telemetry.right_peak,
        telemetry.left_rms,
        telemetry.right_rms,
        telemetry.active_voices,
        bool_field(telemetry.sidechain.required),
        bool_field(telemetry.sidechain.input_detected),
        bool_field(telemetry.sidechain.signal_active),
        bool_field(telemetry.sidechain.note_detected),
        telemetry.sidechain.pitch_confidence,
    )
}

#[allow(clippy::cognitive_complexity)]
pub(super) fn decode_telemetry(payload: &[u8]) -> Option<EditorTelemetry> {
    let text = std::str::from_utf8(payload).ok()?;
    let mut parts = text.split(',');
    let left_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let right_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let left_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let right_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let active_voices = parts.next()?.parse::<f32>().ok()?.clamp(0.0, 64.0);
    let sidechain_required = bool_value(parts.next()?)?;
    let sidechain_input_detected = bool_value(parts.next()?)?;
    let sidechain_signal_active = bool_value(parts.next()?)?;
    let audio_note_detected = bool_value(parts.next()?)?;
    let audio_note_pitch_confidence = finite_unit_telemetry(parts.next()?.parse().ok()?);
    if parts.next().is_some() {
        return None;
    }
    Some(EditorTelemetry {
        left_peak,
        right_peak,
        left_rms,
        right_rms,
        active_voices,
        sidechain_required,
        sidechain_input_detected,
        sidechain_signal_active,
        audio_note_detected,
        audio_note_pitch_confidence,
    })
}

fn finite_telemetry(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 64.0)
    } else {
        0.0
    }
}

fn finite_unit_telemetry(value: f32) -> f32 {
    finite_telemetry(value).clamp(0.0, 1.0)
}

fn bool_field(value: bool) -> u8 {
    u8::from(value)
}

fn bool_value(value: &str) -> Option<bool> {
    match value {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

pub(super) fn default_library_paths() -> LibraryPaths {
    lindelion_sample_library::music_library_paths(DEFAULT_LIBRARY_DIR)
}

fn default_library_root() -> PathBuf {
    lindelion_sample_library::music_library_root(DEFAULT_LIBRARY_DIR)
}

fn open_default_sample_library()
-> Result<FileSampleLibrary, lindelion_sample_library::SampleLibraryError> {
    FileSampleLibrary::open(default_library_paths())
}

fn ensure_excitation_slot(patch: &mut ResonatorSynthPatch, slot_index: usize) {
    let target_len = (slot_index + 1).min(crate::dsp::MAX_EXCITATION_LAYERS);
    while patch.excitation_slots.len() < target_len {
        patch
            .excitation_slots
            .push(crate::ExcitationSlot::default());
    }
}

fn processor_patch_from_controller_patch(patch: &ResonatorSynthPatch) -> ResonatorSynthPatch {
    let mut patch = patch.clone();
    let paths = default_library_paths();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_relative() {
            let candidate = paths.root.join(&reference.last_known_path);
            if candidate.exists() {
                reference.last_known_path = candidate;
            }
        }
    }
    patch
}

fn resolve_patch_samples_for_loaded_path(patch: &mut ResonatorSynthPatch, patch_path: &Path) {
    let Some(patch_dir) = patch_path.parent() else {
        return;
    };
    let default_root = default_library_root();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_absolute() {
            continue;
        }
        let relative = reference.last_known_path.clone();
        for candidate in [patch_dir.join(&relative), default_root.join(&relative)] {
            if candidate.exists() {
                reference.last_known_path = candidate;
                break;
            }
        }
    }
}

fn export_patch_bundle(directory: &Path, patch: &ResonatorSynthPatch) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let samples_dir = directory.join("Samples");
    fs::create_dir_all(&samples_dir)?;

    let mut exported = patch.clone();
    let default_root = default_library_root();
    for slot in &mut exported.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        let source = if reference.last_known_path.is_absolute() {
            reference.last_known_path.clone()
        } else {
            default_root.join(&reference.last_known_path)
        };
        if !source.is_file() {
            continue;
        }
        let filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sample.wav");
        let target = samples_dir.join(filename);
        fs::copy(&source, &target)?;
        reference.last_known_path = PathBuf::from("Samples").join(filename);
    }

    let patch_path = directory.join(format!("{}.toml", sanitize_patch_filename(&exported.name)));
    patch_io::save_patch(&patch_path, &exported)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    Ok(patch_path)
}

fn sanitize_patch_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        "Untitled".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditorPatchSummary {
    pub(super) patch_name: String,
    pub(super) slots: [EditorSlotSummary; 4],
    pub(super) library_samples: Vec<EditorSampleSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditorSampleSummary {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) preview: Vec<EditorWaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct EditorWaveformPoint {
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) rms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorSlotSummary {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) sample_backed: bool,
    pub(super) pitch_track: bool,
    pub(super) looping: bool,
}

impl EditorPatchSummary {
    pub(super) fn from_patch(patch: &crate::ResonatorSynthPatch) -> Self {
        Self::from_patch_and_library(patch, &[])
    }

    fn from_patch_and_library(
        patch: &crate::ResonatorSynthPatch,
        samples: &[SampleMetadata],
    ) -> Self {
        Self {
            patch_name: patch.name.clone(),
            slots: std::array::from_fn(|index| {
                let slot = patch.excitation_slots.get(index);
                EditorSlotSummary::from_slot(index, slot)
            }),
            library_samples: samples
                .iter()
                .map(EditorSampleSummary::from_metadata)
                .collect(),
        }
    }
}

impl EditorSampleSummary {
    fn from_metadata(metadata: &SampleMetadata) -> Self {
        let duration = metadata.duration_ms as f32 / 1_000.0;
        let detail = format!(
            "{duration:.2}s  {} Hz  {}ch",
            metadata.sample_rate, metadata.channels
        );
        Self {
            label: metadata.filename.clone(),
            detail,
            preview: waveform_points(&metadata.waveform_preview),
        }
    }
}

fn waveform_points(preview: &SampleWaveformPreview) -> Vec<EditorWaveformPoint> {
    preview
        .points
        .iter()
        .map(|point| EditorWaveformPoint {
            min: point.min,
            max: point.max,
            rms: point.rms,
        })
        .collect()
}

impl EditorSlotSummary {
    fn from_slot(index: usize, slot: Option<&crate::ExcitationSlot>) -> Self {
        let Some(slot) = slot else {
            return Self::empty(index);
        };

        let Some(reference) = slot.sample.as_ref() else {
            return if index == 0 {
                Self::builtin(slot)
            } else {
                Self::empty(index)
            };
        };

        let filename = reference
            .last_known_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Sample");
        Self {
            label: format!("Slot {}", index + 1),
            detail: filename.to_string(),
            sample_backed: true,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn builtin(slot: &crate::ExcitationSlot) -> Self {
        Self {
            label: "Slot 1".to_string(),
            detail: "Built-in pluck".to_string(),
            sample_backed: false,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn empty(index: usize) -> Self {
        Self {
            label: format!("Slot {}", index + 1),
            detail: "Empty layer".to_string(),
            sample_backed: false,
            pitch_track: false,
            looping: false,
        }
    }
}
