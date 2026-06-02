use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use crate::{DecodedSample, SampleDecodeError};

const WAV_HEADER_BYTES: u32 = 44;
const WAV_RIFF_DATA_OVERHEAD_BYTES: u32 = 36;
const STEREO_PCM16_BYTES_PER_FRAME: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoPcm16WavMetrics {
    pub sample_rate: u32,
    pub frames: usize,
    pub duration_seconds: f64,
    pub peak: f32,
    pub rms: f32,
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub data_bytes: u32,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StereoPcm16WavChannel {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StereoPcm16WavError {
    ChannelLengthMismatch {
        left: usize,
        right: usize,
    },
    Empty,
    InvalidSampleRate,
    NonFiniteSample {
        channel: StereoPcm16WavChannel,
        index: usize,
    },
    Silent,
    PeakOutOfRange {
        peak: f32,
    },
    DataTooLarge {
        frames: usize,
    },
    Io {
        kind: io::ErrorKind,
    },
}

pub fn write_wav_stereo_pcm16(
    path: &Path,
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Result<StereoPcm16WavMetrics, StereoPcm16WavError> {
    let metrics = validate_wav_stereo_pcm16(left, right, sample_rate)?;
    let mut file = fs::File::create(path).map_err(wav_io_error)?;
    file.write_all(b"RIFF").map_err(wav_io_error)?;
    file.write_all(&(WAV_RIFF_DATA_OVERHEAD_BYTES + metrics.data_bytes).to_le_bytes())
        .map_err(wav_io_error)?;
    file.write_all(b"WAVEfmt ").map_err(wav_io_error)?;
    file.write_all(&16u32.to_le_bytes()).map_err(wav_io_error)?;
    file.write_all(&1u16.to_le_bytes()).map_err(wav_io_error)?;
    file.write_all(&2u16.to_le_bytes()).map_err(wav_io_error)?;
    file.write_all(&sample_rate.to_le_bytes())
        .map_err(wav_io_error)?;
    file.write_all(&(sample_rate * STEREO_PCM16_BYTES_PER_FRAME as u32).to_le_bytes())
        .map_err(wav_io_error)?;
    file.write_all(&(STEREO_PCM16_BYTES_PER_FRAME as u16).to_le_bytes())
        .map_err(wav_io_error)?;
    file.write_all(&16u16.to_le_bytes()).map_err(wav_io_error)?;
    file.write_all(b"data").map_err(wav_io_error)?;
    file.write_all(&metrics.data_bytes.to_le_bytes())
        .map_err(wav_io_error)?;
    for (&left, &right) in left.iter().zip(right) {
        file.write_all(&pcm16(left).to_le_bytes())
            .map_err(wav_io_error)?;
        file.write_all(&pcm16(right).to_le_bytes())
            .map_err(wav_io_error)?;
    }
    Ok(metrics)
}

pub fn validate_wav_stereo_pcm16(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Result<StereoPcm16WavMetrics, StereoPcm16WavError> {
    validate_stereo_shape(left, right, sample_rate)?;
    let data_bytes = stereo_pcm16_data_bytes(left.len())?;
    let left_stats = channel_stats(left, StereoPcm16WavChannel::Left)?;
    let right_stats = channel_stats(right, StereoPcm16WavChannel::Right)?;
    let peak = left_stats.peak.max(right_stats.peak);
    if peak == 0.0 {
        return Err(StereoPcm16WavError::Silent);
    }
    if peak > 1.0 {
        return Err(StereoPcm16WavError::PeakOutOfRange { peak });
    }

    let rms = left_stats.rms.max(right_stats.rms);
    Ok(StereoPcm16WavMetrics {
        sample_rate,
        frames: left.len(),
        duration_seconds: left.len() as f64 / f64::from(sample_rate),
        peak,
        rms,
        peak_dbfs: amplitude_dbfs(peak),
        rms_dbfs: amplitude_dbfs(rms),
        data_bytes,
        file_bytes: u64::from(WAV_HEADER_BYTES) + u64::from(data_bytes),
    })
}

pub fn stereo_pcm16_data_bytes(frames: usize) -> Result<u32, StereoPcm16WavError> {
    let bytes = frames
        .checked_mul(STEREO_PCM16_BYTES_PER_FRAME)
        .and_then(|bytes| u32::try_from(bytes).ok())
        .filter(|bytes| *bytes <= u32::MAX - WAV_RIFF_DATA_OVERHEAD_BYTES);
    bytes.ok_or(StereoPcm16WavError::DataTooLarge { frames })
}

fn validate_stereo_shape(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Result<(), StereoPcm16WavError> {
    if left.len() != right.len() {
        return Err(StereoPcm16WavError::ChannelLengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    if left.is_empty() {
        return Err(StereoPcm16WavError::Empty);
    }
    if sample_rate == 0 {
        return Err(StereoPcm16WavError::InvalidSampleRate);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ChannelStats {
    peak: f32,
    rms: f32,
}

fn channel_stats(
    samples: &[f32],
    channel: StereoPcm16WavChannel,
) -> Result<ChannelStats, StereoPcm16WavError> {
    let mut peak = 0.0_f32;
    let mut square_sum = 0.0_f64;
    for (index, sample) in samples.iter().copied().enumerate() {
        if !sample.is_finite() {
            return Err(StereoPcm16WavError::NonFiniteSample { channel, index });
        }
        peak = peak.max(sample.abs());
        square_sum += f64::from(sample) * f64::from(sample);
    }
    Ok(ChannelStats {
        peak,
        rms: (square_sum / samples.len() as f64).sqrt() as f32,
    })
}

fn amplitude_dbfs(value: f32) -> f32 {
    20.0 * value.log10()
}

fn pcm16(sample: f32) -> i16 {
    debug_assert!(sample.is_finite());
    debug_assert!((-1.0..=1.0).contains(&sample));
    (sample * i16::MAX as f32).round() as i16
}

fn wav_io_error(error: io::Error) -> StereoPcm16WavError {
    StereoPcm16WavError::Io { kind: error.kind() }
}

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
