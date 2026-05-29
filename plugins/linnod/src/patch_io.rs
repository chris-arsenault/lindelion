use std::path::Path;

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};

use crate::patch::LinnodPatch;

pub const FORMAT_VERSION: u32 = 1;
pub type PatchIoError = TomlPatchError;

const PATCH_FORMAT: TomlPatchFormat<LinnodPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(patch: &LinnodPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT.to_toml_string(patch)
}

pub fn from_toml_str(input: &str) -> Result<LinnodPatch, PatchIoError> {
    let mut patch = PATCH_FORMAT.from_toml_str(input)?;
    patch.normalize_layout();
    Ok(patch)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &LinnodPatch) -> Result<(), PatchIoError> {
    PATCH_FORMAT.save_patch(path, patch)
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<LinnodPatch, PatchIoError> {
    let mut patch = PATCH_FORMAT.load_patch(path)?;
    patch.normalize_layout();
    Ok(patch)
}

pub fn to_plugin_state(patch: &LinnodPatch) -> Result<PluginState, PatchIoError> {
    PATCH_FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<LinnodPatch, PatchIoError> {
    let mut patch = PATCH_FORMAT.from_plugin_state(state)?;
    patch.normalize_layout();
    Ok(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{
        ChokeGroupId, EngineConfig, EnvelopeConfig, PadEdit, PadId, PitchOffset,
        PitchShiftAlgorithm, PlaybackEdit, PlaybackMode, SLICE_COUNT, SliceEdit, TriggerMode,
    };
    use lindelion_midi::{RootNote, Scale};
    use lindelion_onset_detect::{MarkerKind, SliceMarker};
    use lindelion_sample_library::SampleReference;

    #[test]
    fn patch_toml_roundtrips_schema() {
        let patch = schema_roundtrip_patch();

        let encoded = to_toml_string(&patch).unwrap();
        let decoded = from_toml_str(&encoded).unwrap();

        assert_encoded_schema(&encoded);
        assert_decoded_patch_header(&decoded);
        assert_decoded_source_markers(&decoded);
        assert_decoded_slice_state(&decoded);
    }

    fn schema_roundtrip_patch() -> LinnodPatch {
        let mut patch = LinnodPatch {
            name: "Roundtrip".to_string(),
            engine: EngineConfig {
                pitch_shift_algorithm: PitchShiftAlgorithm::ResampleStretch,
            },
            trigger_mode: TriggerMode::Chromatic,
            ..LinnodPatch::default()
        };
        patch.source_sample = Some(SampleReference::new("hash", "Samples/source.wav"));
        patch.markers = vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 12_000,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 18_000,
                kind: MarkerKind::User,
            },
        ];
        patch.tuning.scale = Scale::PentatonicMinor;
        patch.tuning.root = RootNote::FSharp;
        patch.apply_playback_edit(PlaybackEdit::Mode(PlaybackMode::Continue));
        patch.apply_playback_edit(PlaybackEdit::Envelope(EnvelopeConfig {
            attack_ms: 2.0,
            decay_ms: 15.0,
            sustain: 0.9,
            release_ms: 160.0,
        }));
        patch.apply_slice_edit(
            2,
            SliceEdit::Pitch(PitchOffset {
                semitones: 7,
                cents: -12.5,
            }),
        );
        patch.apply_slice_edit(2, SliceEdit::PlaybackOverride(true));
        patch.apply_slice_edit(2, SliceEdit::PlaybackMode(PlaybackMode::Looped));
        patch.apply_pad_edit(PadId(3), PadEdit::ChokeGroup(Some(ChokeGroupId(2))));
        patch.apply_slice_edit(
            2,
            SliceEdit::Envelope(EnvelopeConfig {
                attack_ms: 5.0,
                decay_ms: 20.0,
                sustain: 0.75,
                release_ms: 80.0,
            }),
        );
        patch
    }

    fn assert_encoded_schema(encoded: &str) {
        assert!(encoded.contains("format_version = 1"));
        assert!(encoded.contains("[patch.engine]"));
        assert!(encoded.contains("[patch.playback]"));
        assert!(encoded.contains("[patch.tuning]"));
        assert!(encoded.contains("[[patch.markers]]"));
        assert!(encoded.contains("kind = \"Auto\""));
        assert!(encoded.contains("kind = \"User\""));
        assert!(encoded.contains("choke_group"));
        assert!(!encoded.contains("analysis"));
    }

    fn assert_decoded_patch_header(decoded: &LinnodPatch) {
        assert_eq!(decoded.name, "Roundtrip");
        assert_eq!(
            decoded.engine.pitch_shift_algorithm,
            PitchShiftAlgorithm::ResampleStretch
        );
        assert_eq!(decoded.trigger_mode, TriggerMode::Chromatic);
        assert_eq!(decoded.playback.mode, PlaybackMode::Continue);
        assert_eq!(decoded.playback.envelope.release_ms, 160.0);
        assert_eq!(decoded.tuning.scale, Scale::PentatonicMinor);
        assert_eq!(decoded.tuning.root, RootNote::FSharp);
        assert_eq!(decoded.pad_map[2].choke_group, Some(ChokeGroupId(2)));
    }

    fn assert_decoded_source_markers(decoded: &LinnodPatch) {
        assert_eq!(
            decoded.source_sample.as_ref().unwrap().last_known_path,
            std::path::PathBuf::from("Samples/source.wav")
        );
        assert_eq!(
            decoded.markers,
            vec![
                SliceMarker {
                    position_samples: 0,
                    kind: MarkerKind::Auto,
                },
                SliceMarker {
                    position_samples: 12_000,
                    kind: MarkerKind::Auto,
                },
                SliceMarker {
                    position_samples: 18_000,
                    kind: MarkerKind::User,
                },
            ]
        );
    }

    fn assert_decoded_slice_state(decoded: &LinnodPatch) {
        assert_eq!(decoded.slices[2].pitch.semitones, 7);
        assert!(decoded.slices[2].use_playback_override);
        assert_eq!(decoded.slices[2].playback_mode, PlaybackMode::Looped);
        assert_eq!(decoded.slices.len(), SLICE_COUNT);
    }

    #[test]
    fn plugin_state_roundtrips_patch_native_slice_edits() {
        let mut patch = LinnodPatch::default();
        patch.apply_slice_edit(4, SliceEdit::Name("Air Bend".to_string()));
        patch.apply_slice_edit(4, SliceEdit::Pan(0.25));
        patch.apply_slice_edit(4, SliceEdit::Reverse(true));

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.slices[4].name, "Air Bend");
        assert_eq!(restored.slices[4].pan, 0.25);
        assert!(restored.slices[4].reverse);
    }
}
