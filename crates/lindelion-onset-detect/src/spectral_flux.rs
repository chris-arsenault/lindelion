use std::{fmt, sync::Arc};

use lindelion_dsp_utils::{
    analysis,
    math::{finite_clamp, ms_to_samples},
};
use realfft::{RealFftPlanner, RealToComplex};

use crate::{
    AlgorithmParams, DetectionConfig, MarkerKind, OnsetDetectionInput, OnsetDetector, SliceMarker,
    StreamingOnsetDetector, dedupe_markers, onset_profile,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct SuperFluxDetector;

impl OnsetDetector for SuperFluxDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        if input.audio.is_empty() || input.sample_rate == 0 {
            return Vec::new();
        }

        let profile = onset_profile(config);
        let lookback_frames = profile.lookback_frames as usize;
        let max_filter_radius = profile.max_filter_radius as usize;
        let frame_size = 1024usize;
        let hop_size = 256usize;
        let min_gap = ms_to_samples(config.min_slice_ms, input.sample_rate);
        let flux = spectral_flux(input.audio, frame_size, hop_size, max_filter_radius);
        markers_from_novelty(
            &flux,
            hop_size,
            config.sensitivity,
            min_gap,
            lookback_frames,
            input.audio.len(),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ComplexFluxDetector;

impl OnsetDetector for ComplexFluxDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        if input.audio.is_empty() || input.sample_rate == 0 {
            return Vec::new();
        }

        let profile = onset_profile(config);
        let (lookback_frames, group_delay_weight) = match config.params {
            AlgorithmParams::ComplexFlux {
                lookback_frames,
                group_delay_weight,
            } => (
                lookback_frames.clamp(1, 32) as usize,
                finite_clamp(group_delay_weight, 0.0, 8.0, 1.0),
            ),
            _ => (profile.lookback_frames as usize, 1.0),
        };
        let frame_size = 1024usize;
        let hop_size = 256usize;
        let min_gap = ms_to_samples(config.min_slice_ms, input.sample_rate);
        let flux = complex_flux(input.audio, frame_size, hop_size, group_delay_weight);
        markers_from_novelty(
            &flux,
            hop_size,
            config.sensitivity,
            min_gap,
            lookback_frames,
            input.audio.len(),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpectralSparsityDetector;

impl OnsetDetector for SpectralSparsityDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        if input.audio.is_empty() || input.sample_rate == 0 {
            return Vec::new();
        }

        let profile = onset_profile(config);
        let window_size = match config.params {
            AlgorithmParams::SpectralSparsity { window_size } => window_size.clamp(64, 8192),
            _ => 1024,
        };
        let hop_size = (window_size / 4).max(1);
        let min_gap = ms_to_samples(config.min_slice_ms, input.sample_rate);
        let flux = spectral_sparsity_novelty(input.audio, window_size, hop_size);
        markers_from_novelty(
            &flux,
            hop_size,
            config.sensitivity,
            min_gap,
            profile.lookback_frames as usize,
            input.audio.len(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamingFluxFrame {
    pub frame_index: usize,
    pub position_samples: usize,
    pub flux: f32,
}

#[derive(Clone)]
pub struct StreamingSpectralFlux {
    frame_size: usize,
    hop_size: usize,
    max_filter_radius: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    previous: Vec<f32>,
    pending: Vec<f32>,
    next_frame_start: usize,
    frame_index: usize,
    total_samples_seen: usize,
    block_flux: Vec<StreamingFluxFrame>,
}

impl fmt::Debug for StreamingSpectralFlux {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamingSpectralFlux")
            .field("frame_size", &self.frame_size)
            .field("hop_size", &self.hop_size)
            .field("max_filter_radius", &self.max_filter_radius)
            .field("pending_len", &self.pending.len())
            .field("next_frame_start", &self.next_frame_start)
            .field("frame_index", &self.frame_index)
            .field("total_samples_seen", &self.total_samples_seen)
            .finish()
    }
}

impl StreamingSpectralFlux {
    pub fn new(frame_size: usize, hop_size: usize, max_filter_radius: usize) -> Self {
        let frame_size = frame_size.max(2);
        let hop_size = hop_size.max(1);
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(frame_size);
        Self {
            frame_size,
            hop_size,
            max_filter_radius,
            fft,
            previous: vec![0.0; frame_size / 2 + 1],
            pending: Vec::new(),
            next_frame_start: 0,
            frame_index: 0,
            total_samples_seen: 0,
            block_flux: Vec::new(),
        }
    }

    pub const fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub const fn hop_size(&self) -> usize {
        self.hop_size
    }

    pub fn next_block(&mut self, audio: &[f32]) -> &[StreamingFluxFrame] {
        self.block_flux.clear();
        analysis::append_sanitized_audio(&mut self.pending, audio);
        self.total_samples_seen += audio.len();
        self.process_available_frames(false);
        &self.block_flux
    }

    pub fn finish(&mut self) -> &[StreamingFluxFrame] {
        self.block_flux.clear();
        self.process_available_frames(true);
        &self.block_flux
    }

    pub fn reset(&mut self) {
        let frame_size = self.frame_size;
        let hop_size = self.hop_size;
        let max_filter_radius = self.max_filter_radius;
        *self = Self::new(frame_size, hop_size, max_filter_radius);
    }

    fn process_available_frames(&mut self, flush: bool) {
        while self.pending.len() >= self.frame_size
            || (flush
                && !self.pending.is_empty()
                && self.next_frame_start < self.total_samples_seen)
        {
            let Some(frame) = self.process_next_frame() else {
                break;
            };
            self.block_flux.push(frame);
        }
    }

    fn process_next_frame(&mut self) -> Option<StreamingFluxFrame> {
        let mut frame = self.fft.make_input_vec();
        for (index, sample) in frame.iter_mut().enumerate() {
            let source = self.pending.get(index).copied().unwrap_or(0.0);
            *sample = source * hann(index, self.frame_size);
        }

        let mut spectrum = self.fft.make_output_vec();
        if self.fft.process(&mut frame, &mut spectrum).is_err() {
            return None;
        }

        let magnitudes = spectrum
            .iter()
            .map(|bin| bin.norm_sqr().sqrt())
            .collect::<Vec<_>>();
        let mut flux = 0.0;
        for (bin, magnitude) in magnitudes.iter().copied().enumerate() {
            let previous_max = local_max(&self.previous, bin, self.max_filter_radius);
            flux += (magnitude - previous_max).max(0.0);
        }
        self.previous = magnitudes;

        let frame = StreamingFluxFrame {
            frame_index: self.frame_index,
            position_samples: self.next_frame_start,
            flux,
        };
        let drain_len = self.hop_size.min(self.pending.len());
        self.pending.drain(0..drain_len);
        self.next_frame_start += self.hop_size;
        self.frame_index += 1;
        Some(frame)
    }
}

#[derive(Debug, Clone)]
pub struct StreamingSuperFluxDetector {
    flux: StreamingSpectralFlux,
    sensitivity: f32,
    min_gap_samples: usize,
    lookback_frames: usize,
    history: Vec<f32>,
    last_peak: usize,
    emitted_initial_marker: bool,
    block_markers: Vec<SliceMarker>,
}

impl StreamingSuperFluxDetector {
    pub fn new(sample_rate: u32, config: DetectionConfig) -> Self {
        let profile = onset_profile(config);
        Self {
            flux: StreamingSpectralFlux::new(1024, 256, profile.max_filter_radius as usize),
            sensitivity: config.sensitivity,
            min_gap_samples: ms_to_samples(config.min_slice_ms, sample_rate),
            lookback_frames: profile.lookback_frames as usize,
            history: Vec::new(),
            last_peak: 0,
            emitted_initial_marker: false,
            block_markers: Vec::new(),
        }
    }

    pub fn finish(&mut self) -> &[SliceMarker] {
        self.block_markers.clear();
        let frames = self.flux.finish().to_vec();
        self.observe_flux_frames(&frames);
        &self.block_markers
    }

    fn observe_flux_frames(&mut self, frames: &[StreamingFluxFrame]) {
        for frame in frames {
            self.history.push(frame.flux);
            self.maybe_emit_peak_with_lookahead();
        }
    }

    fn maybe_emit_peak_with_lookahead(&mut self) {
        if self.history.len() < 3 {
            return;
        }

        let candidate = self.history.len() - 2;
        let threshold = flux_peak_threshold(&self.history[..=candidate], self.sensitivity);
        let lookback_frames = self.lookback_frames.max(1);
        let lookback_start = candidate.saturating_sub(lookback_frames);
        let lookback_peak = self.history[lookback_start..candidate]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        let position = candidate * self.flux.hop_size();
        if self.history[candidate] >= threshold
            && self.history[candidate] >= lookback_peak
            && self.history[candidate] >= self.history[candidate + 1]
            && position.saturating_sub(self.last_peak) >= self.min_gap_samples
        {
            self.block_markers.push(SliceMarker {
                position_samples: position,
                kind: MarkerKind::Auto,
            });
            self.last_peak = position;
        }
    }
}

impl StreamingOnsetDetector for StreamingSuperFluxDetector {
    fn next_block(&mut self, audio: &[f32]) -> &[SliceMarker] {
        self.block_markers.clear();
        if audio.is_empty() {
            return &self.block_markers;
        }

        if !self.emitted_initial_marker {
            self.block_markers.push(SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            });
            self.emitted_initial_marker = true;
        }
        let frames = self.flux.next_block(audio).to_vec();
        self.observe_flux_frames(&frames);
        &self.block_markers
    }

    fn reset(&mut self) {
        self.flux.reset();
        self.history.clear();
        self.last_peak = 0;
        self.emitted_initial_marker = false;
        self.block_markers.clear();
    }
}

fn spectral_flux(
    audio: &[f32],
    frame_size: usize,
    hop_size: usize,
    max_filter_radius: usize,
) -> Vec<f32> {
    let mut stream = StreamingSpectralFlux::new(frame_size, hop_size, max_filter_radius);
    let mut flux = Vec::new();
    flux.extend(stream.next_block(audio).iter().map(|frame| frame.flux));
    flux.extend(stream.finish().iter().map(|frame| frame.flux));
    normalize_flux(&mut flux);
    flux
}

fn markers_from_novelty(
    novelty: &[f32],
    hop_size: usize,
    sensitivity: f32,
    min_gap_samples: usize,
    lookback_frames: usize,
    audio_len: usize,
) -> Vec<SliceMarker> {
    let positions = pick_flux_peaks(
        novelty,
        hop_size,
        sensitivity,
        min_gap_samples,
        lookback_frames,
    );
    dedupe_markers(
        positions
            .into_iter()
            .map(|position_samples| SliceMarker {
                position_samples,
                kind: MarkerKind::Auto,
            })
            .collect(),
        min_gap_samples,
        audio_len,
    )
}

fn complex_flux(
    audio: &[f32],
    frame_size: usize,
    hop_size: usize,
    group_delay_weight: f32,
) -> Vec<f32> {
    let frame_size = frame_size.max(2);
    let hop_size = hop_size.max(1);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let bin_count = frame_size / 2 + 1;
    let mut previous_magnitudes = vec![0.0; bin_count];
    let mut previous_phases = vec![0.0; bin_count];
    let mut previous_previous_phases = vec![0.0; bin_count];
    let mut novelty = Vec::new();

    for (frame_index, frame_start) in frame_starts(audio.len(), frame_size, hop_size).enumerate() {
        let spectrum = spectrum_at(audio, frame_start, frame_size, &*fft);
        let mut frame_flux = 0.0;

        for (bin, value) in spectrum.iter().enumerate() {
            let magnitude = value.norm_sqr().sqrt();
            let phase = value.im.atan2(value.re);
            if frame_index > 0 {
                let predicted_phase = 2.0 * previous_phases[bin] - previous_previous_phases[bin];
                let phase_error = wrap_phase(phase - predicted_phase);
                let phase_distance =
                    (magnitude.mul_add(
                        magnitude,
                        previous_magnitudes[bin] * previous_magnitudes[bin],
                    ) - 2.0 * magnitude * previous_magnitudes[bin] * phase_error.cos())
                    .max(0.0)
                    .sqrt();
                frame_flux += (magnitude - previous_magnitudes[bin]).max(0.0)
                    + group_delay_weight * phase_distance;
            }
            previous_previous_phases[bin] = previous_phases[bin];
            previous_phases[bin] = phase;
            previous_magnitudes[bin] = magnitude;
        }

        novelty.push(frame_flux);
    }

    normalize_flux(&mut novelty);
    novelty
}

fn spectral_sparsity_novelty(audio: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    let frame_size = frame_size.max(2);
    let hop_size = hop_size.max(1);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let mut previous_sparsity = 0.0;
    let mut novelty = Vec::new();

    for (frame_index, frame_start) in frame_starts(audio.len(), frame_size, hop_size).enumerate() {
        let spectrum = spectrum_at(audio, frame_start, frame_size, &*fft);
        let magnitudes = spectrum
            .iter()
            .map(|bin| bin.norm_sqr().sqrt())
            .collect::<Vec<_>>();
        let sparsity = hoyer_sparsity(&magnitudes);
        novelty.push(if frame_index == 0 {
            0.0
        } else {
            (sparsity - previous_sparsity).abs()
        });
        previous_sparsity = sparsity;
    }

    normalize_flux(&mut novelty);
    novelty
}

fn spectrum_at(
    audio: &[f32],
    frame_start: usize,
    frame_size: usize,
    fft: &dyn RealToComplex<f32>,
) -> Vec<realfft::num_complex::Complex32> {
    let mut frame = fft.make_input_vec();
    for (index, sample) in frame.iter_mut().enumerate() {
        let source = audio.get(frame_start + index).copied().unwrap_or(0.0);
        *sample = if source.is_finite() { source } else { 0.0 } * hann(index, frame_size);
    }

    let mut spectrum = fft.make_output_vec();
    if fft.process(&mut frame, &mut spectrum).is_err() {
        return Vec::new();
    }
    spectrum
}

fn frame_starts(
    audio_len: usize,
    frame_size: usize,
    hop_size: usize,
) -> impl Iterator<Item = usize> {
    let frame_size = frame_size.max(1);
    let hop_size = hop_size.max(1);
    let mut next = 0usize;
    std::iter::from_fn(move || {
        if audio_len == 0 || next >= audio_len {
            return None;
        }
        let current = next;
        next = next.saturating_add(hop_size);
        if current == 0 || current.saturating_add(frame_size / 2) < audio_len + frame_size {
            Some(current)
        } else {
            None
        }
    })
}

fn hoyer_sparsity(magnitudes: &[f32]) -> f32 {
    if magnitudes.len() < 2 {
        return 0.0;
    }
    let l1 = magnitudes.iter().copied().sum::<f32>();
    let l2 = magnitudes
        .iter()
        .copied()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if l2 <= f32::EPSILON {
        return 0.0;
    }
    let n = magnitudes.len() as f32;
    ((n.sqrt() - l1 / l2) / (n.sqrt() - 1.0)).clamp(0.0, 1.0)
}

fn wrap_phase(phase: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    (phase + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI
}

fn pick_flux_peaks(
    flux: &[f32],
    hop_size: usize,
    sensitivity: f32,
    min_gap_samples: usize,
    lookback_frames: usize,
) -> Vec<usize> {
    if flux.is_empty() {
        return Vec::new();
    }

    let threshold = flux_peak_threshold(flux, sensitivity);
    let mut peaks = vec![0];
    let mut last_peak = 0usize;

    let lookback_frames = lookback_frames.max(1);
    for index in 1..flux.len().saturating_sub(1) {
        let position = index * hop_size;
        let lookback_start = index.saturating_sub(lookback_frames);
        let lookback_peak = flux[lookback_start..index]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        if flux[index] >= threshold
            && flux[index] >= lookback_peak
            && flux[index] >= flux[index + 1]
            && position.saturating_sub(last_peak) >= min_gap_samples
        {
            peaks.push(position);
            last_peak = position;
        }
    }

    peaks
}

fn flux_peak_threshold(flux: &[f32], sensitivity: f32) -> f32 {
    if flux.is_empty() {
        return 0.05;
    }

    let mean = flux.iter().copied().sum::<f32>() / flux.len() as f32;
    let variance = flux
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f32>()
        / flux.len() as f32;
    let std_dev = variance.sqrt();
    (mean + std_dev * (1.5 - sensitivity.clamp(0.0, 1.0))).max(0.05)
}

fn normalize_flux(flux: &mut [f32]) {
    let max = flux.iter().copied().fold(0.0, f32::max);
    if max <= f32::EPSILON {
        return;
    }
    for value in flux {
        *value = (*value / max).clamp(0.0, 1.0);
    }
}

fn local_max(values: &[f32], center: usize, radius: usize) -> f32 {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(values.len());
    values[start..end].iter().copied().fold(0.0, f32::max)
}

fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = 2.0 * std::f32::consts::PI * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}
