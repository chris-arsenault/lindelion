//! Stream mode + format/buffer negotiation policy.
//!
//! Pure decision logic: try **exclusive** event-driven mode first (lowest latency), fall back to
//! **shared**. The Windows `IsFormatSupported` / `GetDevicePeriod` calls (Step 7) feed this — here is
//! the rule they apply. Decision (Galad M2): exclusive primary, shared fallback; round-trip latency
//! **target ~20 ms** (the *measured* value is recorded at the M2 exit; the target is a goal, not a
//! hard gate).

use super::format::StreamFormat;

/// Target round-trip latency in milliseconds (a goal; the measured value is recorded at runtime).
pub const LATENCY_TARGET_MS: f64 = 20.0;

/// WASAPI share mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareMode {
    Exclusive,
    Shared,
}

/// A chosen stream configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    pub share_mode: ShareMode,
    pub format: StreamFormat,
    pub buffer_frames: u32,
}

/// Choose share mode, format, and buffer size: exclusive when supported, else shared.
pub fn choose(
    exclusive_supported: bool,
    exclusive_format: StreamFormat,
    exclusive_min_period_frames: u32,
    shared_format: StreamFormat,
    shared_default_period_frames: u32,
) -> StreamConfig {
    let (share_mode, format, min_period_frames) = if exclusive_supported {
        (
            ShareMode::Exclusive,
            exclusive_format,
            exclusive_min_period_frames,
        )
    } else {
        (
            ShareMode::Shared,
            shared_format,
            shared_default_period_frames,
        )
    };
    let buffer_frames = buffer_frames_for(format.sample_rate, min_period_frames);
    StreamConfig {
        share_mode,
        format,
        buffer_frames,
    }
}

/// Buffer frames targeting half the round-trip latency (capture + render ≈ the round trip), never
/// below the device's `min_period_frames`.
pub fn buffer_frames_for(sample_rate: u32, min_period_frames: u32) -> u32 {
    let half_target_seconds = (LATENCY_TARGET_MS / 1000.0) / 2.0;
    let target = (sample_rate as f64 * half_target_seconds).round() as u32;
    target.max(min_period_frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::SampleFormat;

    fn fmt(sample: SampleFormat) -> StreamFormat {
        StreamFormat {
            sample_rate: 48_000,
            channels: 2,
            sample,
        }
    }

    #[test]
    fn chooses_exclusive_when_supported() {
        let config = choose(
            true,
            fmt(SampleFormat::I16),
            240,
            fmt(SampleFormat::F32),
            1056,
        );
        assert_eq!(config.share_mode, ShareMode::Exclusive);
        assert_eq!(config.format.sample, SampleFormat::I16);
    }

    #[test]
    fn falls_back_to_shared_when_exclusive_unsupported() {
        let config = choose(
            false,
            fmt(SampleFormat::I16),
            240,
            fmt(SampleFormat::F32),
            1056,
        );
        assert_eq!(config.share_mode, ShareMode::Shared);
        assert_eq!(config.format.sample, SampleFormat::F32);
    }

    #[test]
    fn buffer_targets_half_latency_but_not_below_period() {
        // 48 kHz, ~20 ms round trip → half = 10 ms → 480 frames.
        assert_eq!(buffer_frames_for(48_000, 240), 480);
        // Clamped up to the device minimum period when that is larger.
        assert_eq!(buffer_frames_for(48_000, 600), 600);
    }
}
