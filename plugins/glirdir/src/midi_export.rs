use lindelion_midi::{MidiClip, RootNote, Scale};

use crate::{GlirdirPatch, patch::ScratchpadMetadata};

const MIDI_EXPORT_MAGIC: [u8; 4] = *b"GME1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MidiExportJob {
    pub sequence: u64,
    pub clip: MidiClip,
    pub file_name: String,
}

impl MidiExportJob {
    pub fn new(sequence: u64, patch: &GlirdirPatch, clip: &MidiClip) -> Self {
        Self {
            sequence,
            clip: clip.clone(),
            file_name: midi_file_name(patch, clip),
        }
    }

    pub fn export(self) -> Option<MidiExportPayload> {
        self.clip
            .to_smf_bytes()
            .ok()
            .map(|bytes| MidiExportPayload::new(self.file_name, bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MidiExportPayload {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

impl MidiExportPayload {
    pub fn new(file_name: impl AsRef<str>, bytes: Vec<u8>) -> Self {
        Self {
            file_name: sanitize_midi_file_name(file_name.as_ref()),
            bytes,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let name = self.file_name.as_bytes();
        let mut payload =
            Vec::with_capacity(MIDI_EXPORT_MAGIC.len() + 4 + name.len() + self.bytes.len());
        payload.extend_from_slice(&MIDI_EXPORT_MAGIC);
        payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
        payload.extend_from_slice(name);
        payload.extend_from_slice(&self.bytes);
        payload
    }

    pub fn decode(payload: &[u8]) -> Option<Self> {
        if payload.len() < 8 || payload[..4] != MIDI_EXPORT_MAGIC {
            return None;
        }
        let name_len = u32::from_le_bytes(payload[4..8].try_into().ok()?) as usize;
        let bytes_start = 8usize.checked_add(name_len)?;
        let name = std::str::from_utf8(payload.get(8..bytes_start)?).ok()?;
        Some(Self::new(name, payload.get(bytes_start..)?.to_vec()))
    }
}

pub(crate) fn empty_midi_export(patch: &GlirdirPatch) -> MidiExportPayload {
    let clip = MidiClip::empty_with_time_signature(
        patch.quantize.bpm.round().clamp(1.0, u16::MAX as f32) as u16,
        patch.quantize.time_signature_numerator,
        patch.quantize.time_signature_denominator,
    );
    MidiExportPayload::new(
        midi_file_name(patch, &clip),
        clip.to_smf_bytes().unwrap_or_default(),
    )
}

pub(crate) fn midi_file_name(patch: &GlirdirPatch, clip: &MidiClip) -> String {
    let key = format!(
        "{}{}",
        root_label(patch.quantize.root),
        scale_label(&patch.quantize.scale)
    );
    let metadata = patch
        .scratchpad
        .as_ref()
        .map(|scratchpad| scratchpad.metadata)
        .unwrap_or_else(|| ScratchpadMetadata {
            bpm: clip.bpm,
            capture_bars: patch.capture.bars.bars(),
            ..ScratchpadMetadata::default()
        });
    sanitize_midi_file_name(&format!(
        "glirdir-{key}-{}bar-{}bpm.mid",
        metadata.capture_bars, clip.bpm
    ))
}

pub(crate) fn sanitize_midi_file_name(input: &str) -> String {
    let mut file_name = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if !file_name.ends_with(".mid") {
        file_name.push_str(".mid");
    }
    if file_name == ".mid" {
        "glirdir.mid".to_string()
    } else {
        file_name
    }
}

fn root_label(root: RootNote) -> &'static str {
    match root {
        RootNote::C => "C",
        RootNote::CSharp => "Cs",
        RootNote::D => "D",
        RootNote::DSharp => "Ds",
        RootNote::E => "E",
        RootNote::F => "F",
        RootNote::FSharp => "Fs",
        RootNote::G => "G",
        RootNote::GSharp => "Gs",
        RootNote::A => "A",
        RootNote::ASharp => "As",
        RootNote::B => "B",
    }
}

fn scale_label(scale: &Scale) -> &'static str {
    match scale {
        Scale::Chromatic => "chrom",
        Scale::Major => "maj",
        Scale::NaturalMinor => "min",
        Scale::HarmonicMinor => "harm-min",
        Scale::MelodicMinor => "mel-min",
        Scale::PentatonicMajor => "pent-maj",
        Scale::PentatonicMinor => "pent-min",
        Scale::Blues => "blues",
        Scale::Dorian => "dor",
        Scale::Mixolydian => "mix",
        Scale::Custom(_) => "custom",
    }
}

#[cfg(test)]
mod tests {
    use lindelion_midi::{QuantizeSettings, RootNote, Scale};

    use super::*;
    use crate::{CaptureBars, CaptureSettings, ScratchpadAudio};

    #[test]
    fn midi_file_name_is_deterministic_and_safe() {
        let patch = GlirdirPatch {
            capture: CaptureSettings {
                bars: CaptureBars::Four,
                ..CaptureSettings::default()
            },
            quantize: QuantizeSettings {
                root: RootNote::C,
                scale: Scale::NaturalMinor,
                ..QuantizeSettings::default()
            },
            scratchpad: Some(ScratchpadAudio::with_metadata(
                48_000,
                ScratchpadMetadata::new(120.0, 4, 4, 4),
                vec![0.0],
            )),
            ..GlirdirPatch::default()
        };

        assert_eq!(
            midi_file_name(&patch, &MidiClip::empty(120)),
            "glirdir-Cmin-4bar-120bpm.mid"
        );
    }

    #[test]
    fn midi_export_payload_roundtrips_file_name_and_bytes() {
        let payload = MidiExportPayload::new("bad/name?.mid", b"MThd".to_vec());
        let decoded = MidiExportPayload::decode(&payload.encode()).unwrap();

        assert_eq!(decoded.file_name, "bad-name-.mid");
        assert_eq!(decoded.bytes, b"MThd");
    }
}
