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

pub mod meters;
pub mod reassigned;
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

/// One drained reassignment frame: the three per-bin lanes the reassigned-spectrogram view scatters
/// (magnitude, fractional-bin frequency offset, sample time offset relative to the window center).
pub struct ReassignedFrame<'a> {
    pub magnitudes: &'a [f32],
    pub freq_offsets: &'a [f32],
    pub time_offsets: &'a [f32],
}

/// The editor's source of **reassignment** frames, implemented by the plugin over the same
/// lock-free audio→editor ring as [`SpectrogramSource`] (the ring carries all three lanes). The
/// reassigned view drains this on the UI thread; the realtime ring type stays in the plugin crate.
pub trait ReassignedSource: Send + Sync {
    /// Deliver reassignment frames written since the last call, oldest first (editor thread,
    /// lock-free).
    fn drain_frames(&self, sink: &mut dyn FnMut(ReassignedFrame));
    /// Bins per frame (`= frame_size/2 + 1`).
    fn bins(&self) -> usize;
    /// STFT frame size (`= (bins - 1) * 2`).
    fn frame_size(&self) -> usize;
    /// Analysis hop in samples (`= frame_size / 4`), for mapping the time offset to a scroll column.
    fn hop(&self) -> usize;
    /// Audio sample rate, for the log-frequency axis.
    fn sample_rate(&self) -> f32;
}

/// The editor's source of meter + analysis-signal readouts, implemented by the plugin over its
/// `Arc` atomic cells (the same object that drains the spectrogram ring). Read on the UI thread's
/// refresh tick; the realtime cell types stay in the plugin crate.
pub trait MeterSource: Send + Sync {
    /// The latest level/loudness readout (peak/RMS/crest + LUFS).
    fn meters(&self) -> meters::MeterReadout;
    /// The latest analysis-signal readout (voicing/pitch/flux/HNR + speech presence).
    fn signals(&self) -> meters::SignalReadout;
}

/// The editor's persisted display settings, implemented by the plugin over its settings cell. The
/// values are UI-framework-neutral (the editor maps them to its `ViewMode`/`FreqScale`/`ColorMap`):
/// `active_view`/`freq_scale`/`color_map` are `u32` discriminants, `db_floor`/`db_ceil` are dB. The
/// editor reads them on open and writes them when a control changes; `state`/`load_state` persist them.
pub trait SettingsStore: Send + Sync {
    fn active_view(&self) -> u32;
    fn freq_scale(&self) -> u32;
    fn color_map(&self) -> u32;
    fn db_floor(&self) -> f32;
    fn db_ceil(&self) -> f32;
    fn set_active_view(&self, value: u32);
    fn set_freq_scale(&self, value: u32);
    fn set_color_map(&self, value: u32);
    fn set_db_floor(&self, value: f32);
    fn set_db_ceil(&self, value: f32);
}

/// Everything the editor needs from the plugin. Cheap to clone (`Arc` inside). The magnitude and
/// reassigned sources drain the **same** lock-free ring (typically two trait-object handles to one
/// concrete source); the editor drains whichever the active view selects. `meters` reads the meter +
/// analysis-signal cells of that same object; `settings` reads/writes the persisted editor settings.
#[derive(Clone)]
pub struct CenedrilEditorHost {
    pub source: Arc<dyn SpectrogramSource>,
    pub reassigned: Arc<dyn ReassignedSource>,
    pub meters: Arc<dyn MeterSource>,
    pub settings: Arc<dyn SettingsStore>,
}

impl CenedrilEditorHost {
    pub fn new(
        source: Arc<dyn SpectrogramSource>,
        reassigned: Arc<dyn ReassignedSource>,
        meters: Arc<dyn MeterSource>,
        settings: Arc<dyn SettingsStore>,
    ) -> Self {
        Self {
            source,
            reassigned,
            meters,
            settings,
        }
    }
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::CenedrilViziaEditor;
