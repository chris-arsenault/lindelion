use lindelion_dsp_utils::resampling::WindowedSincResampler;

use crate::{
    PitchShiftRatios, PitchShiftSourceCache, resample_pro_stretch,
    synthesis_support::raised_cosine_window,
};

const TRANSIENT_LAYER_WINDOW_FFT_FRACTION: f32 = 0.125;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleProRenderError {
    EmptyCache,
    OutputLength,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResampleProRenderRegion {
    pub start_sample: usize,
    pub end_sample: usize,
    pub guarded_start_sample: usize,
    pub guarded_end_sample: usize,
}

impl ResampleProRenderRegion {
    pub fn new(
        source_len_samples: usize,
        start_sample: usize,
        end_sample: usize,
        guard: usize,
    ) -> Self {
        let start_sample = start_sample.min(source_len_samples);
        let end_sample = end_sample.min(source_len_samples).max(start_sample);
        Self {
            start_sample,
            end_sample,
            guarded_start_sample: start_sample.saturating_sub(guard),
            guarded_end_sample: end_sample.saturating_add(guard).min(source_len_samples),
        }
    }

    pub fn output_len(self) -> usize {
        self.end_sample.saturating_sub(self.start_sample)
    }

    fn guarded_len(self) -> usize {
        self.guarded_end_sample
            .saturating_sub(self.guarded_start_sample)
    }

    fn trim_start(self) -> usize {
        self.start_sample.saturating_sub(self.guarded_start_sample)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResampleProSourceRegionRequest<'a> {
    pub source: &'a [f32],
    pub cache: &'a PitchShiftSourceCache,
    pub start_sample: usize,
    pub end_sample: usize,
    pub ratios: PitchShiftRatios,
}

pub struct ResampleProRenderState {
    stretch: resample_pro_stretch::ResampleProStretchState,
    stretched: Vec<f32>,
    guarded: Vec<f32>,
    resampler: WindowedSincResampler,
}

impl ResampleProRenderState {
    pub fn new(
        cache: &PitchShiftSourceCache,
        max_pitch_ratio: f64,
    ) -> Result<Self, ResampleProRenderError> {
        if cache.source_len_samples == 0 {
            return Err(ResampleProRenderError::EmptyCache);
        }
        let max_pitch_ratio = resample_pro_stretch::sanitize_stretch_ratio(max_pitch_ratio);
        let stretch_capacity =
            resample_pro_stretch::stretched_output_len(cache.source_len_samples, max_pitch_ratio);
        Ok(Self {
            stretch: resample_pro_stretch::ResampleProStretchState::new(cache, stretch_capacity)
                .map_err(|_| ResampleProRenderError::Stretch)?,
            stretched: vec![0.0; stretch_capacity],
            guarded: vec![0.0; cache.source_len_samples],
            resampler: WindowedSincResampler::pitch_shift(),
        })
    }

    pub fn render_pitch_shift_to(
        &mut self,
        cache: &PitchShiftSourceCache,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<(), ResampleProRenderError> {
        if cache.source_len_samples == 0 {
            return Err(ResampleProRenderError::EmptyCache);
        }
        if output.len() != cache.source_len_samples {
            return Err(ResampleProRenderError::OutputLength);
        }

        let ratios = ratios.sanitized();
        self.render_pitch_shift_region_to(cache, 0, ratios, output)
    }

    pub fn render_region_pitch_shift_to(
        &mut self,
        cache: &PitchShiftSourceCache,
        start_sample: usize,
        end_sample: usize,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<ResampleProRenderRegion, ResampleProRenderError> {
        if cache.source_len_samples == 0 {
            return Err(ResampleProRenderError::EmptyCache);
        }
        let region = guarded_region(cache, start_sample, end_sample);
        if output.len() != region.output_len() {
            return Err(ResampleProRenderError::OutputLength);
        }
        if region.output_len() == 0 {
            return Ok(region);
        }

        let guarded_len = region.guarded_len();
        let mut guarded = std::mem::take(&mut self.guarded);
        if guarded.len() < guarded_len {
            self.guarded = guarded;
            return Err(ResampleProRenderError::OutputLength);
        }
        let render_result = self.render_pitch_shift_region_to(
            cache,
            region.guarded_start_sample,
            ratios,
            &mut guarded[..guarded_len],
        );
        if render_result.is_ok() {
            let trim_start = region.trim_start();
            output.copy_from_slice(&guarded[trim_start..trim_start + output.len()]);
        }
        self.guarded = guarded;
        render_result?;
        Ok(region)
    }

    pub fn render_source_region_pitch_shift_to(
        &mut self,
        request: ResampleProSourceRegionRequest<'_>,
        output: &mut [f32],
    ) -> Result<ResampleProRenderRegion, ResampleProRenderError> {
        let region = self.render_region_pitch_shift_to(
            request.cache,
            request.start_sample,
            request.end_sample,
            request.ratios,
            output,
        )?;
        apply_direct_transient_layer(request.source, request.cache, region, output);
        Ok(region)
    }

    fn render_pitch_shift_region_to(
        &mut self,
        cache: &PitchShiftSourceCache,
        source_start_sample: usize,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<(), ResampleProRenderError> {
        let ratios = ratios.sanitized();
        let pitch_ratio = resample_pro_stretch::sanitize_stretch_ratio(ratios.pitch_ratio as f64);
        let stretched_len = resample_pro_stretch::stretched_output_len(output.len(), pitch_ratio);
        let stretched = self
            .stretched
            .get_mut(..stretched_len)
            .ok_or(ResampleProRenderError::OutputLength)?;
        self.stretch
            .render_to_region_with_ratios(
                cache,
                source_start_sample,
                pitch_ratio,
                ratios,
                stretched,
            )
            .map_err(|_| ResampleProRenderError::Stretch)?;
        self.resampler.render_to(stretched, pitch_ratio, output);
        Ok(())
    }
}

pub fn guarded_region(
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
) -> ResampleProRenderRegion {
    ResampleProRenderRegion::new(
        cache.source_len_samples,
        start_sample,
        end_sample,
        cache.resample_pro.fft_size,
    )
}

pub(crate) fn render_pitch_shift(
    cache: &PitchShiftSourceCache,
    ratios: PitchShiftRatios,
) -> Result<Vec<f32>, ResampleProRenderError> {
    let ratios = ratios.sanitized();
    let pitch_ratio = resample_pro_stretch::sanitize_stretch_ratio(ratios.pitch_ratio as f64);
    let mut output = vec![0.0; cache.source_len_samples];
    let mut state = ResampleProRenderState::new(cache, pitch_ratio)?;
    state.render_pitch_shift_to(cache, ratios, &mut output)?;
    Ok(output)
}

pub(crate) fn render_region_pitch_shift_with_source(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
    ratios: PitchShiftRatios,
) -> Result<Vec<f32>, ResampleProRenderError> {
    let region = guarded_region(cache, start_sample, end_sample);
    let mut output = vec![0.0; region.output_len()];
    let mut state = ResampleProRenderState::new(cache, ratios.pitch_ratio as f64)?;
    state.render_source_region_pitch_shift_to(
        ResampleProSourceRegionRequest {
            source,
            cache,
            start_sample,
            end_sample,
            ratios,
        },
        &mut output,
    )?;
    Ok(output)
}

fn apply_direct_transient_layer(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    region: ResampleProRenderRegion,
    output: &mut [f32],
) {
    if source.is_empty() || output.is_empty() || cache.resample_pro.transient_samples.is_empty() {
        return;
    }
    let radius = (cache.resample_pro.fft_size as f32 * TRANSIENT_LAYER_WINDOW_FFT_FRACTION)
        .round()
        .max(8.0) as usize;
    for transient_sample in cache
        .resample_pro
        .transient_samples
        .iter()
        .copied()
        .filter(|sample| *sample >= region.start_sample && *sample < region.end_sample)
    {
        apply_direct_transient(source, region, output, transient_sample, radius);
    }
}

fn apply_direct_transient(
    source: &[f32],
    region: ResampleProRenderRegion,
    output: &mut [f32],
    transient_sample: usize,
    radius: usize,
) {
    let center = transient_sample.saturating_sub(region.start_sample);
    let start = center.saturating_sub(radius);
    let end = center
        .saturating_add(radius)
        .saturating_add(1)
        .min(output.len());
    for (index, sample) in output.iter_mut().enumerate().take(end).skip(start) {
        let source_index = region.start_sample.saturating_add(index);
        let Some(source_sample) = source.get(source_index).copied() else {
            continue;
        };
        let distance = index as f32 - center as f32;
        let blend = if index <= center {
            1.0
        } else {
            raised_cosine_window(distance / radius.max(1) as f32)
        };
        *sample = *sample * (1.0 - blend) + source_sample * blend;
    }
}
