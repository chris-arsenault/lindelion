use std::sync::Arc;

use lindelion_dsp_utils::{ola, phase};
use realfft::num_complex::Complex64;
use realfft::{ComplexToReal, RealFftPlanner};

use crate::{
    PitchShiftRatios, PitchShiftSourceCache, ResampleProCache, ResampleProFrame,
    synthesis_support::{frame_at_position, spectral_envelope_formant_gain},
};

const MIN_STRETCH_RATIO: f64 = 0.125;
const MAX_STRETCH_RATIO: f64 = 8.0;
const IDENTITY_STRETCH_EPSILON: f64 = 1.0e-9;
const MAGNITUDE_FLOOR: f64 = 1.0e-12;
const TRANSIENT_PROTECTION_WINDOW_FFT_FRACTION: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResampleProStretchError {
    EmptyCache,
    InvalidFrameShape,
    OutputTooLong,
    InverseFft,
}

pub(crate) struct ResampleProStretchState {
    inverse: Arc<dyn ComplexToReal<f64>>,
    spectrum: Vec<Complex64>,
    time_domain: Vec<f64>,
    output_accum: Vec<f64>,
    normalization: Vec<f64>,
    magnitudes: Vec<f64>,
    analysis_phases: Vec<f64>,
    inst_freqs: Vec<f64>,
    peak_owner_by_bin: Vec<u16>,
    peak_phases: Vec<f64>,
    bin_phases: Vec<f64>,
}

impl ResampleProStretchState {
    pub(crate) fn new(
        cache: &PitchShiftSourceCache,
        output_capacity_samples: usize,
    ) -> Result<Self, ResampleProStretchError> {
        let pro = &cache.resample_pro;
        validate_cache(pro)?;

        let mut planner = RealFftPlanner::<f64>::new();
        let inverse = planner.plan_fft_inverse(pro.fft_size);
        let bin_count = pro.bin_count();

        Ok(Self {
            spectrum: inverse.make_input_vec(),
            time_domain: inverse.make_output_vec(),
            inverse,
            output_accum: vec![0.0; output_capacity_samples],
            normalization: vec![0.0; output_capacity_samples],
            magnitudes: vec![0.0; bin_count],
            analysis_phases: vec![0.0; bin_count],
            inst_freqs: vec![0.0; bin_count],
            peak_owner_by_bin: vec![0; bin_count],
            peak_phases: vec![0.0; bin_count],
            bin_phases: vec![0.0; bin_count],
        })
    }

    pub(crate) fn render_to(
        &mut self,
        cache: &PitchShiftSourceCache,
        stretch_ratio: f64,
        output: &mut [f32],
    ) -> Result<(), ResampleProStretchError> {
        self.render_to_with_ratios(cache, stretch_ratio, PitchShiftRatios::identity(), output)
    }

