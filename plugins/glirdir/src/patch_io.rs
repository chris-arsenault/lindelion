use std::{
    fs, io,
    path::{Path, PathBuf},
};

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};
use lindelion_sample_library::LibraryPaths;

use crate::patch::{GlirdirPatch, ScratchpadAudio, ScratchpadMetadata};

pub const FORMAT_VERSION: u32 = 1;
pub const PLUGIN_STATE_FORMAT_VERSION: u32 = 3;

const PATCH_FORMAT: TomlPatchFormat<GlirdirPatch> = TomlPatchFormat::new(FORMAT_VERSION);
const STATE_MAGIC: [u8; 4] = *b"GLD1";
const SCRATCHPAD_MAGIC_V1: [u8; 4] = *b"GSA1";
const SCRATCHPAD_MAGIC_V2: [u8; 4] = *b"GSA2";
const STATE_HEADER_BYTES: usize = 12;
const SCRATCHPAD_V1_HEADER_BYTES: usize = 12;
const SCRATCHPAD_V2_HEADER_BYTES: usize = 17;
const MAX_PATCH_TOML_BYTES: usize = 1_048_576;
const MAX_SCRATCHPAD_BYTES: usize = 24 * 1_048_576;
const MAX_SCRATCHPAD_SAMPLES: usize =
    (MAX_SCRATCHPAD_BYTES - SCRATCHPAD_V2_HEADER_BYTES) / std::mem::size_of::<f32>();
const LEGACY_BINARY_STATE_FORMAT_VERSION: u32 = 2;

#[derive(Debug)]
pub enum PatchIoError {
    Patch(TomlPatchError),
    UnsupportedStateVersion { found: u32, supported: u32 },
    CorruptState,
    PatchTomlTooLarge { bytes: usize, max: usize },
    ScratchpadTooLarge { samples: usize, max: usize },
    StatePayloadTooLarge { bytes: usize, max: usize },
}

impl From<TomlPatchError> for PatchIoError {
    fn from(value: TomlPatchError) -> Self {
        Self::Patch(value)
    }
}

impl From<io::Error> for PatchIoError {
    fn from(value: io::Error) -> Self {
        Self::Patch(TomlPatchError::Io(value))
    }
}

pub fn to_toml_string(patch: &GlirdirPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT
        .to_toml_string(patch)
        .map_err(PatchIoError::from)
}

