mod analyzer;
mod cache;
mod pitch_synchronous_synthesis;
mod resample_pro_analysis;
mod resample_pro_render;
mod resample_pro_stretch;
mod resample_stretch_compat;
mod spectral;
mod spectral_peak_synthesis;
mod synthesis;
mod synthesis_support;
mod varispeed_synthesis;

pub use analyzer::{PitchShiftAnalysisConfig, PitchShiftAnalysisError, PitchShiftAnalyzer};
pub use cache::{
    PitchShiftFrameAnalysis, PitchShiftSliceSummary, PitchShiftSourceCache, ResampleProCache,
    ResampleProFrame, ResidualEnergyDescriptor, SourceCacheKey, SpectralEnvelope,
    SpectralEnvelopePoint, SpectralPeak, VoicingKind, VoicingSegment,
};
pub use resample_pro_render::{
    ResampleProRenderError, ResampleProRenderRegion, ResampleProRenderState,
    ResampleProSourceRegionRequest,
};
pub use synthesis::{
    PitchShiftEngine, PitchShiftRatios, PitchShiftRegionSampleRequest, PitchShiftRenderConfig,
    PitchShiftRenderError, PitchShiftSliceRenderRequest, PitchShiftSliceSampleRequest,
    PitchShiftSynthesisAlgorithm, ResidualMixPolicy,
};

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

#[cfg(test)]
mod tests;
