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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFileSlotId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFileSlotListView {
    pub selected: AudioFileSlotId,
    pub slots: Vec<AudioFileSlotView>,
}

impl Default for AudioFileSlotListView {
    fn default() -> Self {
        Self {
            selected: AudioFileSlotId(0),
            slots: Vec::new(),
        }
    }
}

pub trait AudioFileSlotListSurface: Send + Sync {
    fn slot_list_view(&self) -> AudioFileSlotListView;
    fn select_slot(&self, slot: AudioFileSlotId);
    fn load_audio_file(&self, slot: AudioFileSlotId, path: &Path);
    fn clear_audio_file(&self, slot: AudioFileSlotId);
}

#[derive(Clone)]
pub struct AudioFileSlotListHost {
    pub surface: Arc<dyn AudioFileSlotListSurface>,
}

impl AudioFileSlotListHost {
    pub fn new(surface: Arc<dyn AudioFileSlotListSurface>) -> Self {
        Self { surface }
    }
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
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

    impl AudioFileSlotListSurface for StubSlot {
        fn slot_list_view(&self) -> AudioFileSlotListView {
            AudioFileSlotListView {
                selected: AudioFileSlotId(0),
                slots: vec![AudioFileSlotView::default()],
            }
        }

        fn select_slot(&self, _slot: AudioFileSlotId) {}

        fn load_audio_file(&self, _slot: AudioFileSlotId, _path: &Path) {}

        fn clear_audio_file(&self, _slot: AudioFileSlotId) {}
    }

    #[test]
    fn host_wraps_shared_audio_slot_surface() {
        let host = AudioFileSlotHost::new(Arc::new(StubSlot));
        assert_eq!(host.surface.slot_view().source, AudioFileSource::BuiltIn);
    }

    #[test]
    fn list_host_wraps_indexed_audio_slots() {
        let host = AudioFileSlotListHost::new(Arc::new(StubSlot));
        assert_eq!(host.surface.slot_list_view().slots.len(), 1);
        assert!(is_supported_audio_file(Path::new("sample.WAV")));
        assert!(!is_supported_audio_file(Path::new("sample.aiff")));
    }
}
