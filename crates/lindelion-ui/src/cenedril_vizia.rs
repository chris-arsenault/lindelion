//! Cenedril editor surface.
//!
//! The size/host types and the [`spectrogram`] model are platform-neutral and compile everywhere
//! (including Linux `make ci`). The Vizia view and the Windows `IPlugView`→`HWND` attach live in the
//! `windows`-gated `platform` submodule, because `vizia` is a Windows-target dependency for the new
//! VSTs (ADR-0023).
//!
//! Frames reach the editor through the [`SpectrogramSource`] trait, which the plugin implements over
//! its lock-free audio→editor ring — so this UI crate never names the realtime ring type and stays
//! free of audio-thread infrastructure.

use std::sync::Arc;

pub mod spectrogram;

pub const CENEDRIL_EDITOR_WIDTH: i32 = 720;
pub const CENEDRIL_EDITOR_HEIGHT: i32 = 480;

#[derive(Debug, Clone, Copy)]
pub struct CenedrilEditorSize {
    pub width: i32,
    pub height: i32,
}

/// The editor's source of STFT magnitude frames, implemented by the plugin over its lock-free
/// audio→editor ring. The view drains it on the UI thread; the realtime ring type stays in the
/// plugin crate.
pub trait SpectrogramSource: Send + Sync {
    /// Deliver magnitude frames written since the last call, oldest first (editor thread, lock-free).
    fn drain_frames(&self, sink: &mut dyn FnMut(&[f32]));
    /// Bins per frame (`= frame_size/2 + 1`).
    fn bins(&self) -> usize;
    /// STFT frame size (`= (bins - 1) * 2`).
    fn frame_size(&self) -> usize;
    /// Audio sample rate, for the log-frequency axis.
    fn sample_rate(&self) -> f32;
}

/// Everything the editor needs from the plugin. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct CenedrilEditorHost {
    pub source: Arc<dyn SpectrogramSource>,
}

impl CenedrilEditorHost {
    pub fn new(source: Arc<dyn SpectrogramSource>) -> Self {
        Self { source }
    }
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::CenedrilViziaEditor;
