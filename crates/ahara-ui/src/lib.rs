#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformPoint {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

impl WaveformPoint {
    pub fn silence() -> Self {
        Self {
            min: 0.0,
            max: 0.0,
            rms: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveformPreview {
    pub sample_rate: u32,
    pub points: Vec<WaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PadId(pub u8);

impl PadId {
    pub fn new(index: u8) -> Option<Self> {
        if (1..=16).contains(&index) {
            Some(Self(index))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Pad,
    Chromatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    SavePatch,
    LoadPatch,
    ExportPatchWithSamples,
    OpenLibrary,
    RedetectSlices,
    TuneSelectedSlice,
    TuneAllSlices,
    SnapAllSlicesToScale,
}
