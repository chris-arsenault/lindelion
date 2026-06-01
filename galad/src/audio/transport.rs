//! Passthrough transport — the per-callback step the realtime thread runs, written once and shared
//! by the Linux test and the Windows engine (M2 Step 8).
//!
//! Capture: channel-adapt device input frames to stereo (the chain width) and push to the ring.
//! Render: pop stereo from the ring, channel-adapt to the output device's channels, and write the
//! output; an under-run (ring empty) writes silence. Allocation-free and lock-free (ADR-0001): it
//! operates on preallocated scratch, so nothing allocates per callback.

use super::channels::adapt;
use super::ring::AudioRing;

/// The VST3 chain runs stereo.
const CHAIN_CHANNELS: u16 = 2;

/// Moves audio from the capture device, through the ring, to the render device.
pub struct Transport {
    ring: AudioRing,
    input_channels: u16,
    output_channels: u16,
    capture_scratch: Vec<f32>,
    render_scratch: Vec<f32>,
}

impl Transport {
    /// A transport for `input_channels`→stereo→`output_channels`, sized for up to `max_frames` per
    /// callback and a ring holding `ring_capacity` samples.
    pub fn new(
        input_channels: u16,
        output_channels: u16,
        max_frames: usize,
        ring_capacity: usize,
    ) -> Self {
        let stereo_scratch = max_frames * CHAIN_CHANNELS as usize;
        Self {
            ring: AudioRing::with_capacity(ring_capacity),
            input_channels,
            output_channels,
            capture_scratch: vec![0.0; stereo_scratch],
            render_scratch: vec![0.0; stereo_scratch],
        }
    }

    /// Adapt `device_frames` (interleaved, `input_channels` wide) to stereo and push to the ring.
    pub fn capture(&mut self, device_frames: &[f32]) {
        let in_ch = self.input_channels.max(1) as usize;
        let frames = device_frames.len() / in_ch;
        let stereo_len = (frames * CHAIN_CHANNELS as usize).min(self.capture_scratch.len());
        let scratch = &mut self.capture_scratch[..stereo_len];
        adapt(self.input_channels, CHAIN_CHANNELS, device_frames, scratch);
        self.ring.push(scratch);
    }

    /// Pop stereo from the ring, adapt to `output_channels`, and write `out`; under-run → silence.
    pub fn render(&mut self, out: &mut [f32]) {
        self.render_through(out, |_| {});
    }

    /// Like [`render`](Self::render), but run `process_stereo` over the popped **stereo** block
    /// in place between the ring-pop and the output channel-adapt. The engine injects the VST3 chain
    /// here (M3 Step 6); `Transport` itself stays plugin-agnostic. `process_stereo` runs on the audio
    /// thread, so it must be allocation-free and lock-free (ADR-0001).
    pub fn render_through(&mut self, out: &mut [f32], process_stereo: impl FnOnce(&mut [f32])) {
        let out_ch = self.output_channels.max(1) as usize;
        let frames = out.len() / out_ch;
        let stereo_len = (frames * CHAIN_CHANNELS as usize).min(self.render_scratch.len());
        let scratch = &mut self.render_scratch[..stereo_len];
        let popped = self.ring.pop(scratch);
        for slot in scratch[popped..].iter_mut() {
            *slot = 0.0;
        }
        process_stereo(scratch);
        adapt(CHAIN_CHANNELS, self.output_channels, scratch, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_through_applies_processor_between_pop_and_output() {
        let frames = 8;
        let mut transport = Transport::new(2, 2, frames, 4096);

        let input: Vec<f32> = (0..frames * 2).map(|i| i as f32).collect();
        transport.capture(&input);
        let mut out = vec![0.0f32; frames * 2];
        transport.render_through(&mut out, |stereo| stereo.iter_mut().for_each(|x| *x *= 0.5));

        let expected: Vec<f32> = input.iter().map(|&x| x * 0.5).collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn stereo_loopback_passes_signal_through() {
        let frames = 64;
        let mut transport = Transport::new(2, 2, frames, 4096);

        let input: Vec<f32> = (0..frames * 2).map(|i| (i as f32 * 0.01).sin()).collect();
        transport.capture(&input);
        let mut out = vec![0.0f32; frames * 2];
        transport.render(&mut out);

        assert_eq!(out, input);
    }

    #[test]
    fn mono_capture_duplicates_into_stereo_output() {
        let frames = 8;
        let mut transport = Transport::new(1, 2, frames, 4096);

        let mono: Vec<f32> = (0..frames).map(|i| 0.1 * i as f32).collect();
        transport.capture(&mono);
        let mut out = vec![0.0f32; frames * 2];
        transport.render(&mut out);

        for (i, &m) in mono.iter().enumerate() {
            assert_eq!(out[i * 2], m);
            assert_eq!(out[i * 2 + 1], m);
        }
    }

    #[test]
    fn render_under_run_is_silence() {
        let frames = 16;
        let mut transport = Transport::new(2, 2, frames, 4096);

        let mut out = vec![1.0f32; frames * 2];
        transport.render(&mut out); // ring empty

        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn capture_render_is_allocation_free() {
        let frames = 64;
        let mut transport = Transport::new(2, 2, frames, 4096);
        let input = vec![0.5f32; frames * 2];
        let mut out = vec![0.0f32; frames * 2];

        lindelion_test_allocator::assert_no_allocations("transport capture/render", || {
            transport.capture(&input);
            transport.render(&mut out);
        });
    }
}
