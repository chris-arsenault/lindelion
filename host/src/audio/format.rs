//! Sample-format conversion and the negotiated stream-format descriptor.
//!
//! WASAPI presents audio as interleaved frames in a concrete sample type — shared mode is typically
//! 32-bit float, exclusive mode is often 16-bit PCM. The engine works in interleaved `f32`; these
//! converters bridge the device byte buffer and the engine. (i24/i32 are a later extension.)

/// Interleaved sample type a WASAPI stream presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 32-bit IEEE float (typical shared-mode mix format).
    F32,
    /// 16-bit signed PCM (common exclusive-mode format).
    I16,
}

impl SampleFormat {
    /// Bytes per sample for this format.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::F32 => 4,
            SampleFormat::I16 => 2,
        }
    }
}

/// A negotiated stream format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample: SampleFormat,
}

/// Convert interleaved device bytes (per `format`) into interleaved engine `f32`; returns samples
/// written (min of the two capacities). Allocation-free.
pub fn bytes_to_f32(format: SampleFormat, src: &[u8], dst: &mut [f32]) -> usize {
    match format {
        SampleFormat::F32 => {
            let n = (src.len() / 4).min(dst.len());
            for (i, slot) in dst.iter_mut().take(n).enumerate() {
                let bytes = [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]];
                *slot = f32::from_le_bytes(bytes);
            }
            n
        }
        SampleFormat::I16 => {
            let n = (src.len() / 2).min(dst.len());
            for (i, slot) in dst.iter_mut().take(n).enumerate() {
                let value = i16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                *slot = value as f32 / 32768.0;
            }
            n
        }
    }
}

/// Convert interleaved engine `f32` into interleaved device bytes (per `format`); returns samples
/// written (min of the two capacities). Allocation-free.
pub fn f32_to_bytes(format: SampleFormat, src: &[f32], dst: &mut [u8]) -> usize {
    match format {
        SampleFormat::F32 => {
            let n = src.len().min(dst.len() / 4);
            for (i, &sample) in src.iter().take(n).enumerate() {
                dst[i * 4..i * 4 + 4].copy_from_slice(&sample.to_le_bytes());
            }
            n
        }
        SampleFormat::I16 => {
            let n = src.len().min(dst.len() / 2);
            for (i, &sample) in src.iter().take(n).enumerate() {
                let value = (sample * 32767.0)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                dst[i * 2..i * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
            n
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_roundtrips_within_one_lsb() {
        for &value in &[i16::MIN, -100, -1, 0, 1, 100, i16::MAX] {
            let src = value.to_le_bytes();
            let mut floats = [0.0f32; 1];
            assert_eq!(bytes_to_f32(SampleFormat::I16, &src, &mut floats), 1);

            let mut back = [0u8; 2];
            assert_eq!(f32_to_bytes(SampleFormat::I16, &floats, &mut back), 1);

            let restored = i16::from_le_bytes(back);
            assert!(
                (restored as i32 - value as i32).abs() <= 1,
                "value={value} restored={restored}"
            );
        }
    }

    #[test]
    fn f32_is_identity() {
        let samples = [0.0f32, 0.5, -0.5, 1.0, -1.0, 0.123456];
        let mut bytes = [0u8; 24];
        assert_eq!(f32_to_bytes(SampleFormat::F32, &samples, &mut bytes), 6);

        let mut out = [0.0f32; 6];
        assert_eq!(bytes_to_f32(SampleFormat::F32, &bytes, &mut out), 6);

        assert_eq!(out, samples);
    }
}