    pub(crate) fn render_to_with_ratios(
        &mut self,
        cache: &PitchShiftSourceCache,
        stretch_ratio: f64,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<(), ResampleProStretchError> {
        self.render_to_region_with_ratios(cache, 0, stretch_ratio, ratios, output)
    }

    pub(crate) fn render_to_region_with_ratios(
        &mut self,
        cache: &PitchShiftSourceCache,
        source_start_sample: usize,
        stretch_ratio: f64,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<(), ResampleProStretchError> {
        let pro = &cache.resample_pro;
        validate_cache(pro)?;
        self.validate_shape(pro, output.len())?;
        clear_work_buffers(
            &mut self.output_accum[..output.len()],
            &mut self.normalization[..output.len()],
            output,
        );

        let stretch_ratio = sanitize_stretch_ratio(stretch_ratio);
        if is_identity_stretch(stretch_ratio) {
            self.render_analysis_frames_to(pro, source_start_sample, output.len())?;
        } else {
            self.render_variable_frames_to(
                cache,
                source_start_sample,
                stretch_ratio,
                ratios,
                output.len(),
            )?;
        }

        normalize_to_output(
            &self.output_accum[..output.len()],
            &self.normalization[..output.len()],
            output,
        );
        Ok(())
    }

    fn validate_shape(
        &self,
        cache: &ResampleProCache,
        output_len: usize,
    ) -> Result<(), ResampleProStretchError> {
        let bin_count = cache.bin_count();
        if output_len > self.output_accum.len()
            || output_len > self.normalization.len()
            || self.spectrum.len() != bin_count
            || self.magnitudes.len() != bin_count
            || self.analysis_phases.len() != bin_count
            || self.inst_freqs.len() != bin_count
            || self.peak_owner_by_bin.len() != bin_count
            || self.peak_phases.len() != bin_count
            || self.bin_phases.len() != bin_count
            || self.time_domain.len() != cache.fft_size
        {
            return Err(ResampleProStretchError::OutputTooLong);
        }
        Ok(())
    }

    fn render_analysis_frames_to(
        &mut self,
        cache: &ResampleProCache,
        source_start_sample: usize,
        output_len: usize,
    ) -> Result<(), ResampleProStretchError> {
        for frame in &cache.frames {
            let output_start = frame.start_sample as isize - source_start_sample as isize;
            if !frame_overlaps_output(output_start, cache.fft_size, output_len) {
                continue;
            }
            frame_spectrum_into(frame, cache, &mut self.spectrum)?;
            inverse_fft_frame(&self.inverse, &mut self.spectrum, &mut self.time_domain)?;
            overlap_add_frame_at(
                output_start,
                cache,
                &self.time_domain,
                &mut self.output_accum[..output_len],
                &mut self.normalization[..output_len],
            );
        }
        Ok(())
    }

    fn render_variable_frames_to(
        &mut self,
        source_cache: &PitchShiftSourceCache,
        source_start_sample: usize,
        stretch_ratio: f64,
        ratios: PitchShiftRatios,
        output_len: usize,
    ) -> Result<(), ResampleProStretchError> {
        let cache = &source_cache.resample_pro;
        self.peak_phases.fill(0.0);
        self.bin_phases.fill(0.0);

        let hop = cache.synthesis_hop.max(1);
        let mut output_start = 0usize;
        let mut output_frame_index = 0usize;
        while output_start < output_len {
            let source_start = source_start_sample as f64
                + source_start_for_output_frame(output_start, stretch_ratio, cache);
            interpolate_analysis_frame(
                cache,
                source_start,
                &mut self.magnitudes,
                &mut self.analysis_phases,
                &mut self.inst_freqs,
                &mut self.peak_owner_by_bin,
            )?;
            apply_formant_compensation(source_cache, source_start, ratios, &mut self.magnitudes);
            let protected_transient = protected_transient_at(cache, source_start);
            self.propagate_phase_locked_frame(
                cache,
                output_frame_index == 0 || protected_transient.is_some(),
            );
            locked_spectrum_into(
                cache,
                &self.magnitudes,
                &self.bin_phases,
                &mut self.spectrum,
            )?;
            inverse_fft_frame(&self.inverse, &mut self.spectrum, &mut self.time_domain)?;
            let frame_output_start = protected_transient
                .map(|transient| {
                    transient_output_start(
                        transient,
                        source_start_sample,
                        source_start,
                        stretch_ratio,
                    )
                })
                .unwrap_or(output_start as isize);
            overlap_add_frame_at(
                frame_output_start,
                cache,
                &self.time_domain,
                &mut self.output_accum[..output_len],
                &mut self.normalization[..output_len],
            );

            output_start = output_start.saturating_add(hop);
            output_frame_index += 1;
        }
        Ok(())
    }

    fn propagate_phase_locked_frame(&mut self, cache: &ResampleProCache, reset_phase: bool) {
        if reset_phase {
            self.bin_phases.copy_from_slice(&self.analysis_phases);
            self.peak_phases.copy_from_slice(&self.analysis_phases);
            return;
        }

        let hop = cache.synthesis_hop.max(1) as f64;
        for bin in 0..self.peak_phases.len() {
            if peak_owner(&self.peak_owner_by_bin, bin) == bin {
                self.peak_phases[bin] =
                    phase::principal_angle(self.peak_phases[bin] + self.inst_freqs[bin] * hop);
            }
        }

        for bin in 0..self.bin_phases.len() {
            let owner = peak_owner(&self.peak_owner_by_bin, bin);
            if owner >= self.bin_phases.len() {
                self.bin_phases[bin] =
                    phase::principal_angle(self.bin_phases[bin] + self.inst_freqs[bin] * hop);
                continue;
            }
            let local_offset =
                phase::principal_angle(self.analysis_phases[bin] - self.analysis_phases[owner]);
            self.bin_phases[bin] = phase::principal_angle(self.peak_phases[owner] + local_offset);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProtectedTransient {
    source_sample: usize,
}

fn frame_overlaps_output(output_start: isize, frame_len: usize, output_len: usize) -> bool {
    output_start < output_len as isize && output_start.saturating_add(frame_len as isize) > 0
}

pub(crate) fn render_stretch(
    cache: &PitchShiftSourceCache,
    stretch_ratio: f64,
) -> Result<Vec<f32>, ResampleProStretchError> {
    let output_len = stretched_output_len(cache.source_len_samples, stretch_ratio);
    let mut output = vec![0.0; output_len];
    let mut state = ResampleProStretchState::new(cache, output_len)?;
    state.render_to(cache, stretch_ratio, &mut output)?;
    Ok(output)
}

pub(crate) fn stretched_output_len(source_len: usize, stretch_ratio: f64) -> usize {
    if source_len == 0 {
        return 0;
    }
    (source_len as f64 * sanitize_stretch_ratio(stretch_ratio))
        .ceil()
        .max(1.0) as usize
}

fn validate_cache(cache: &ResampleProCache) -> Result<(), ResampleProStretchError> {
    if cache.fft_size == 0 || cache.frames.is_empty() || cache.window.len() != cache.fft_size {
        return Err(ResampleProStretchError::EmptyCache);
    }
    let bin_count = cache.bin_count();
    if cache.frames.iter().any(|frame| {
        frame.magnitudes.len() != bin_count
            || frame.phases.len() != bin_count
            || frame.instantaneous_frequency_rad_per_sample.len() != bin_count
            || frame.peak_owner_by_bin.len() != bin_count
    }) {
        return Err(ResampleProStretchError::InvalidFrameShape);
    }
    Ok(())
}

fn clear_work_buffers(output_accum: &mut [f64], normalization: &mut [f64], output: &mut [f32]) {
    output_accum.fill(0.0);
    normalization.fill(0.0);
    output.fill(0.0);
}

pub(crate) fn sanitize_stretch_ratio(stretch_ratio: f64) -> f64 {
    if stretch_ratio.is_finite() {
        stretch_ratio.clamp(MIN_STRETCH_RATIO, MAX_STRETCH_RATIO)
    } else {
        1.0
    }
}

fn is_identity_stretch(stretch_ratio: f64) -> bool {
    (stretch_ratio - 1.0).abs() <= IDENTITY_STRETCH_EPSILON
}

fn source_start_for_output_frame(
    output_start: usize,
    stretch_ratio: f64,
    cache: &ResampleProCache,
) -> f64 {
    let half_fft = cache.fft_size as f64 * 0.5;
    let output_center = output_start as f64 + half_fft;
    (output_center / stretch_ratio - half_fft).max(0.0)
}

fn interpolate_analysis_frame(
    cache: &ResampleProCache,
    source_start: f64,
    magnitudes: &mut [f64],
    phases: &mut [f64],
    inst_freqs: &mut [f64],
    peak_owner_by_bin: &mut [u16],
) -> Result<(), ResampleProStretchError> {
    let bin_count = cache.bin_count();
    if magnitudes.len() != bin_count
        || phases.len() != bin_count
        || inst_freqs.len() != bin_count
        || peak_owner_by_bin.len() != bin_count
    {
        return Err(ResampleProStretchError::InvalidFrameShape);
    }

    let frame_coordinate = source_start / cache.analysis_hop.max(1) as f64;
    let left_index = frame_coordinate
        .floor()
        .clamp(0.0, cache.frames.len().saturating_sub(1) as f64) as usize;
    let right_index = (left_index + 1).min(cache.frames.len() - 1);
    let fraction = if left_index == right_index {
        0.0
    } else {
        (frame_coordinate - left_index as f64).clamp(0.0, 1.0)
    };
    let left = &cache.frames[left_index];
    let right = &cache.frames[right_index];

    for bin in 0..bin_count {
        magnitudes[bin] =
            interpolate_magnitude(left.magnitudes[bin], right.magnitudes[bin], fraction);
        phases[bin] = interpolate_phase(left.phases[bin], right.phases[bin], fraction);
        inst_freqs[bin] = left.instantaneous_frequency_rad_per_sample[bin]
            + (right.instantaneous_frequency_rad_per_sample[bin]
                - left.instantaneous_frequency_rad_per_sample[bin])
                * fraction;
        peak_owner_by_bin[bin] = if fraction < 0.5 {
            left.peak_owner_by_bin[bin]
        } else {
            right.peak_owner_by_bin[bin]
        };
    }
    Ok(())
}

fn interpolate_magnitude(left: f64, right: f64, fraction: f64) -> f64 {
    if left > MAGNITUDE_FLOOR && right > MAGNITUDE_FLOOR {
        (left.ln() + (right.ln() - left.ln()) * fraction).exp()
    } else {
        (left + (right - left) * fraction).max(0.0)
    }
}

fn interpolate_phase(left: f64, right: f64, fraction: f64) -> f64 {
    phase::principal_angle(left + phase::principal_angle(right - left) * fraction)
}

fn apply_formant_compensation(
    cache: &PitchShiftSourceCache,
    source_start: f64,
    ratios: PitchShiftRatios,
    magnitudes: &mut [f64],
) {
    let ratios = ratios.sanitized();
    if is_identity_formant_compensation(ratios) {
        return;
    }

    let pro = &cache.resample_pro;
    let source_center = (source_start + pro.fft_size as f64 * 0.5)
        .round()
        .clamp(0.0, cache.source_len_samples.saturating_sub(1) as f64)
        as usize;
    let envelope_frame = frame_at_position(&cache.frames, source_center);
    let bin_hz = pro.sample_rate as f64 / pro.fft_size as f64;
    for (bin, magnitude) in magnitudes.iter_mut().enumerate().skip(1) {
        let source_frequency_hz = bin as f64 * bin_hz;
        *magnitude *= spectral_envelope_formant_gain(
            &envelope_frame.spectral_envelope,
            source_frequency_hz,
            ratios,
        );
    }
}

fn is_identity_formant_compensation(ratios: PitchShiftRatios) -> bool {
    let ratios = ratios.sanitized();
    (ratios.pitch_ratio - 1.0).abs() <= f32::EPSILON
        && ratios
            .formant_ratio
            .is_none_or(|formant_ratio| (formant_ratio - 1.0).abs() <= f32::EPSILON)
}

fn protected_transient_at(
    cache: &ResampleProCache,
    source_start: f64,
) -> Option<ProtectedTransient> {
    if cache.transient_samples.is_empty() {
        return None;
    }
    let source_center = source_start + cache.fft_size as f64 * 0.5;
    let protection_radius = cache.fft_size as f64 * TRANSIENT_PROTECTION_WINDOW_FFT_FRACTION;
    cache
        .transient_samples
        .iter()
        .copied()
        .filter(|sample| (source_center - *sample as f64).abs() <= protection_radius)
        .min_by(|left, right| {
            (source_center - *left as f64)
                .abs()
                .total_cmp(&(source_center - *right as f64).abs())
        })
        .map(|source_sample| ProtectedTransient { source_sample })
}

fn transient_output_start(
    transient: ProtectedTransient,
    region_source_start_sample: usize,
    analysis_source_start: f64,
    stretch_ratio: f64,
) -> isize {
    let target_output_position =
        (transient.source_sample as f64 - region_source_start_sample as f64) * stretch_ratio;
    let transient_frame_offset = transient.source_sample as f64 - analysis_source_start;
    (target_output_position - transient_frame_offset).round() as isize
}

fn peak_owner(owners: &[u16], bin: usize) -> usize {
    owners.get(bin).copied().map(usize::from).unwrap_or(bin)
}

fn frame_spectrum_into(
    frame: &ResampleProFrame,
    cache: &ResampleProCache,
    spectrum: &mut [Complex64],
) -> Result<(), ResampleProStretchError> {
    locked_spectrum_into(cache, &frame.magnitudes, &frame.phases, spectrum)
}

fn locked_spectrum_into(
    cache: &ResampleProCache,
    magnitudes: &[f64],
    phases: &[f64],
    spectrum: &mut [Complex64],
) -> Result<(), ResampleProStretchError> {
    let bin_count = cache.bin_count();
    if magnitudes.len() != bin_count || phases.len() != bin_count || spectrum.len() != bin_count {
        return Err(ResampleProStretchError::InvalidFrameShape);
    }

    for ((bin, magnitude), phase) in spectrum
        .iter_mut()
        .zip(magnitudes.iter().copied())
        .zip(phases.iter().copied())
    {
        *bin = Complex64::new(magnitude * phase.cos(), magnitude * phase.sin());
    }
    if let Some(dc) = spectrum.first_mut() {
        dc.im = 0.0;
    }
    if let Some(nyquist) = spectrum
        .last_mut()
        .filter(|_| cache.fft_size.is_multiple_of(2))
    {
        nyquist.im = 0.0;
    }
    Ok(())
}

fn inverse_fft_frame(
    inverse: &Arc<dyn ComplexToReal<f64>>,
    spectrum: &mut [Complex64],
    time_domain: &mut [f64],
) -> Result<(), ResampleProStretchError> {
    inverse
        .process(spectrum, time_domain)
        .map_err(|_| ResampleProStretchError::InverseFft)
}

fn overlap_add_frame_at(
    output_start: isize,
    cache: &ResampleProCache,
    time_domain: &[f64],
    output: &mut [f64],
    normalization: &mut [f64],
) {
    let fft_scale = cache.fft_size as f64;
    for (index, sample) in time_domain.iter().copied().enumerate() {
        let Some(position) = output_start.checked_add(index as isize) else {
            continue;
        };
        if position < 0 {
            continue;
        }
        let position = position as usize;
        if position >= output.len() {
            break;
        }
        let window = cache.window[index];
        output[position] += sample / fft_scale * window;
    }
    ola::accumulate_squared_window_at(&cache.window, output_start, normalization);
}

fn normalize_to_output(output_accum: &[f64], normalization: &[f64], output: &mut [f32]) {
    for ((sample, value), weight) in output
        .iter_mut()
        .zip(output_accum.iter().copied())
        .zip(normalization.iter().copied())
    {
        *sample = if weight > f64::EPSILON {
            (value / weight) as f32
        } else {
            0.0
        };
    }
}
