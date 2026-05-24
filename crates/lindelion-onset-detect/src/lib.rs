use std::{fmt, sync::Arc};

use lindelion_dsp_utils::{
    analysis,
    math::{cents_between, finite_clamp, ms_to_samples},
};
use lindelion_pitch_detect::{PitchContour, PitchFrame, median_voiced_pitch};
use realfft::{RealFftPlanner, RealToComplex};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ONSET_SENSITIVITY: f32 = 0.5;
pub const DEFAULT_MIN_SLICE_MS: f32 = 50.0;
pub const DEFAULT_SUPERFLUX_LOOKBACK_FRAMES: u32 = 3;
pub const DEFAULT_SUPERFLUX_MAX_FILTER_RADIUS: u32 = 3;
pub const DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS: f32 = 120.0;
pub const DEFAULT_PITCH_STABILITY_DURATION_MS: f32 = 64.0;
pub const ENERGY_TRANSIENT_BASE_THRESHOLD: f32 = 0.02;
pub const ENERGY_TRANSIENT_SENSITIVITY_RANGE: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionAlgorithm {
    SuperFlux,
    ComplexFlux,
    SpectralSparsity,
    PitchStability,
    EnergyTransient,
    ManualGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerKind {
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceMarker {
    pub position_samples: usize,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AlgorithmParams {
    SuperFlux {
        lookback_frames: u32,
        max_filter_radius: u32,
    },
    ComplexFlux {
        lookback_frames: u32,
        group_delay_weight: f32,
    },
    SpectralSparsity {
        window_size: usize,
    },
    PitchStability {
        threshold_cents: f32,
        min_stable_duration_ms: f32,
    },
    EnergyTransient {
        frame_size: usize,
    },
    ManualGrid {
        divisions: usize,
        offset_ms: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OnsetDetectionProfile {
    pub lookback_frames: u32,
    pub max_filter_radius: u32,
    pub pitch_stability_threshold_cents: f32,
    pub pitch_stability_duration_ms: f32,
}

impl Default for OnsetDetectionProfile {
    fn default() -> Self {
        Self {
            lookback_frames: DEFAULT_SUPERFLUX_LOOKBACK_FRAMES,
            max_filter_radius: DEFAULT_SUPERFLUX_MAX_FILTER_RADIUS,
            pitch_stability_threshold_cents: DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
            pitch_stability_duration_ms: DEFAULT_PITCH_STABILITY_DURATION_MS,
        }
    }
}

impl OnsetDetectionProfile {
    pub fn relaxed() -> Self {
        Self {
            lookback_frames: 5,
            max_filter_radius: 4,
            pitch_stability_threshold_cents: 160.0,
            pitch_stability_duration_ms: 80.0,
        }
    }

    pub fn aggressive() -> Self {
        Self {
            lookback_frames: 2,
            max_filter_radius: 2,
            pitch_stability_threshold_cents: 80.0,
            pitch_stability_duration_ms: 48.0,
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            lookback_frames: self.lookback_frames.clamp(1, 32),
            max_filter_radius: self.max_filter_radius.min(32),
            pitch_stability_threshold_cents: finite_clamp(
                self.pitch_stability_threshold_cents,
                1.0,
                2_400.0,
                DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
            ),
            pitch_stability_duration_ms: finite_clamp(
                self.pitch_stability_duration_ms,
                1.0,
                5_000.0,
                DEFAULT_PITCH_STABILITY_DURATION_MS,
            ),
        }
    }

    pub fn superflux_params(self) -> AlgorithmParams {
        let profile = self.sanitized();
        AlgorithmParams::SuperFlux {
            lookback_frames: profile.lookback_frames,
            max_filter_radius: profile.max_filter_radius,
        }
    }

    pub fn pitch_stability_params(self) -> AlgorithmParams {
        let profile = self.sanitized();
        AlgorithmParams::PitchStability {
            threshold_cents: profile.pitch_stability_threshold_cents,
            min_stable_duration_ms: profile.pitch_stability_duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub algorithm: DetectionAlgorithm,
    pub sensitivity: f32,
    pub min_slice_ms: f32,
    #[serde(default)]
    pub profile: OnsetDetectionProfile,
    pub params: AlgorithmParams,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        let profile = OnsetDetectionProfile::default();
        Self {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity: DEFAULT_ONSET_SENSITIVITY,
            min_slice_ms: DEFAULT_MIN_SLICE_MS,
            profile,
            params: profile.superflux_params(),
        }
    }
}

impl DetectionConfig {
    pub fn superflux(sensitivity: f32, min_slice_ms: f32, profile: OnsetDetectionProfile) -> Self {
        let profile = profile.sanitized();
        Self {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity,
            min_slice_ms,
            profile,
            params: profile.superflux_params(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchTrack<'a> {
    pub source_sample_rate: u32,
    pub frame_hop_samples: usize,
    pub frames: &'a [PitchFrame],
}

impl<'a> PitchTrack<'a> {
    pub fn from_contour(contour: &'a PitchContour) -> Self {
        Self {
            source_sample_rate: contour.source_sample_rate,
            frame_hop_samples: contour.source_frame_hop_samples(),
            frames: &contour.frames,
        }
    }
}

impl<'a> From<&'a PitchContour> for PitchTrack<'a> {
    fn from(contour: &'a PitchContour) -> Self {
        Self::from_contour(contour)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnsetDetectionInput<'a> {
    pub audio: &'a [f32],
    pub sample_rate: u32,
    pub pitch_track: Option<PitchTrack<'a>>,
}

impl<'a> OnsetDetectionInput<'a> {
    pub const fn new(audio: &'a [f32], sample_rate: u32) -> Self {
        Self {
            audio,
            sample_rate,
            pitch_track: None,
        }
    }

    pub fn with_pitch_track(mut self, pitch_track: PitchTrack<'a>) -> Self {
        self.pitch_track = Some(pitch_track);
        self
    }

    pub fn with_pitch_contour(self, pitch_contour: &'a PitchContour) -> Self {
        self.with_pitch_track(PitchTrack::from_contour(pitch_contour))
    }
}

pub trait OnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker>;
}

pub trait StreamingOnsetDetector {
    fn next_block(&mut self, audio: &[f32]) -> &[SliceMarker];
    fn reset(&mut self);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ConfiguredOnsetDetector;

impl OnsetDetector for ConfiguredOnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        match config.algorithm {
            DetectionAlgorithm::SuperFlux
            | DetectionAlgorithm::ComplexFlux
            | DetectionAlgorithm::SpectralSparsity => SuperFluxDetector.detect(input, config),
            DetectionAlgorithm::EnergyTransient => EnergyTransientDetector.detect(input, config),
            DetectionAlgorithm::ManualGrid => ManualGridDetector.detect(input, config),
            DetectionAlgorithm::PitchStability => PitchStabilityDetector.detect(input, config),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HybridOnsetDetector;

impl OnsetDetector for HybridOnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let profile = onset_profile(config);
        let mut markers = SuperFluxDetector.detect(
            input,
            DetectionConfig {
                algorithm: DetectionAlgorithm::SuperFlux,
                profile,
                params: profile.superflux_params(),
                ..config
            },
        );

        let Some(pitch_track) = input.pitch_track else {
            return markers;
        };

        markers.extend(PitchStabilityDetector.detect(input.with_pitch_track(pitch_track), config));
        dedupe_markers(
            markers,
            ms_to_samples(config.min_slice_ms, input.sample_rate),
            input.audio.len(),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ManualGridDetector;

impl OnsetDetector for ManualGridDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let AlgorithmParams::ManualGrid {
            divisions,
            offset_ms,
        } = config.params
        else {
            return Vec::new();
        };

        let audio = input.audio;
        if divisions == 0 || audio.is_empty() {
            return Vec::new();
        }

        let offset_samples = ms_to_samples(offset_ms, input.sample_rate);
        let step = audio.len() / divisions.max(1);
        (0..divisions)
            .map(|index| SliceMarker {
                position_samples: (offset_samples + index * step).min(audio.len() - 1),
                kind: MarkerKind::Auto,
            })
            .collect()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EnergyTransientDetector;

impl OnsetDetector for EnergyTransientDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        if input.audio.is_empty() || input.sample_rate == 0 {
            return Vec::new();
        }

        let mut detector = StreamingEnergyTransientDetector::new(input.sample_rate, config);
        let mut markers = Vec::new();
        markers.extend_from_slice(detector.next_block(input.audio));
        markers.extend_from_slice(detector.finish());
        markers
    }
}

#[derive(Debug, Clone)]
pub struct StreamingEnergyTransientDetector {
    frame_size: usize,
    min_gap_samples: usize,
    threshold: f32,
    pending: Vec<f32>,
    next_frame_start: usize,
    total_samples_seen: usize,
    last_energy: f32,
    last_marker: usize,
    emitted_initial_marker: bool,
    block_markers: Vec<SliceMarker>,
}

impl StreamingEnergyTransientDetector {
    pub fn new(sample_rate: u32, config: DetectionConfig) -> Self {
        let frame_size = match config.params {
            AlgorithmParams::EnergyTransient { frame_size } => frame_size.max(32),
            _ => 512,
        };
        let min_gap_samples = ms_to_samples(config.min_slice_ms, sample_rate);
        let threshold = ENERGY_TRANSIENT_BASE_THRESHOLD
            + (1.0 - config.sensitivity.clamp(0.0, 1.0)) * ENERGY_TRANSIENT_SENSITIVITY_RANGE;
        Self {
            frame_size,
            min_gap_samples,
            threshold,
            pending: Vec::new(),
            next_frame_start: 0,
            total_samples_seen: 0,
            last_energy: 0.0,
            last_marker: 0,
            emitted_initial_marker: false,
            block_markers: Vec::new(),
        }
    }

    pub fn with_realtime_capacity(
        sample_rate: u32,
        config: DetectionConfig,
        max_block_size: usize,
    ) -> Self {
        let mut detector = Self::new(sample_rate, config);
        detector.reserve_realtime_capacity(max_block_size);
        detector
    }

    pub fn reserve_realtime_capacity(&mut self, max_block_size: usize) {
        let pending_capacity = self.frame_size.saturating_add(max_block_size);
        if self.pending.capacity() < pending_capacity {
            self.pending
                .reserve_exact(pending_capacity - self.pending.capacity());
        }

        let marker_capacity = pending_capacity
            .div_ceil(self.frame_size.max(1))
            .saturating_add(1);
        if self.block_markers.capacity() < marker_capacity {
            self.block_markers
                .reserve_exact(marker_capacity - self.block_markers.capacity());
        }
    }

    pub fn finish(&mut self) -> &[SliceMarker] {
        self.block_markers.clear();
        self.process_available_frames(true);
        &self.block_markers
    }

    fn process_available_frames(&mut self, flush: bool) {
        while self.pending.len() >= self.frame_size || (flush && !self.pending.is_empty()) {
            let frame_len = self.pending.len().min(self.frame_size);
            let energy = analysis::rms(&self.pending[..frame_len]).powi(2);
            let delta = energy - self.last_energy;
            let position = self.next_frame_start;

            if delta > self.threshold
                && position.saturating_sub(self.last_marker) >= self.min_gap_samples
            {
                self.block_markers.push(SliceMarker {
                    position_samples: position,
                    kind: MarkerKind::Auto,
                });
                self.last_marker = position;
            }

            self.last_energy = energy;
            self.pending.drain(0..frame_len);
            self.next_frame_start += self.frame_size;
        }
    }
}

impl StreamingOnsetDetector for StreamingEnergyTransientDetector {
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
        analysis::append_sanitized_audio(&mut self.pending, audio);
        self.total_samples_seen += audio.len();
        self.process_available_frames(false);
        &self.block_markers
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.block_markers.clear();
        self.next_frame_start = 0;
        self.total_samples_seen = 0;
        self.last_energy = 0.0;
        self.last_marker = 0;
        self.emitted_initial_marker = false;
    }
}

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
        let positions = pick_flux_peaks(
            &flux,
            hop_size,
            config.sensitivity,
            min_gap,
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
            min_gap,
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

#[derive(Debug, Default, Clone, Copy)]
pub struct PitchStabilityDetector;

impl OnsetDetector for PitchStabilityDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let Some(pitch_track) = input.pitch_track else {
            return Vec::new();
        };
        let profile = onset_profile(config);
        pitch_stability_markers_from_track(
            pitch_track,
            profile.pitch_stability_threshold_cents,
            profile.pitch_stability_duration_ms,
            config.min_slice_ms,
        )
    }
}

pub fn pitch_stability_markers_from_track(
    pitch_track: PitchTrack<'_>,
    threshold_cents: f32,
    min_stable_duration_ms: f32,
    min_note_ms: f32,
) -> Vec<SliceMarker> {
    if pitch_track.frames.is_empty() {
        return Vec::new();
    }

    let frame_ms = pitch_track.frame_hop_samples as f32
        / pitch_track.source_sample_rate.max(1) as f32
        * 1000.0;
    let stable_frames = (min_stable_duration_ms.max(frame_ms) / frame_ms).round() as usize;
    let min_gap = ms_to_samples(min_note_ms, pitch_track.source_sample_rate);
    let mut markers = vec![SliceMarker {
        position_samples: 0,
        kind: MarkerKind::Auto,
    }];
    let threshold_cents = threshold_cents.max(1.0);

    for index in stable_frames..pitch_track.frames.len().saturating_sub(stable_frames) {
        let left = median_voiced_pitch(&pitch_track.frames[index - stable_frames..index]);
        let right = median_voiced_pitch(&pitch_track.frames[index..index + stable_frames]);
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        let cents = cents_between(left, right);
        let position = pitch_track.frames[index].source_sample_position;
        let far_enough = markers
            .last()
            .map(|last| position.saturating_sub(last.position_samples) >= min_gap)
            .unwrap_or(true);
        if cents >= threshold_cents && far_enough {
            markers.push(SliceMarker {
                position_samples: position,
                kind: MarkerKind::Auto,
            });
        }
    }

    markers
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

fn dedupe_markers(
    mut markers: Vec<SliceMarker>,
    min_gap_samples: usize,
    audio_len: usize,
) -> Vec<SliceMarker> {
    if audio_len == 0 {
        return Vec::new();
    }

    markers.sort_by_key(|marker| marker.position_samples);
    let mut deduped: Vec<SliceMarker> = Vec::new();
    for mut marker in markers {
        marker.position_samples = marker.position_samples.min(audio_len - 1);
        let far_enough = deduped
            .last()
            .map(|last| {
                marker
                    .position_samples
                    .saturating_sub(last.position_samples)
                    >= min_gap_samples
            })
            .unwrap_or(true);
        if far_enough {
            deduped.push(marker);
        }
    }
    if deduped.first().map(|marker| marker.position_samples) != Some(0) {
        deduped.insert(
            0,
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
        );
    }
    deduped
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

fn onset_profile(config: DetectionConfig) -> OnsetDetectionProfile {
    let mut profile = config.profile.sanitized();
    if config.profile == OnsetDetectionProfile::default() {
        match config.params {
            AlgorithmParams::SuperFlux {
                lookback_frames,
                max_filter_radius,
            } => {
                profile.lookback_frames = lookback_frames;
                profile.max_filter_radius = max_filter_radius;
            }
            AlgorithmParams::PitchStability {
                threshold_cents,
                min_stable_duration_ms,
            } => {
                profile.pitch_stability_threshold_cents = threshold_cents;
                profile.pitch_stability_duration_ms = min_stable_duration_ms;
            }
            _ => {}
        }
    }
    profile.sanitized()
}

fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = 2.0 * std::f32::consts::PI * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}

#[cfg(test)]
mod tests;
