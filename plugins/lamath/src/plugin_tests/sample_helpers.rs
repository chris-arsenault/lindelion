struct StaticSampleLibrary {
    path: Option<PathBuf>,
}

impl SampleLibrary for StaticSampleLibrary {
    type Error = ();

    fn resolve(&self, reference: &SampleReference) -> Result<SampleResolution, Self::Error> {
        Ok(match &self.path {
            Some(path) => SampleResolution::Found(path.clone()),
            None => SampleResolution::Missing(reference.clone()),
        })
    }

    fn ingest(
        &mut self,
        _path: PathBuf,
    ) -> Result<lindelion_sample_library::SampleMetadata, Self::Error> {
        unimplemented!("test library only resolves existing references")
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
}
