use std::{path::Path, sync::Arc};

use crate::WaveformPoint;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFileSlotView {
    pub label: String,
    pub source: AudioFileSource,
    pub waveform: Vec<WaveformPoint>,
}

impl Default for AudioFileSlotView {
    fn default() -> Self {
        Self {
            label: "Built-in strike".to_string(),
            source: AudioFileSource::BuiltIn,
            waveform: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFileSource {
    BuiltIn,
    Loaded,
}

pub trait AudioFileSlotSurface: Send + Sync {
    fn slot_view(&self) -> AudioFileSlotView;
    fn load_audio_file(&self, path: &Path);
    fn clear_audio_file(&self);
}

#[derive(Clone)]
pub struct AudioFileSlotHost {
    pub surface: Arc<dyn AudioFileSlotSurface>,
}

impl AudioFileSlotHost {
    pub fn new(surface: Arc<dyn AudioFileSlotSurface>) -> Self {
        Self { surface }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubSlot;

    impl AudioFileSlotSurface for StubSlot {
        fn slot_view(&self) -> AudioFileSlotView {
            AudioFileSlotView::default()
        }

        fn load_audio_file(&self, _path: &Path) {}

        fn clear_audio_file(&self) {}
    }

    #[test]
    fn host_wraps_shared_audio_slot_surface() {
        let host = AudioFileSlotHost::new(Arc::new(StubSlot));
        assert_eq!(host.surface.slot_view().source, AudioFileSource::BuiltIn);
    }
}
