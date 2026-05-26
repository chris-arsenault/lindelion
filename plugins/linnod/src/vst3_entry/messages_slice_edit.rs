#[derive(Debug, Clone, PartialEq)]
pub(super) enum LinnodSliceEditMessage {
    Select {
        slice_index: usize,
    },
    Name {
        slice_index: usize,
        name: String,
    },
    GainDb {
        slice_index: usize,
        gain_db: f32,
    },
    Pan {
        slice_index: usize,
        pan: f32,
    },
    Pitch {
        slice_index: usize,
        semitones: i32,
        cents: f32,
    },
    Reverse {
        slice_index: usize,
        reverse: bool,
    },
    PlaybackOverride {
        slice_index: usize,
        enabled: bool,
    },
    PlaybackMode {
        slice_index: usize,
        mode: PlaybackMode,
    },
    Envelope {
        slice_index: usize,
        envelope: EnvelopeConfig,
    },
    Offsets {
        slice_index: usize,
        start_offset_ms: f32,
        end_offset_ms: f32,
    },
    FilterCutoff {
        slice_index: usize,
        cutoff_hz: f32,
    },
}

impl LinnodSliceEditMessage {
    pub(super) fn encode(&self) -> Vec<u8> {
        match self {
            Self::Select { slice_index } => format!("select\n{slice_index}\n"),
            Self::Name { slice_index, name } => format!("name\n{slice_index}\n{name}"),
            Self::GainDb {
                slice_index,
                gain_db,
            } => format!("gain_db\n{slice_index}\n{gain_db:.8}"),
            Self::Pan { slice_index, pan } => format!("pan\n{slice_index}\n{pan:.8}"),
            Self::Pitch {
                slice_index,
                semitones,
                cents,
            } => format!("pitch\n{slice_index}\n{semitones},{cents:.8}"),
            Self::Reverse {
                slice_index,
                reverse,
            } => format!("reverse\n{slice_index}\n{}", u8::from(*reverse)),
            Self::PlaybackOverride {
                slice_index,
                enabled,
            } => format!("playback_override\n{slice_index}\n{}", u8::from(*enabled)),
            Self::PlaybackMode { slice_index, mode } => {
                format!("playback_mode\n{slice_index}\n{}", playback_mode_id(*mode))
            }
            Self::Envelope {
                slice_index,
                envelope,
            } => {
                format!("envelope\n{slice_index}\n{}", encode_envelope(*envelope))
            }
            Self::Offsets {
                slice_index,
                start_offset_ms,
                end_offset_ms,
            } => format!("offsets\n{slice_index}\n{start_offset_ms:.8},{end_offset_ms:.8}"),
            Self::FilterCutoff {
                slice_index,
                cutoff_hz,
            } => format!("filter_cutoff\n{slice_index}\n{cutoff_hz:.8}"),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.splitn(3, '\n');
        let kind = parts.next()?;
        let slice_index = parts.next()?.parse().ok()?;
        let value = parts.next().unwrap_or_default();
        decode_slice_edit_message(kind, slice_index, value)
    }
}

fn decode_slice_edit_message(
    kind: &str,
    slice_index: usize,
    value: &str,
) -> Option<LinnodSliceEditMessage> {
    decode_slice_basic_edit(kind, slice_index, value)
        .or_else(|| decode_slice_playback_edit(kind, slice_index, value))
        .or_else(|| decode_slice_range_edit(kind, slice_index, value))
}

fn decode_slice_basic_edit(
    kind: &str,
    slice_index: usize,
    value: &str,
) -> Option<LinnodSliceEditMessage> {
    match kind {
        "select" => Some(LinnodSliceEditMessage::Select { slice_index }),
        "name" => Some(LinnodSliceEditMessage::Name {
            slice_index,
            name: value.to_string(),
        }),
        "gain_db" => Some(LinnodSliceEditMessage::GainDb {
            slice_index,
            gain_db: value.parse().ok()?,
        }),
        "pan" => Some(LinnodSliceEditMessage::Pan {
            slice_index,
            pan: value.parse().ok()?,
        }),
        "pitch" => decode_slice_pitch_edit(slice_index, value),
        "reverse" => Some(LinnodSliceEditMessage::Reverse {
            slice_index,
            reverse: bool_from_id(value)?,
        }),
        _ => None,
    }
}

fn decode_slice_pitch_edit(slice_index: usize, value: &str) -> Option<LinnodSliceEditMessage> {
    let (semitones, cents) = value.split_once(',')?;
    Some(LinnodSliceEditMessage::Pitch {
        slice_index,
        semitones: semitones.parse().ok()?,
        cents: cents.parse().ok()?,
    })
}

fn decode_slice_playback_edit(
    kind: &str,
    slice_index: usize,
    value: &str,
) -> Option<LinnodSliceEditMessage> {
    match kind {
        "playback_override" => Some(LinnodSliceEditMessage::PlaybackOverride {
            slice_index,
            enabled: bool_from_id(value)?,
        }),
        "playback_mode" => Some(LinnodSliceEditMessage::PlaybackMode {
            slice_index,
            mode: playback_mode_from_id(value.parse().ok()?)?,
        }),
        "envelope" => Some(LinnodSliceEditMessage::Envelope {
            slice_index,
            envelope: decode_envelope(value)?,
        }),
        _ => None,
    }
}

fn decode_slice_range_edit(
    kind: &str,
    slice_index: usize,
    value: &str,
) -> Option<LinnodSliceEditMessage> {
    match kind {
        "offsets" => decode_slice_offsets_edit(slice_index, value),
        "filter_cutoff" => Some(LinnodSliceEditMessage::FilterCutoff {
            slice_index,
            cutoff_hz: value.parse().ok()?,
        }),
        _ => None,
    }
}

fn decode_slice_offsets_edit(slice_index: usize, value: &str) -> Option<LinnodSliceEditMessage> {
    let (start, end) = value.split_once(',')?;
    Some(LinnodSliceEditMessage::Offsets {
        slice_index,
        start_offset_ms: start.parse().ok()?,
        end_offset_ms: end.parse().ok()?,
    })
}
