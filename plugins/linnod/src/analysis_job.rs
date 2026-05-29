use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use lindelion_onset_detect::OnsetDetector;
use lindelion_pitch_detect::PitchDetector;
use lindelion_plugin_shell::{AsyncJobSequence, SequencedAsyncCache};
use lindelion_sample_library::{
    DEFAULT_WAVEFORM_PREVIEW_POINTS, DecodedSample, FileSampleLibrary, LibraryPaths,
    OwnedMonoAudioBuffer, SampleLibrary, SampleLibraryError, SampleMetadata, SampleReference,
    SampleResolution, decode_wav_mono,
};

use crate::{
    analysis::{LinnodSourceAnalyzer, SourceAnalysis, SourceAnalysisError},
    patch::LinnodPatch,
};

pub type SourceAnalysisSequence = AsyncJobSequence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAnalysisStatus {
    Idle,
    PendingLoad,
    Analyzing,
    Ready,
    MissingSource,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAnalysisJob {
    pub sequence: SourceAnalysisSequence,
    pub request: SourceLoadRequest,
    marker_policy: SourceMarkerPolicy,
    patch: LinnodPatch,
    library_root: PathBuf,
}

impl SourceAnalysisJob {
    pub fn load(
        sequence: SourceAnalysisSequence,
        patch: &LinnodPatch,
        library_root: PathBuf,
    ) -> Option<Self> {
        Self::load_with_marker_policy(
            sequence,
            patch,
            library_root,
            SourceMarkerPolicy::UseSavedMarkersOrDetect,
        )
    }

    pub(crate) fn redetect(
        sequence: SourceAnalysisSequence,
        patch: &LinnodPatch,
        library_root: PathBuf,
    ) -> Option<Self> {
        Self::load_with_marker_policy(
            sequence,
            patch,
            library_root,
            SourceMarkerPolicy::DetectAndMergeUserMarkers,
        )
    }

    pub(crate) fn load_with_marker_policy(
        sequence: SourceAnalysisSequence,
        patch: &LinnodPatch,
        library_root: PathBuf,
        marker_policy: SourceMarkerPolicy,
    ) -> Option<Self> {
        patch.source_sample.as_ref().map(|reference| Self {
            sequence,
            request: SourceLoadRequest::Reference(reference.clone()),
            marker_policy,
            patch: patch.clone(),
            library_root,
        })
    }

    pub fn ingest(
        sequence: SourceAnalysisSequence,
        path: impl Into<PathBuf>,
        patch: &LinnodPatch,
        library_root: PathBuf,
    ) -> Self {
        Self {
            sequence,
            request: SourceLoadRequest::Ingest(path.into()),
            marker_policy: SourceMarkerPolicy::DetectAndMergeUserMarkers,
            patch: patch.clone(),
            library_root,
        }
    }

    pub const fn marker_policy(&self) -> SourceMarkerPolicy {
        self.marker_policy
    }

    pub fn run(self) -> SourceAnalysisJobResult {
        let analyzer = LinnodSourceAnalyzer::default();
        self.run_with_analyzer(&analyzer)
    }

    pub fn run_with_analyzer<D, O>(
        self,
        analyzer: &LinnodSourceAnalyzer<D, O>,
    ) -> SourceAnalysisJobResult
    where
        D: PitchDetector,
        O: OnsetDetector,
    {
        let sequence = self.sequence;
        SourceAnalysisJobResult {
            sequence,
            result: self.run_inner(analyzer),
        }
    }

    fn run_inner<D, O>(
        &self,
        analyzer: &LinnodSourceAnalyzer<D, O>,
    ) -> Result<SourceAnalysis, SourceLoadError>
    where
        D: PitchDetector,
        O: OnsetDetector,
    {
        let (metadata, audio) = self.load_source()?;
        let analysis = match self.marker_policy {
            SourceMarkerPolicy::UseSavedMarkersOrDetect => analyzer.analyze_with_saved_markers(
                metadata,
                audio,
                self.patch.detection,
                &self.patch.markers,
            ),
            SourceMarkerPolicy::DetectAndMergeUserMarkers => {
                analyzer.analyze(metadata, audio, self.patch.detection, &self.patch.markers)
            }
        }?;
        Ok(analysis)
    }

    fn load_source(&self) -> Result<(SampleMetadata, OwnedMonoAudioBuffer), SourceLoadError> {
        let paths = LibraryPaths::from_root(self.library_root.clone());
        let mut library = FileSampleLibrary::open(paths)?;
        match &self.request {
            SourceLoadRequest::Ingest(path) => {
                let metadata = library.ingest(path.clone())?;
                let (_path, decoded) = resolve_decoded(&library, &metadata.reference)?;
                Ok((metadata, OwnedMonoAudioBuffer::from(decoded)))
            }
            SourceLoadRequest::Reference(reference) => {
                let (path, decoded) = resolve_decoded(&library, reference)?;
                let metadata = SampleMetadata::from_decoded(
                    resolved_reference(&library, reference, &path),
                    &path,
                    &decoded,
                    DEFAULT_WAVEFORM_PREVIEW_POINTS,
                );
                Ok((metadata, OwnedMonoAudioBuffer::from(decoded)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLoadRequest {
    Reference(SampleReference),
    Ingest(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMarkerPolicy {
    UseSavedMarkersOrDetect,
    DetectAndMergeUserMarkers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAnalysisJobResult {
    pub sequence: SourceAnalysisSequence,
    pub result: Result<SourceAnalysis, SourceLoadError>,
}

impl SourceAnalysisJobResult {
    pub fn ready(sequence: SourceAnalysisSequence, result: SourceAnalysis) -> Self {
        Self {
            sequence,
            result: Ok(result),
        }
    }

    pub fn error(sequence: SourceAnalysisSequence, error: SourceLoadError) -> Self {
        Self {
            sequence,
            result: Err(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLoadError {
    MissingSource(SampleReference),
    Library(String),
    Decode(String),
    Analysis(SourceAnalysisError),
}

impl fmt::Display for SourceLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSource(reference) => {
                write!(
                    formatter,
                    "source sample is missing: {}",
                    reference.blake3_hash.0
                )
            }
            Self::Library(error) => write!(formatter, "sample library error: {error}"),
            Self::Decode(error) => write!(formatter, "sample decode error: {error}"),
            Self::Analysis(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for SourceLoadError {}

impl From<SampleLibraryError> for SourceLoadError {
    fn from(value: SampleLibraryError) -> Self {
        Self::Library(value.to_string())
    }
}

impl From<lindelion_sample_library::SampleDecodeError> for SourceLoadError {
    fn from(value: lindelion_sample_library::SampleDecodeError) -> Self {
        Self::Decode(value.to_string())
    }
}

impl From<SourceAnalysisError> for SourceLoadError {
    fn from(value: SourceAnalysisError) -> Self {
        Self::Analysis(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAnalysisCache {
    inner: SequencedAsyncCache<SourceAnalysisStatus, SourceAnalysis, SourceLoadError>,
}

impl Default for SourceAnalysisCache {
    fn default() -> Self {
        Self {
            inner: SequencedAsyncCache::new(SourceAnalysisStatus::Idle),
        }
    }
}

impl SourceAnalysisCache {
    pub const fn sequence(&self) -> SourceAnalysisSequence {
        self.inner.sequence()
    }

    pub const fn status(&self) -> SourceAnalysisStatus {
        self.inner.status()
    }

    pub fn analysis(&self) -> Option<&SourceAnalysis> {
        self.inner.output()
    }

    pub fn error(&self) -> Option<&SourceLoadError> {
        self.inner.error()
    }

    pub fn mark_idle(&mut self, sequence: SourceAnalysisSequence) {
        self.inner.mark_empty(sequence, SourceAnalysisStatus::Idle);
    }

    pub fn mark_pending_load(&mut self, sequence: SourceAnalysisSequence) {
        self.inner
            .mark_empty(sequence, SourceAnalysisStatus::PendingLoad);
    }

    pub fn mark_analyzing(&mut self, sequence: SourceAnalysisSequence) {
        self.inner
            .mark_empty(sequence, SourceAnalysisStatus::Analyzing);
    }

    pub fn publish_result(&mut self, result: SourceAnalysisJobResult) -> bool {
        let error_status = match &result.result {
            Err(SourceLoadError::MissingSource(_)) => SourceAnalysisStatus::MissingSource,
            Err(_) => SourceAnalysisStatus::Error,
            Ok(_) => SourceAnalysisStatus::Ready,
        };
        self.inner.publish_result(
            result.sequence,
            result.result,
            SourceAnalysisStatus::Ready,
            error_status,
        )
    }
}

fn resolve_decoded(
    library: &FileSampleLibrary,
    reference: &SampleReference,
) -> Result<(PathBuf, DecodedSample), SourceLoadError> {
    match library.resolve(reference)? {
        SampleResolution::Found(path) => {
            let decoded = decode_wav_mono(&path)?;
            Ok((path, decoded))
        }
        SampleResolution::Missing(reference) => Err(SourceLoadError::MissingSource(reference)),
    }
}

fn resolved_reference(
    library: &FileSampleLibrary,
    reference: &SampleReference,
    path: &Path,
) -> SampleReference {
    let last_known_path = path
        .strip_prefix(&library.paths().root)
        .unwrap_or(path)
        .to_path_buf();
    SampleReference {
        blake3_hash: reference.blake3_hash.clone(),
        last_known_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::LinnodSourceAnalyzer;
    use lindelion_onset_detect::{DetectionConfig, MarkerKind, OnsetDetectionInput, SliceMarker};
    use lindelion_pitch_detect::{PitchContour, PitchDetectionError, PitchFrame};
    use std::{
        fs,
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn cache_rejects_stale_source_analysis_result() {
        let mut cache = SourceAnalysisCache::default();
        cache.mark_analyzing(2);

        assert!(!cache.publish_result(SourceAnalysisJobResult::error(
            1,
            SourceLoadError::Analysis(SourceAnalysisError::EmptySource)
        )));
        assert_eq!(cache.status(), SourceAnalysisStatus::Analyzing);
    }

    #[test]
    fn source_job_ingests_decodes_and_analyzes_off_audio_thread_payload() {
        let root = temp_root("linnod-source-job");
        let source = root.join("source.wav");
        write_test_wav(&source, &[0.0, 0.25, -0.25, 0.5]);
        let patch = LinnodPatch::default();
        let job = SourceAnalysisJob::ingest(4, &source, &patch, root.join("Library"));
        let analyzer = LinnodSourceAnalyzer::new(FixedPitchDetector, FixedOnsetDetector);

        let result = job.run_with_analyzer(&analyzer);

        let analysis = result.result.unwrap();
        assert_eq!(result.sequence, 4);
        assert_eq!(analysis.audio.samples().len(), 4);
        assert_eq!(analysis.source.sample_rate, 48_000);
        assert!(!analysis.source.waveform_preview.points.is_empty());
        assert_eq!(analysis.markers[0].position_samples, 0);
    }

    #[test]
    fn source_job_loads_reference_through_hash_recovery() {
        let root = temp_root("linnod-source-recovery");
        let source = root.join("source.wav");
        write_test_wav(&source, &[0.0, 0.25, -0.25, 0.5]);
        let library_root = root.join("Library");
        let mut library = FileSampleLibrary::open(LibraryPaths::from_root(&library_root)).unwrap();
        let metadata = library.ingest(source).unwrap();
        let original = match library.resolve(&metadata.reference).unwrap() {
            SampleResolution::Found(path) => path,
            SampleResolution::Missing(_) => panic!("expected ingested sample"),
        };
        let moved = library_root.join("Samples/moved/source.wav");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(original, &moved).unwrap();

        let patch = LinnodPatch {
            source_sample: Some(metadata.reference),
            ..LinnodPatch::default()
        };
        let job = SourceAnalysisJob::load(7, &patch, library_root).unwrap();
        let analyzer = LinnodSourceAnalyzer::new(FixedPitchDetector, FixedOnsetDetector);

        let result = job.run_with_analyzer(&analyzer);

        let analysis = result.result.unwrap();
        assert_eq!(result.sequence, 7);
        assert_eq!(
            analysis.source.reference.last_known_path,
            PathBuf::from("Samples/moved/source.wav")
        );
    }

    #[test]
    fn source_load_reuses_saved_markers_and_redetect_forces_detector() {
        let root = temp_root("linnod-source-saved-markers");
        let source = root.join("source.wav");
        let samples = vec![0.2; 4_800];
        write_test_wav(&source, &samples);
        let library_root = root.join("Library");
        let mut library = FileSampleLibrary::open(LibraryPaths::from_root(&library_root)).unwrap();
        let metadata = library.ingest(source).unwrap();
        let saved_markers = vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_400,
                kind: MarkerKind::Auto,
            },
        ];
        let patch = LinnodPatch {
            source_sample: Some(metadata.reference.clone()),
            markers: saved_markers.clone(),
            ..LinnodPatch::default()
        };
        let analyzer = LinnodSourceAnalyzer::new(FixedPitchDetector, FixedOnsetDetector);

        let load_job = SourceAnalysisJob::load(8, &patch, library_root.clone()).unwrap();
        assert_eq!(
            load_job.marker_policy(),
            SourceMarkerPolicy::UseSavedMarkersOrDetect
        );
        let loaded = load_job.run_with_analyzer(&analyzer).result.unwrap();

        let redetect_job = SourceAnalysisJob::redetect(9, &patch, library_root).unwrap();
        assert_eq!(
            redetect_job.marker_policy(),
            SourceMarkerPolicy::DetectAndMergeUserMarkers
        );
        let redetected = redetect_job.run_with_analyzer(&analyzer).result.unwrap();

        assert_eq!(loaded.markers, saved_markers);
        assert_eq!(
            redetected.markers,
            vec![SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            }]
        );
    }

    #[derive(Debug, Clone)]
    struct FixedPitchDetector;

    impl PitchDetector for FixedPitchDetector {
        fn detect(
            &self,
            _audio: &[f32],
            sample_rate: u32,
        ) -> Result<PitchContour, PitchDetectionError> {
            Ok(PitchContour {
                source_sample_rate: sample_rate,
                analysis_sample_rate: sample_rate,
                hop_size: 256,
                frames: vec![PitchFrame {
                    frame_index: 0,
                    source_sample_position: 0,
                    timestamp_seconds: 0.0,
                    f0_hz: Some(440.0),
                    raw_f0_hz: 440.0,
                    confidence: 0.95,
                    voiced: true,
                    rms: 0.2,
                }],
            })
        }
    }

    #[derive(Debug, Clone)]
    struct FixedOnsetDetector;

    impl OnsetDetector for FixedOnsetDetector {
        fn detect(
            &self,
            input: OnsetDetectionInput<'_>,
            _config: DetectionConfig,
        ) -> Vec<SliceMarker> {
            assert!(input.pitch_track.is_some());
            vec![SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            }]
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lindelion-{name}-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_test_wav(path: &Path, samples: &[f32]) {
        let mut file = fs::File::create(path).unwrap();
        let data_len = samples.len() as u32 * 2;
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&48_000u32.to_le_bytes()).unwrap();
        file.write_all(&(48_000u32 * 2).to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();
        for sample in samples {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            file.write_all(&pcm.to_le_bytes()).unwrap();
        }
        file.flush().unwrap();
    }
}
