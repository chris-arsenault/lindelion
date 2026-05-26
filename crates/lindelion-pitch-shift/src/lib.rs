mod analyzer;
mod cache;
mod spectral;
mod synthesis;

pub use analyzer::{PitchShiftAnalysisConfig, PitchShiftAnalysisError, PitchShiftAnalyzer};
pub use cache::{
    PitchShiftFrameAnalysis, PitchShiftSliceSummary, PitchShiftSourceCache,
    ResidualEnergyDescriptor, SourceCacheKey, SpectralEnvelope, SpectralEnvelopePoint, VoicingKind,
    VoicingSegment,
};
pub use synthesis::{
    PitchShiftEngine, PitchShiftRatios, PitchShiftRegionSampleRequest, PitchShiftRenderConfig,
    PitchShiftRenderError, PitchShiftSliceRenderRequest, PitchShiftSliceSampleRequest,
    ResidualMixPolicy,
};

#[cfg(test)]
mod tests;