pub fn from_toml_str(input: &str) -> Result<GlirdirPatch, PatchIoError> {
    PATCH_FORMAT
        .from_toml_str(input)
        .map_err(PatchIoError::from)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &GlirdirPatch) -> Result<(), PatchIoError> {
    PATCH_FORMAT.save_patch(path, patch)?;
    Ok(())
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<GlirdirPatch, PatchIoError> {
    PATCH_FORMAT.load_patch(path).map_err(PatchIoError::from)
}

pub fn to_plugin_state(patch: &GlirdirPatch) -> Result<PluginState, PatchIoError> {
    let patch_toml = PATCH_FORMAT.to_toml_string(&patch_settings_only(patch))?;
    guard_patch_toml_len(patch_toml.len())?;
    let scratchpad = encode_scratchpad(patch.scratchpad.as_ref())?;
    guard_state_payload_len(STATE_HEADER_BYTES + patch_toml.len() + scratchpad.len())?;

    let mut payload = Vec::with_capacity(STATE_HEADER_BYTES + patch_toml.len() + scratchpad.len());
    payload.extend_from_slice(&STATE_MAGIC);
    payload.extend_from_slice(&(patch_toml.len() as u32).to_le_bytes());
    payload.extend_from_slice(&(scratchpad.len() as u32).to_le_bytes());
    payload.extend_from_slice(patch_toml.as_bytes());
    payload.extend_from_slice(&scratchpad);

    Ok(PluginState {
        format_version: PLUGIN_STATE_FORMAT_VERSION,
        payload,
    })
}

pub fn from_plugin_state(state: PluginState) -> Result<GlirdirPatch, PatchIoError> {
    match state.format_version {
        FORMAT_VERSION => PATCH_FORMAT
            .from_plugin_state(state)
            .map_err(PatchIoError::from),
        LEGACY_BINARY_STATE_FORMAT_VERSION | PLUGIN_STATE_FORMAT_VERSION => {
            decode_plugin_state_payload(&state.payload)
        }
        found if found > PLUGIN_STATE_FORMAT_VERSION => {
            Err(PatchIoError::UnsupportedStateVersion {
                found,
                supported: PLUGIN_STATE_FORMAT_VERSION,
            })
        }
        _ => Err(PatchIoError::CorruptState),
    }
}

pub fn save_library_patch(
    paths: &LibraryPaths,
    patch: &GlirdirPatch,
) -> Result<PathBuf, PatchIoError> {
    fs::create_dir_all(&paths.patches)?;
    let path = paths
        .patches
        .join(format!("{}.toml", sanitize_patch_name(&patch.name)));
    save_patch(&path, patch)?;
    Ok(path)
}

pub fn load_library_patch(path: impl AsRef<Path>) -> Result<GlirdirPatch, PatchIoError> {
    load_patch(path)
}

fn sanitize_patch_name(name: &str) -> String {
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

fn patch_settings_only(patch: &GlirdirPatch) -> GlirdirPatch {
    GlirdirPatch {
        name: patch.name.clone(),
        capture: patch.capture,
        analysis: patch.analysis,
        quantize: patch.quantize.clone(),
        audition: patch.audition,
        scratchpad: None,
    }
}

fn decode_plugin_state_payload(payload: &[u8]) -> Result<GlirdirPatch, PatchIoError> {
    guard_state_payload_len(payload.len())?;
    let (patch_toml, scratchpad_payload) = state_payload_parts(payload)?;
    let patch_text = std::str::from_utf8(patch_toml).map_err(|_| PatchIoError::CorruptState)?;
    let mut patch = PATCH_FORMAT.from_toml_str(patch_text)?;
    patch.scratchpad = decode_scratchpad(scratchpad_payload)?;
    Ok(patch)
}

fn state_payload_parts(payload: &[u8]) -> Result<(&[u8], &[u8]), PatchIoError> {
    if payload.len() < STATE_HEADER_BYTES || payload[..4] != STATE_MAGIC {
        return Err(PatchIoError::CorruptState);
    }

    let patch_len = u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;
    let scratchpad_len = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    guard_patch_toml_len(patch_len)?;
    guard_scratchpad_payload_len(scratchpad_len)?;

    let payload_len = STATE_HEADER_BYTES
        .checked_add(patch_len)
        .and_then(|len| len.checked_add(scratchpad_len))
        .ok_or(PatchIoError::CorruptState)?;
    if payload_len != payload.len() {
        return Err(PatchIoError::CorruptState);
    }

    let patch_start = STATE_HEADER_BYTES;
    let scratchpad_start = patch_start + patch_len;
    Ok((
        &payload[patch_start..scratchpad_start],
        &payload[scratchpad_start..],
    ))
}

fn encode_scratchpad(scratchpad: Option<&ScratchpadAudio>) -> Result<Vec<u8>, PatchIoError> {
    let Some(scratchpad) = scratchpad else {
        return Ok(Vec::new());
    };
    guard_scratchpad_samples_len(scratchpad.samples.len())?;
    let byte_len =
        SCRATCHPAD_V2_HEADER_BYTES + scratchpad.samples.len() * std::mem::size_of::<f32>();
    guard_scratchpad_payload_len(byte_len)?;

    let mut payload = Vec::with_capacity(byte_len);
    payload.extend_from_slice(&SCRATCHPAD_MAGIC_V2);
    payload.extend_from_slice(&scratchpad.sample_rate.to_le_bytes());
    payload.extend_from_slice(&(scratchpad.samples.len() as u32).to_le_bytes());
    payload.extend_from_slice(&scratchpad.metadata.bpm.to_le_bytes());
    payload.push(scratchpad.metadata.time_signature_numerator);
    payload.push(scratchpad.metadata.time_signature_denominator);
    payload.push(scratchpad.metadata.capture_bars);
    for sample in &scratchpad.samples {
        let sample = if sample.is_finite() { *sample } else { 0.0 };
        payload.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(payload)
}

fn decode_scratchpad(payload: &[u8]) -> Result<Option<ScratchpadAudio>, PatchIoError> {
    if payload.is_empty() {
        return Ok(None);
    }
    if payload.len() < 4 {
        return Err(PatchIoError::CorruptState);
    }

    if payload[..4] == SCRATCHPAD_MAGIC_V1 {
        decode_scratchpad_v1(payload)
    } else if payload[..4] == SCRATCHPAD_MAGIC_V2 {
        decode_scratchpad_v2(payload)
    } else {
        Err(PatchIoError::CorruptState)
    }
}

fn decode_scratchpad_v1(payload: &[u8]) -> Result<Option<ScratchpadAudio>, PatchIoError> {
    if payload.len() < SCRATCHPAD_V1_HEADER_BYTES {
        return Err(PatchIoError::CorruptState);
    }
    let sample_rate = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    let sample_count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    decode_scratchpad_samples(
        payload,
        SCRATCHPAD_V1_HEADER_BYTES,
        sample_rate,
        sample_count,
        ScratchpadMetadata::default(),
    )
}

fn decode_scratchpad_v2(payload: &[u8]) -> Result<Option<ScratchpadAudio>, PatchIoError> {
    if payload.len() < SCRATCHPAD_V2_HEADER_BYTES {
        return Err(PatchIoError::CorruptState);
    }
    let sample_rate = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    let sample_count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    let bpm = u16::from_le_bytes(payload[12..14].try_into().unwrap());
    let metadata = ScratchpadMetadata::new(
        f64::from(bpm),
        u16::from(payload[14]),
        u16::from(payload[15]),
        payload[16],
    );
    decode_scratchpad_samples(
        payload,
        SCRATCHPAD_V2_HEADER_BYTES,
        sample_rate,
        sample_count,
        metadata,
    )
}

fn decode_scratchpad_samples(
    payload: &[u8],
    header_bytes: usize,
    sample_rate: u32,
    sample_count: usize,
    metadata: ScratchpadMetadata,
) -> Result<Option<ScratchpadAudio>, PatchIoError> {
    guard_scratchpad_samples_len(sample_count)?;
    let expected_len = header_bytes
        .checked_add(
            sample_count
                .checked_mul(std::mem::size_of::<f32>())
                .ok_or(PatchIoError::CorruptState)?,
        )
        .ok_or(PatchIoError::CorruptState)?;
    if payload.len() != expected_len {
        return Err(PatchIoError::CorruptState);
    }

    let samples = payload[header_bytes..]
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    Ok(Some(ScratchpadAudio::with_metadata(
        sample_rate,
        metadata,
        samples,
    )))
}

fn guard_patch_toml_len(bytes: usize) -> Result<(), PatchIoError> {
    if bytes > MAX_PATCH_TOML_BYTES {
        Err(PatchIoError::PatchTomlTooLarge {
            bytes,
            max: MAX_PATCH_TOML_BYTES,
        })
    } else {
        Ok(())
    }
}

fn guard_scratchpad_samples_len(samples: usize) -> Result<(), PatchIoError> {
    if samples > MAX_SCRATCHPAD_SAMPLES {
        Err(PatchIoError::ScratchpadTooLarge {
            samples,
            max: MAX_SCRATCHPAD_SAMPLES,
        })
    } else {
        Ok(())
    }
}

fn guard_scratchpad_payload_len(bytes: usize) -> Result<(), PatchIoError> {
    if bytes > MAX_SCRATCHPAD_BYTES {
        Err(PatchIoError::StatePayloadTooLarge {
            bytes,
            max: MAX_SCRATCHPAD_BYTES,
        })
    } else {
        Ok(())
    }
}

fn guard_state_payload_len(bytes: usize) -> Result<(), PatchIoError> {
    let max = STATE_HEADER_BYTES + MAX_PATCH_TOML_BYTES + MAX_SCRATCHPAD_BYTES;
    if bytes > max {
        Err(PatchIoError::StatePayloadTooLarge { bytes, max })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureBars, ScratchpadAudio, ScratchpadMetadata};

    #[test]
    fn patch_toml_roundtrips_through_shared_format() {
        let patch = GlirdirPatch {
            name: "Glirdir State".to_string(),
            scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.0, 0.25, -0.25])),
            ..GlirdirPatch::default()
        };

        let encoded = to_toml_string(&patch).unwrap();
        let decoded = from_toml_str(&encoded).unwrap();

        assert_eq!(decoded.name, "Glirdir State");
        assert_eq!(decoded.scratchpad.unwrap().samples.len(), 3);
    }

    #[test]
    fn plugin_state_roundtrips() {
        let patch = GlirdirPatch {
            capture: crate::CaptureSettings {
                bars: CaptureBars::Sixteen,
                ..crate::CaptureSettings::default()
            },
            scratchpad: Some(ScratchpadAudio::with_metadata(
                44_100,
                ScratchpadMetadata::new(135.0, 7, 8, 16),
                vec![0.0, 0.25, f32::NAN, f32::INFINITY],
            )),
            ..GlirdirPatch::default()
        };

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.capture.bars, CaptureBars::Sixteen);
        let scratchpad = restored.scratchpad.expect("scratchpad should roundtrip");
        assert_eq!(scratchpad.sample_rate, 44_100);
        assert_eq!(scratchpad.metadata.bpm, 135);
        assert_eq!(scratchpad.metadata.time_signature_numerator, 7);
        assert_eq!(scratchpad.metadata.time_signature_denominator, 8);
        assert_eq!(scratchpad.metadata.capture_bars, 16);
        assert_eq!(scratchpad.samples, vec![0.0, 0.25, 0.0, 0.0]);
    }

    #[test]
    fn empty_scratchpad_plugin_state_roundtrips() {
        let patch = GlirdirPatch {
            name: "Settings Only".to_string(),
            scratchpad: None,
            ..GlirdirPatch::default()
        };

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.name, "Settings Only");
        assert!(restored.scratchpad.is_none());
    }

    #[test]
    fn plugin_state_keeps_scratchpad_out_of_toml_payload() {
        let patch = GlirdirPatch {
            scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.25; 512])),
            ..GlirdirPatch::default()
        };

        let state = to_plugin_state(&patch).unwrap();
        let (patch_toml, scratchpad_payload) = state_payload_parts(&state.payload).unwrap();
        let patch_toml = std::str::from_utf8(patch_toml).unwrap();

        assert_eq!(state.format_version, PLUGIN_STATE_FORMAT_VERSION);
        assert!(!patch_toml.contains("scratchpad"));
        assert!(!patch_toml.contains("samples"));
        assert!(!scratchpad_payload.is_empty());
    }

    #[test]
    fn corrupted_plugin_state_payload_fails_cleanly() {
        let state = PluginState {
            format_version: PLUGIN_STATE_FORMAT_VERSION,
            payload: b"not a glirdir state".to_vec(),
        };

        let error = from_plugin_state(state).unwrap_err();

        assert!(matches!(error, PatchIoError::CorruptState));
    }

    #[test]
    fn forward_plugin_state_version_fails_cleanly() {
        let state = PluginState {
            format_version: PLUGIN_STATE_FORMAT_VERSION + 1,
            payload: Vec::new(),
        };

        let error = from_plugin_state(state).unwrap_err();

        assert!(matches!(
            error,
            PatchIoError::UnsupportedStateVersion {
                found,
                supported: PLUGIN_STATE_FORMAT_VERSION
            } if found == PLUGIN_STATE_FORMAT_VERSION + 1
        ));
    }

    #[test]
    fn oversized_scratchpad_payload_is_rejected_before_allocation() {
        let patch_toml = to_toml_string(&GlirdirPatch::default()).unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(&STATE_MAGIC);
        payload.extend_from_slice(&(patch_toml.len() as u32).to_le_bytes());
        payload.extend_from_slice(&((MAX_SCRATCHPAD_BYTES + 1) as u32).to_le_bytes());
        payload.extend_from_slice(patch_toml.as_bytes());
        let state = PluginState {
            format_version: PLUGIN_STATE_FORMAT_VERSION,
            payload,
        };

        let error = from_plugin_state(state).unwrap_err();

        assert!(matches!(error, PatchIoError::StatePayloadTooLarge { .. }));
    }

    #[test]
    fn legacy_toml_plugin_state_still_loads() {
        let patch = GlirdirPatch {
            scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.1, 0.2])),
            ..GlirdirPatch::default()
        };
        let state = PATCH_FORMAT.to_plugin_state(&patch).unwrap();

        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.scratchpad.unwrap().samples, vec![0.1, 0.2]);
    }
}
