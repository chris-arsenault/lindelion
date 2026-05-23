use std::{fs, path::Path};

use crate::{DecodedSample, SampleDecodeError};

pub fn decode_wav_mono(path: &Path) -> Result<DecodedSample, SampleDecodeError> {
    let bytes = fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    }

    let mut offset = 12;
    let mut format = None;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let len = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.saturating_add(len).min(bytes.len());

        match id {
            b"fmt " if len >= 16 => {
                format = Some(WavFormat {
                    audio_format: u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                    channels: u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                    sample_rate: u32::from_le_bytes(
                        bytes[start + 4..start + 8].try_into().unwrap(),
                    ),
                    bits_per_sample: u16::from_le_bytes(
                        bytes[start + 14..start + 16].try_into().unwrap(),
                    ),
                });
            }
            b"data" => data = Some((start, end)),
            _ => {}
        }

        offset = end + (len % 2);
    }

    let Some(format) = format else {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    };
    let Some((data_start, data_end)) = data else {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    };

    let frame_samples = decode_wav_samples(&bytes[data_start..data_end], format, path)?;
    let mono = mix_to_mono(&frame_samples, format.channels);
    Ok(DecodedSample {
        samples: mono,
        sample_rate: format.sample_rate,
        channels: format.channels,
    })
}

#[cfg(feature = "wav-decoder")]
#[derive(Debug, Clone, Copy)]
struct WavFormat {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

#[cfg(feature = "wav-decoder")]
fn decode_wav_samples(
    data: &[u8],
    format: WavFormat,
    path: &Path,
) -> Result<Vec<f32>, SampleDecodeError> {
    match (format.audio_format, format.bits_per_sample) {
        (1, 16) => Ok(data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes(chunk.try_into().unwrap()) as f32 / i16::MAX as f32)
            .collect()),
        (1, 24) => Ok(data.chunks_exact(3).map(decode_i24).collect()),
        (1, 32) => Ok(data
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()) as f32 / i32::MAX as f32)
            .collect()),
        (3, 32) => Ok(data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()).clamp(-1.0, 1.0))
            .collect()),
        _ => Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf())),
    }
}

#[cfg(feature = "wav-decoder")]
fn decode_i24(chunk: &[u8]) -> f32 {
    let sign = if chunk[2] & 0x80 == 0 { 0 } else { 0xFF };
    let value = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], sign]);
    (value as f32 / 8_388_607.0).clamp(-1.0, 1.0)
}

#[cfg(feature = "wav-decoder")]
fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}
