use lindelion_dsp_utils::{analysis, math::ms_to_samples};

use crate::{
    AlgorithmParams, DetectionConfig, ENERGY_TRANSIENT_BASE_THRESHOLD,
    ENERGY_TRANSIENT_SENSITIVITY_RANGE, MarkerKind, OnsetDetectionInput, OnsetDetector,
    SliceMarker, StreamingOnsetDetector,
};

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
