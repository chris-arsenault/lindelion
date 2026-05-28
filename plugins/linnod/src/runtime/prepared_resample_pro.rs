use lindelion_dsp_utils::interpolation;
use lindelion_pitch_shift::{
    PitchShiftRatios, ResampleProRenderState, ResampleProSourceRegionRequest, SourceCacheKey,
};

use crate::{LinnodPatch, SourceAnalysis};

use super::{
    pitch_source::is_identity_pitch_request,
    trigger::{LinnodVoiceTrigger, for_each_preparable_trigger_note, voice_trigger_from_note},
};

const MAX_PREPARED_PITCH_RATIO: f64 = 8.0;
const RESAMPLE_PRO_RENDER_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Default)]
pub(super) struct PreparedResampleProSlices {
    variants: Vec<PreparedResampleProVariant>,
    render_errors: usize,
    render_count: usize,
}

impl PreparedResampleProSlices {
    pub fn prepare(&mut self, patch: &LinnodPatch, analysis: &SourceAnalysis, sample_rate: f32) {
        self.variants.clear();
        self.render_errors = 0;
        self.render_count = 0;
        let Ok(mut renderer) =
            ResampleProRenderState::new(&analysis.pitch_shift_cache, MAX_PREPARED_PITCH_RATIO)
        else {
            self.render_errors += 1;
            return;
        };

        for_each_preparable_trigger_note(patch, |note| {
            let Some(trigger) = voice_trigger_from_note(patch, analysis, note, sample_rate, 1.0)
            else {
                return;
            };
            if !trigger.needs_prepared_resample_pro() {
                return;
            }
            self.prepare_trigger(analysis, &mut renderer, trigger);
        });
    }

    pub fn sample(&self, request: PreparedResampleProSampleRequest) -> Option<f32> {
        if request.offset_samples < 0.0 {
            return Some(0.0);
        }
        let key = PreparedResampleProVariantKey::from_sample_request(request);
        let variant = self.variants.iter().find(|variant| variant.key == key)?;
        let duration = variant.samples.len() as f32;
        if request.offset_samples >= duration {
            return Some(0.0);
        }
        Some(interpolation::cubic_f64(
            &variant.samples,
            request.offset_samples as f64,
        ))
    }

    #[cfg(test)]
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    #[cfg(test)]
    pub fn render_count(&self) -> usize {
        self.render_count
    }

    fn prepare_trigger(
        &mut self,
        analysis: &SourceAnalysis,
        renderer: &mut ResampleProRenderState,
        trigger: LinnodVoiceTrigger,
    ) {
        let key =
            PreparedResampleProVariantKey::from_trigger(analysis.pitch_shift_cache.key, trigger);
        if self.variants.iter().any(|variant| variant.key == key) {
            return;
        }

        let duration = trigger
            .source_end_sample
            .saturating_sub(trigger.source_start_sample);
        let mut samples = vec![0.0; duration];
        let rendered = renderer.render_source_region_pitch_shift_to(
            ResampleProSourceRegionRequest {
                source: analysis.audio.samples(),
                cache: &analysis.pitch_shift_cache,
                start_sample: trigger.source_start_sample,
                end_sample: trigger.source_end_sample,
                ratios: trigger.ratios,
            },
            &mut samples,
        );
        if rendered.is_ok() {
            self.variants
                .push(PreparedResampleProVariant { key, samples });
            self.render_count += 1;
        } else {
            self.render_errors += 1;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreparedResampleProSampleRequest {
    pub source_key: SourceCacheKey,
    pub slice_index: usize,
    pub source_start_sample: usize,
    pub source_end_sample: usize,
    pub offset_samples: f32,
    pub ratios: PitchShiftRatios,
    pub reverse: bool,
}

#[derive(Debug)]
struct PreparedResampleProVariant {
    key: PreparedResampleProVariantKey,
    samples: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedResampleProVariantKey {
    source_key: SourceCacheKey,
    slice_index: usize,
    source_start_sample: usize,
    source_end_sample: usize,
    ratios: PreparedRatiosKey,
    reverse: bool,
    render_config: PreparedRenderConfigKey,
}

impl PreparedResampleProVariantKey {
    fn from_trigger(source_key: SourceCacheKey, trigger: LinnodVoiceTrigger) -> Self {
        Self {
            source_key,
            slice_index: trigger.slice_index,
            source_start_sample: trigger.source_start_sample,
            source_end_sample: trigger.source_end_sample,
            ratios: PreparedRatiosKey::from(trigger.ratios),
            reverse: trigger.reverse,
            render_config: PreparedRenderConfigKey::current(),
        }
    }

    fn from_sample_request(request: PreparedResampleProSampleRequest) -> Self {
        Self {
            source_key: request.source_key,
            slice_index: request.slice_index,
            source_start_sample: request.source_start_sample,
            source_end_sample: request.source_end_sample,
            ratios: PreparedRatiosKey::from(request.ratios),
            reverse: request.reverse,
            render_config: PreparedRenderConfigKey::current(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedRatiosKey {
    pitch_ratio: u32,
    formant_ratio: Option<u32>,
}

impl From<PitchShiftRatios> for PreparedRatiosKey {
    fn from(value: PitchShiftRatios) -> Self {
        let value = value.sanitized();
        Self {
            pitch_ratio: value.pitch_ratio.to_bits(),
            formant_ratio: value.formant_ratio.map(f32::to_bits),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedRenderConfigKey {
    version: u32,
}

impl PreparedRenderConfigKey {
    const fn current() -> Self {
        Self {
            version: RESAMPLE_PRO_RENDER_CONFIG_VERSION,
        }
    }
}

impl LinnodVoiceTrigger {
    fn needs_prepared_resample_pro(self) -> bool {
        matches!(
            self.algorithm,
            lindelion_pitch_shift::PitchShiftSynthesisAlgorithm::ResampleStretch
        ) && !is_identity_pitch_request(self.ratios)
    }
}
