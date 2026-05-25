use super::*;
use std::{
    fs,
    io::{Seek, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn owned_mono_audio_buffer_sanitizes_samples_and_rate() {
    let buffer = OwnedMonoAudioBuffer::new(vec![0.25, f32::NAN, f32::INFINITY], f32::NAN);

    assert_eq!(buffer.sample_rate, DEFAULT_AUDIO_SAMPLE_RATE_HZ);
    assert_eq!(buffer.samples, vec![0.25, 0.0, 0.0]);
    assert_eq!(
        OwnedMonoAudioBuffer::new(vec![0.0], 768_000.0).runtime_sample_rate(),
        MAX_RUNTIME_AUDIO_SAMPLE_RATE_HZ
    );
}

#[test]
fn library_patch_file_path_sanitizes_patch_names() {
    let paths = LibraryPaths::from_root("Library");

    assert_eq!(
        paths.patch_file_path("Lead/Pad:01"),
        PathBuf::from("Library/Patches/Lead-Pad-01.toml")
    );
    assert_eq!(
        paths.patch_file_path("   "),
        PathBuf::from("Library/Patches/Untitled.toml")
    );
}

#[test]
fn save_library_patch_to_path_uses_shared_patch_path_policy() {
    let root = temp_root("library-patch-save");
    let paths = LibraryPaths::from_root(root.join("Library"));

    let path = save_library_patch_to_path(&paths, "Lead/Pad:01", |path| {
        fs::write(path, b"patch").map_err(SampleDecodeError::Io)
    })
    .unwrap();

    assert!(path.ends_with("Lead-Pad-01.toml"));
    assert_eq!(fs::read(&path).unwrap(), b"patch");
}

#[test]
fn decoded_sample_metadata_uses_shared_preview_and_level_policy() {
    let audio = DecodedSample {
        samples: vec![0.0, 0.5, -0.5, 0.25],
        sample_rate: 1_000,
        channels: 1,
    };
    let metadata = SampleMetadata::from_decoded(
        SampleReference::new("hash", "Samples/source.wav"),
        Path::new("source.wav"),
        &audio,
        2,
    );

    assert_eq!(metadata.filename, "source.wav");
    assert_eq!(metadata.duration_ms, 4);
    assert_eq!(metadata.peak_db, Some(-6.0206003));
    assert_eq!(metadata.waveform_preview.points.len(), 2);
}

#[test]
fn referenced_sample_loader_resolves_slots_and_reports_missing_samples() {
    struct StaticLibrary {
        found_path: PathBuf,
    }

    impl SampleLibrary for StaticLibrary {
        type Error = ();

        fn resolve(&self, reference: &SampleReference) -> Result<SampleResolution, Self::Error> {
            Ok(if reference.blake3_hash.0 == "found" {
                SampleResolution::Found(self.found_path.clone())
            } else {
                SampleResolution::Missing(reference.clone())
            })
        }

        fn ingest(&mut self, _path: PathBuf) -> Result<SampleMetadata, Self::Error> {
            unimplemented!("test library only resolves samples")
        }
    }

    let root = temp_root("referenced-loader");
    let sample_path = root.join("source.wav");
    write_test_wav(&sample_path, &[0.0, 0.5, -0.5]);
    let found = SampleReference::new("found", &sample_path);
    let missing = SampleReference::new("missing", root.join("missing.wav"));
    let library = StaticLibrary {
        found_path: sample_path,
    };

    let (buffers, report) =
        load_referenced_mono_audio_from_library::<4, _, _>([(1, &found), (3, &missing)], &library)
            .unwrap();

    assert!(buffers[0].is_none());
    assert_eq!(buffers[1].as_ref().unwrap().sample_rate, 48_000);
    assert_eq!(buffers[1].as_ref().unwrap().samples.len(), 3);
    assert!(buffers[3].is_none());
    assert_eq!(report.loaded_slots, 1);
    assert_eq!(report.missing_samples, vec![missing]);
}

#[test]
fn file_library_ingests_hashes_indexes_and_previews_wav_samples() {
    let root = temp_root("ingest");
    let source = root.join("source.wav");
    write_test_wav(&source, &[0.0, 0.5, -0.5, 0.25]);
    let paths = LibraryPaths::from_root(root.join("Library"));
    let mut library = FileSampleLibrary::open(paths.clone()).unwrap();

    let metadata = library.ingest(source).unwrap();

    assert_ingested_metadata(&paths, &metadata);
    assert_resolves_existing_sample(&library, &metadata);
    let listed = library.list_samples().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].reference, metadata.reference);
    assert!(!listed[0].waveform_preview.points.is_empty());
}

fn assert_ingested_metadata(paths: &LibraryPaths, metadata: &SampleMetadata) {
    assert!(paths.index_db.exists());
    assert!(metadata.reference.blake3_hash.0.len() >= 32);
    assert_eq!(metadata.sample_rate, 48_000);
    assert_eq!(metadata.channels, 1);
    assert_eq!(metadata.duration_ms, 0);
    assert!(metadata.peak_db.unwrap() <= 0.0);
    assert!(metadata.rms_db.unwrap() < 0.0);
    assert!(!metadata.waveform_preview.points.is_empty());
}

fn assert_resolves_existing_sample(library: &FileSampleLibrary, metadata: &SampleMetadata) {
    assert!(matches!(
        library.resolve(&metadata.reference).unwrap(),
        SampleResolution::Found(path) if path.exists()
    ));
}

#[test]
fn file_library_resolves_moved_samples_by_hash_and_reports_missing_samples() {
    let root = temp_root("resolve");
    let source = root.join("source.wav");
    write_test_wav(&source, &[0.25, -0.25, 0.75, -0.75]);
    let paths = LibraryPaths::from_root(root.join("Library"));
    let mut library = FileSampleLibrary::open(paths.clone()).unwrap();
    let metadata = library.ingest(source).unwrap();
    let original = match library.resolve(&metadata.reference).unwrap() {
        SampleResolution::Found(path) => path,
        SampleResolution::Missing(_) => panic!("expected sample to resolve"),
    };
    let moved = paths.samples.join("moved").join("source.wav");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&original, &moved).unwrap();

    let resolved = library.resolve(&metadata.reference).unwrap();
    assert_eq!(resolved, SampleResolution::Found(moved.clone()));

    fs::remove_file(moved).unwrap();
    let missing = library.resolve(&metadata.reference).unwrap();
    assert_eq!(missing, SampleResolution::Missing(metadata.reference));
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
    file.rewind().unwrap();
}
