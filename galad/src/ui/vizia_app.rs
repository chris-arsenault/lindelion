//! Vizia host UI — Model + views + the standalone `Application`, controller, and entry point (M6
//! Steps 6–7). Windows-only. The neutral `HostUiState`/`UiCommand` (Steps 3–4, Linux-tested) are the
//! source of truth; the `Model` wraps a `HostUiState` and mirrors its bindable parts into reactive
//! `Signal`s the views read. The **controller** (`Runtime`, folded into the `Model`) owns the live
//! `AudioEngine` (M2/M3), the `EditorHost` (M5), and the loaded `.vst3` modules, and executes the
//! effectful commands: start/stop, live chain edits republished through the M3 `Handoff`, plugin
//! editors, and session save/load (M4). A Vizia timer ticks the meters off the audio thread.
//!
//! The chain is prepared at the **input device's actual sample rate** (`device_sample_rate`) — the
//! host declares the true rate to plugins via `setupProcessing` and never resamples; input and output
//! devices are required to share a rate (a single-channel router does not reconcile two clocks).
//!
//! Plugin state is preserved across live chain edits: the controller keeps a **persistent pool** of
//! plugin instances (`Runtime::pool`), and an edit (add/remove/reorder/bypass while running) rebuilds
//! only the *ordering* over the same live instances (`ChainProcessor` holds shared `Arc`s),
//! republished gaplessly through the M3 `Handoff`. Instances are never recreated on an edit, so
//! parameters and transient DSP state survive; session save/load round-trip the opaque state (M4).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::Duration;

use vizia::icons::{
    ICON_ADJUSTMENTS, ICON_CHEVRON_DOWN, ICON_CHEVRON_UP, ICON_DEVICE_FLOPPY, ICON_FOLDER,
    ICON_FOLDER_PLUS, ICON_PLAYER_PLAY, ICON_PLAYER_STOP, ICON_PLUS, ICON_POWER, ICON_REFRESH,
    ICON_TRASH, ICON_VOLUME, ICON_X,
};
use vizia::prelude::*;
use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;

use crate::audio::{
    AudioDirection, AudioEngine, MasterSettings, MeterSnapshot, default_device, device_sample_rate,
    enumerate,
};
use crate::diagnostics;
use crate::session::{AppSettings, DeviceRef, HostSession};
use crate::ui::command::{UiCommand, apply};
use crate::ui::state::{Dir, HostUiState, MASTER_GAIN_MAX_DB, MASTER_GAIN_MIN_DB, PluginCatalog};
use crate::vst3_host::{
    ChainProcessor, EditorHost, HostContext, HostError, PluginInstance, PoolSlot, ProcessDriver,
    SessionSlot, capture_session, load_module, restore_pool, validate_plugin,
};

const STYLE: &str = r#"
    :root {
        background-color: #111517;
        color: #cdd6d0;
        font-size: 12px;
    }

    label {
        color: #c2ccc6;
        text-wrap: false;
    }

    .root { background-color: #111517; }

    .top-strip {
        background-color: #181e20;
        border-width: 1px;
        border-color: #2c3437;
        corner-radius: 7px;
        padding-left: 12px;
        padding-right: 12px;
    }

    .panel,
    .strip {
        background-color: #161c1e;
        border-width: 1px;
        border-color: #2c3437;
        corner-radius: 7px;
    }

    .wordmark { color: #f1f6f2; font-size: 17px; }

    .brand-mark {
        background-color: #59b6d8;
        corner-radius: 3px;
        width: 12px;
        height: 12px;
    }

    .run-chip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        corner-radius: 5px;
        color: #9aa6a0;
        font-size: 9px;
        padding-left: 8px;
        padding-right: 8px;
        alignment: center;
    }

    .run-chip.is-live {
        background-color: #24372d;
        border-color: #79ad89;
        color: #dff3e3;
    }

    .status-text {
        color: #8d9994;
        font-size: 11px;
        text-overflow: ellipsis;
    }

    .accent-bar {
        width: 4px;
        height: 22px;
        corner-radius: 2px;
    }

    .accent-audio { background-color: #59b6d8; }
    .accent-tone { background-color: #7ed06d; }
    .accent-transport { background-color: #ef6f88; }
    .accent-warn { background-color: #d7a540; }

    .section-title { color: #eef4ef; font-size: 12px; }

    .section-sub {
        color: #7c8883;
        font-size: 9px;
        text-overflow: ellipsis;
    }

    .field-label { color: #8e9a94; font-size: 10px; }
    .value-strong { color: #eef5f0; font-size: 12px; }

    .muted {
        color: #8d9994;
        font-size: 11px;
        text-overflow: ellipsis;
    }

    .divider {
        background-color: #29302f;
        height: 1px;
    }

    .v-divider {
        background-color: #29302f;
        width: 1px;
    }

    .slot-name {
        color: #e9f0eb;
        font-size: 13px;
        text-overflow: ellipsis;
    }

    .add-slot {
        background-color: #181f1c;
        border-width: 1px;
        border-color: #3a4a44;
        corner-radius: 6px;
    }

    .add-slot:hover {
        background-color: #1e2a26;
        border-color: #7ed06d;
    }

    .add-slot .btn-icon {
        color: #8fd3b3;
        fill: #8fd3b3;
    }

    .add-slot-label { color: #aed9c4; font-size: 12px; }

    .backdrop {
        background-color: #06080a;
        border-width: 0px;
        corner-radius: 0px;
        opacity: 0.62;
    }

    .modal {
        background-color: #181e20;
        border-width: 1px;
        border-color: #36444a;
        corner-radius: 9px;
    }

    .modal-title { color: #f1f6f2; font-size: 14px; }

    .overlay-layer { display: none; }
    .overlay-layer.is-open { display: flex; }

    button {
        background-color: #1b2225;
        border-width: 1px;
        border-color: #313c3f;
        corner-radius: 5px;
        color: #d5e0da;
        alignment: center;
    }

    button:hover { border-color: #59b6d8; }

    .tool-button {
        background-color: #20282b;
        border-color: #39464b;
        font-size: 11px;
    }

    .tool-button:hover {
        background-color: #273235;
        border-color: #59b6d8;
    }

    .btn-icon {
        color: #d5e0da;
        fill: #d5e0da;
        width: 15px;
        height: 15px;
    }

    .icon-btn { alignment: center; }
    .icon-btn:hover { background-color: #232d30; border-color: #59b6d8; }

    .icon-btn.danger:hover { border-color: #ef6f88; background-color: #33282a; }
    .icon-btn.danger:hover .btn-icon { color: #f2b8bd; fill: #f2b8bd; }

    .icon-btn.add {
        background-color: #213b2c;
        border-color: #4f8c66;
    }
    .icon-btn.add .btn-icon { color: #cdeecf; fill: #cdeecf; }
    .icon-btn.add:hover { border-color: #7ed06d; background-color: #264634; }

    .slot {
        background-color: #1b2225;
        border-width: 1px;
        border-color: #2f3a3d;
        corner-radius: 6px;
        padding-left: 8px;
        padding-right: 8px;
    }

    .slot:hover { border-color: #3c4a4d; }

    .slot.is-bypassed {
        background-color: #1f1d18;
        border-color: #6c573b;
    }

    .slot-index { color: #6b7873; font-size: 10px; alignment: center; }

    .slot-power .btn-icon { color: #79c39c; fill: #79c39c; }
    .slot-power.is-bypassed .btn-icon { color: #cf9f5a; fill: #cf9f5a; }

    .slot-empty {
        background-color: #14191b;
        border-width: 1px;
        border-color: #2a3336;
        corner-radius: 6px;
        alignment: center;
    }

    .row-name {
        color: #e4ece7;
        font-size: 12px;
        text-overflow: ellipsis;
    }

    .row-path {
        color: #6b7873;
        font-size: 9px;
        text-overflow: ellipsis;
    }

    .row-reason {
        color: #c9a06a;
        font-size: 9px;
        text-overflow: ellipsis;
    }

    .mini-chip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        corner-radius: 4px;
        color: #93a39c;
        font-size: 8px;
        padding-left: 5px;
        padding-right: 5px;
        height: 16px;
        alignment: center;
    }

    .dot {
        background-color: #53605b;
        corner-radius: 4px;
        width: 7px;
        height: 7px;
    }

    .dot.is-ok { background-color: #7ed06d; }
    .dot.is-error { background-color: #ef6f88; }

    .vendor-header {
        color: #8fb6c9;
        font-size: 9px;
        padding-left: 2px;
        padding-top: 4px;
    }

    .browse-row {
        background-color: #1b2225;
        border-width: 1px;
        border-color: #2a3336;
        corner-radius: 5px;
        padding-left: 8px;
        padding-right: 8px;
    }

    .browse-row.is-compatible:hover { border-color: #59b6d8; }

    .folder-row {
        background-color: #1b2225;
        border-width: 1px;
        border-color: #2a3336;
        corner-radius: 5px;
        padding-left: 7px;
        padding-right: 7px;
    }

    .inline-icon { color: #8e9a94; fill: #8e9a94; width: 16px; height: 16px; }
    .inline-icon-sm { color: #6b7873; fill: #6b7873; width: 13px; height: 13px; }

    .meter-label { color: #7c8883; font-size: 9px; }

    .meter-track {
        background-color: #0f1416;
        border-width: 1px;
        border-color: #2a3336;
        corner-radius: 3px;
    }

    .meter-fill {
        background-color: #66c08a;
        corner-radius: 2px;
    }

    .meter-fill.is-hot { background-color: #ef6f88; }

    .transport-btn {
        background-color: #234a36;
        border-color: #5aa37c;
        color: #e9f8ef;
        font-size: 13px;
    }

    .transport-btn:hover { background-color: #2a5a41; border-color: #7ed06d; }
    .transport-btn .btn-icon { color: #e9f8ef; fill: #e9f8ef; }

    .transport-btn.is-stop {
        background-color: #45232a;
        border-color: #ef6f88;
        color: #f8dde1;
    }
    .transport-btn.is-stop:hover { background-color: #532a32; border-color: #ff8a9e; }
    .transport-btn.is-stop .btn-icon { color: #f8dde1; fill: #f8dde1; }

    .mute-btn.mute-on { background-color: #45232a; border-color: #ef6f88; }
    .mute-btn.mute-on .btn-icon { color: #f2b8bd; fill: #f2b8bd; }

    select.device-select {
        width: 1s;
        min-width: auto;
        height: 32px;
    }

    select.device-select button {
        background-color: #11171a;
        border-width: 1px;
        border-color: #2e383c;
        corner-radius: 5px;
        color: #e3ece7;
        width: 1s;
        height: 32px;
        alignment: left;
        padding-left: 10px;
        padding-right: 10px;
    }

    select.device-select button:hover { border-color: #59b6d8; }

    select.device-select button > * {
        width: 1s;
        min-width: auto;
        alignment: center;
    }

    select.device-select label {
        color: #e3ece7;
        text-wrap: false;
        text-overflow: ellipsis;
    }

    select.device-select svg {
        fill: #8ea29b;
        color: #8ea29b;
    }

    select.device-select popup {
        background-color: #181e20;
        border-width: 1px;
        border-color: #3a474c;
        corner-radius: 6px;
    }

    select.device-select list.selectable list-item {
        background-color: #181e20;
        color: #dce6e0;
        height: 30px;
    }

    select.device-select list.selectable list-item:hover,
    select.device-select list.selectable list-item.focused,
    select.device-select list.selectable list-item:focus-visible {
        background-color: #213034;
    }

    select.device-select list.selectable list-item .checkmark {
        fill: #59b6d8;
    }

    scrollview {
        overflow: hidden;
    }

    scrollview > scrollbar {
        display: none;
    }

    scrollview.v-scroll > scrollbar,
    scrollview.h-scroll > scrollbar {
        display: flex;
    }

    scrollbar.vertical {
        width: 7px;
    }

    scrollbar.horizontal {
        height: 7px;
    }

    scrollbar .thumb {
        background-color: #4a5559;
        corner-radius: 3px;
        opacity: 0.8;
    }

    slider.master-slider {
        height: 16px;
        width: 1s;
        alignment: left;
    }

    slider.master-slider .track {
        background-color: #0f1416;
        height: 5px;
        corner-radius: 3px;
    }

    slider.master-slider .range {
        background-color: #59b6d8;
        corner-radius: 3px;
    }

    slider.master-slider .thumb {
        background-color: #eef6f0;
        border-width: 1px;
        border-color: #59b6d8;
        corner-radius: 6px;
        width: 12px;
        height: 12px;
    }
"#;

/// Fallback chain sample rate, used only if the selected input device cannot be probed (in which
/// case starting the engine will fail anyway). Normally the chain is prepared at the device's actual
/// rate via [`chain_sample_rate`].
const CHAIN_SAMPLE_RATE_FALLBACK: f64 = 48_000.0;
const CHAIN_MAX_FRAMES: usize = 4096;

/// The sample rate to prepare the chain at — the selected input device's actual rate, which is the
/// rate the engine runs the stream at. Declared to plugins via `setupProcessing`; the host never
/// resamples.
fn chain_sample_rate(state: &HostUiState) -> f64 {
    state
        .selected_input
        .as_ref()
        .and_then(|device| device_sample_rate(device).ok())
        .map(|rate| rate as f64)
        .unwrap_or(CHAIN_SAMPLE_RATE_FALLBACK)
}

/// One chain row, bound into the list view.
#[derive(Clone, PartialEq)]
pub struct ChainRow {
    pub name: String,
    pub bypassed: bool,
}

/// One scanned-plugin row, bound into the catalog view.
#[derive(Clone, PartialEq)]
pub struct CatalogRow {
    pub catalog_index: usize,
    pub vendor: String,
    pub name: String,
    pub path: String,
    pub compatible: bool,
    /// Why it is incompatible (empty when compatible).
    pub detail: String,
}

/// One vendor group in the scanned-plugin catalog.
#[derive(Clone, PartialEq)]
pub struct CatalogGroup {
    pub vendor: String,
    pub rows: Vec<CatalogRow>,
}

/// One VST3 folder row, bound into the scan-root list.
#[derive(Clone, PartialEq)]
pub struct ScanFolderRow {
    pub name: String,
    pub path: String,
    pub system: bool,
    pub custom_index: Option<usize>,
}

/// The reactive signals the views bind to. `Signal` is `Copy`, so this is cheap to pass around.
#[derive(Clone, Copy)]
pub struct Signals {
    pub input_names: Signal<Vec<String>>,
    pub output_names: Signal<Vec<String>>,
    pub selected_input_index: Signal<Option<usize>>,
    pub selected_output_index: Signal<Option<usize>>,
    pub chain: Signal<Vec<ChainRow>>,
    pub catalog: Signal<Vec<CatalogRow>>,
    pub scan_folders: Signal<Vec<ScanFolderRow>>,
    pub running: Signal<bool>,
    pub input_left_level: Signal<f32>,
    pub input_right_level: Signal<f32>,
    pub output_left_level: Signal<f32>,
    pub output_right_level: Signal<f32>,
    pub master_gain_db: Signal<f32>,
    pub master_muted: Signal<bool>,
    pub status: Signal<String>,
    /// Whether the add-plugin browser overlay is open (pure UI state, not part of `HostUiState`).
    pub browser_open: Signal<bool>,
}

impl Signals {
    /// Create the signals (must run inside a Vizia reactive scope — the `Application` closure).
    pub fn new() -> Self {
        Signals {
            input_names: Signal::new(Vec::new()),
            output_names: Signal::new(Vec::new()),
            selected_input_index: Signal::new(None),
            selected_output_index: Signal::new(None),
            chain: Signal::new(Vec::new()),
            catalog: Signal::new(Vec::new()),
            scan_folders: Signal::new(Vec::new()),
            running: Signal::new(false),
            input_left_level: Signal::new(0.0),
            input_right_level: Signal::new(0.0),
            output_left_level: Signal::new(0.0),
            output_right_level: Signal::new(0.0),
            master_gain_db: Signal::new(0.0),
            master_muted: Signal::new(false),
            status: Signal::new("stopped".to_string()),
            browser_open: Signal::new(false),
        }
    }
}

/// UI events emitted by the views and the meter timer.
#[derive(Debug)]
pub enum AppEvent {
    SelectInput(usize),
    SelectOutput(usize),
    RemovePlugin(usize),
    MoveUp(usize),
    MoveDown(usize),
    ToggleBypass(usize),
    OpenEditor(usize),
    /// Add a custom VST3 scan folder.
    AddScanFolder,
    /// Remove a custom VST3 scan folder by index in `HostUiState.scan_dirs`.
    RemoveScanFolder(usize),
    /// Rescan system + custom VST3 folders and validate discovered plugins.
    RescanPlugins,
    /// Add a compatible plugin from the scanned catalog by index.
    AddFromCatalog(usize),
    SetMasterGain(f32),
    ToggleMasterMute,
    Start,
    Stop,
    Save,
    Load,
    /// Open the add-plugin browser overlay.
    OpenBrowser,
    /// Close the add-plugin browser overlay.
    CloseBrowser,
    /// Meter timer tick — pull the latest snapshot off the audio thread.
    Tick,
}

/// The controller: the live host runtime the Model drives. Owns the engine, the editor host, and a
/// **persistent pool of prepared plugin instances** (index-aligned with `HostUiState.chain`). The
/// instances live for the life of their slot; chain edits rebuild only the *ordering* over them
/// ([`ChainProcessor`] holds shared `Arc`s), so plugin state is preserved across reorder/bypass.
struct Runtime {
    host: ComPtr<IHostApplication>,
    engine: Option<AudioEngine>,
    editor: EditorHost,
    pool: Vec<PoolSlot>,
}

impl Runtime {
    fn new() -> Self {
        Runtime {
            host: HostContext::new()
                .to_com_ptr::<IHostApplication>()
                .expect("host exposes IHostApplication"),
            engine: None,
            editor: EditorHost::new(),
            pool: Vec::new(),
        }
    }

    fn is_running(&self) -> bool {
        self.engine.is_some()
    }

    /// Build a chain referencing the pool's (already-prepared) instances in the current order, with
    /// bypass from the UI state. No instantiation, no preparation — the instances are shared, so this
    /// never disturbs their state.
    fn build_chain(&self, state: &HostUiState) -> Box<ChainProcessor> {
        let instances = self.pool.iter().map(|slot| slot.instance.clone()).collect();
        let bypass: Vec<bool> = state.chain.iter().map(|slot| slot.bypassed).collect();
        Box::new(ChainProcessor::new(instances, bypass, CHAIN_MAX_FRAMES))
    }

    /// Prepare (set up + activate) every pooled instance at `sample_rate`. Safe to re-run (it
    /// quiesces first), and parameters survive, so this is also how a rate change re-prepares the
    /// pool. Used at Start.
    fn prepare_pool(&self, sample_rate: f64) -> Result<(), HostError> {
        let driver = ProcessDriver::new(sample_rate, CHAIN_MAX_FRAMES);
        for slot in &self.pool {
            driver.prepare(&slot.instance)?;
        }
        Ok(())
    }
}

/// The Vizia model: the neutral host state (source of truth), the reactive signals (view mirror),
/// and the live runtime (controller).
pub struct AppData {
    pub state: HostUiState,
    pub signals: Signals,
    runtime: Runtime,
    catalog_scan: Option<Receiver<PluginCatalog>>,
    first_tick_logged: bool,
    /// Meter ticks since launch; drives the low-frequency session autosave safety net.
    tick_count: u64,
}

impl AppData {
    pub fn new(state: HostUiState, signals: Signals) -> Self {
        diagnostics::log(format!(
            "ui: AppData::new start inputs={} outputs={} cached_catalog={}",
            state.inputs.len(),
            state.outputs.len(),
            state.catalog.entries.len()
        ));
        let mut model = AppData {
            state,
            signals,
            runtime: Runtime::new(),
            catalog_scan: None,
            first_tick_logged: false,
            tick_count: 0,
        };
        model.refresh_catalog();
        model.sync();
        diagnostics::log("ui: AppData::new done");
        model
    }

    /// Push the bindable parts of `state` into the signals.
    pub fn sync(&self) {
        // Idempotent: only push signals whose value actually changed. `sync` runs on every ~33 ms
        // meter tick, and `Signal::set` notifies unconditionally — without this guard the chain,
        // catalog, and folder `Binding`s would rebuild ~30×/second, recreating every button mid-click
        // (so clicks never complete and hover flickers). Normally only the meter levels change per
        // tick, so only the meter bars rebuild.
        set_if_changed(
            &self.signals.input_names,
            self.state.inputs.iter().map(|d| d.name.clone()).collect(),
        );
        set_if_changed(
            &self.signals.output_names,
            self.state.outputs.iter().map(|d| d.name.clone()).collect(),
        );
        set_if_changed(
            &self.signals.selected_input_index,
            selected_device_index(&self.state.inputs, &self.state.selected_input),
        );
        set_if_changed(
            &self.signals.selected_output_index,
            selected_device_index(&self.state.outputs, &self.state.selected_output),
        );
        set_if_changed(
            &self.signals.chain,
            self.state
                .chain
                .iter()
                .map(|slot| ChainRow {
                    name: slot.name.clone(),
                    bypassed: slot.bypassed,
                })
                .collect(),
        );
        let scan_roots = self.state.plugin_scan_roots();
        set_if_changed(
            &self.signals.catalog,
            self.state
                .catalog
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| CatalogRow {
                    catalog_index: index,
                    vendor: catalog_vendor(&entry.path, &scan_roots),
                    name: entry.name.clone(),
                    path: entry.path.display().to_string(),
                    compatible: entry.compatible,
                    detail: entry.reason.clone().unwrap_or_default(),
                })
                .collect(),
        );
        set_if_changed(&self.signals.scan_folders, scan_folder_rows(&self.state));
        set_if_changed(&self.signals.running, self.state.running);
        set_if_changed(
            &self.signals.input_left_level,
            self.state.meter.input_left_peak,
        );
        set_if_changed(
            &self.signals.input_right_level,
            self.state.meter.input_right_peak,
        );
        set_if_changed(
            &self.signals.output_left_level,
            self.state.meter.output_left_peak,
        );
        set_if_changed(
            &self.signals.output_right_level,
            self.state.meter.output_right_peak,
        );
        set_if_changed(&self.signals.master_gain_db, self.state.master_gain_db);
        set_if_changed(&self.signals.master_muted, self.state.master_muted);
        // A notice (last error/info) takes precedence in the status line; otherwise run state.
        let status = match &self.state.notice {
            Some(notice) => notice.clone(),
            None => if self.state.running {
                "audio live"
            } else {
                "audio off"
            }
            .to_string(),
        };
        set_if_changed(&self.signals.status, status);
    }

    // --- Controller commands (effectful) -------------------------------------------------------

    /// Log a failure and surface it as a UI notice.
    fn fail(&mut self, message: String) {
        diagnostics::log(format!("ui: fail {message}"));
        eprintln!("galad: {message}");
        self.state.set_notice(message);
    }

    fn persist_settings(&mut self) {
        diagnostics::log(format!(
            "ui: persist settings path={}",
            AppSettings::default_path().display()
        ));
        if let Err(error) = self.state.settings().save_default() {
            self.fail(format!("failed to save app settings: {error:?}"));
        }
    }

    fn master_settings(&self) -> MasterSettings {
        MasterSettings {
            gain_db: self.state.master_gain_db,
            muted: self.state.master_muted,
        }
    }

    /// If running, rebuild the ordering over the pooled instances and hand it to the audio thread.
    /// Gapless, and never disturbs plugin state (the instances are shared, not recreated).
    fn republish(&mut self) {
        if !self.runtime.is_running() {
            return;
        }
        let chain = self.runtime.build_chain(&self.state);
        if let Some(engine) = &self.runtime.engine {
            engine.publish_chain(chain);
        }
    }

    fn start_engine(&mut self) {
        if self.runtime.is_running() {
            return;
        }
        let (Some(input), Some(output)) = (
            self.state.selected_input.clone(),
            self.state.selected_output.clone(),
        ) else {
            self.fail("select an input and output device before starting".to_string());
            return;
        };
        // Prepare every pooled instance at the device's actual rate, then build the chain over them.
        if let Err(error) = self.runtime.prepare_pool(chain_sample_rate(&self.state)) {
            self.fail(format!("failed to prepare plugins: {error:?}"));
            return;
        }
        let chain = self.runtime.build_chain(&self.state);
        match AudioEngine::start_with_chain_and_master(input, output, chain, self.master_settings())
        {
            Ok(engine) => {
                self.runtime.engine = Some(engine);
                apply(&mut self.state, &UiCommand::Start);
                self.state.clear_notice();
            }
            Err(error) => self.fail(format!("failed to start engine: {error:?}")),
        }
    }

    /// Load a plugin into the pool: instantiate it and, if the engine is running, prepare it at the
    /// running rate so it can be republished live. (Stopped: `Start` prepares the whole pool.)
    fn load_into_pool(&mut self, path: &Path) -> Result<(), HostError> {
        let module = load_module(path)?;
        let instance = Arc::new(PluginInstance::from_factory(
            module.factory(),
            &self.runtime.host,
        )?);
        if self.runtime.is_running() {
            ProcessDriver::new(chain_sample_rate(&self.state), CHAIN_MAX_FRAMES)
                .prepare(&instance)?;
        }
        self.runtime.pool.push(PoolSlot { module, instance });
        Ok(())
    }

    fn stop_engine(&mut self) {
        if let Some(mut engine) = self.runtime.engine.take() {
            engine.stop();
        }
        apply(&mut self.state, &UiCommand::Stop);
        self.state.set_meter(MeterSnapshot::default());
        self.state.clear_notice();
    }

    /// Add a custom VST3 scan folder, then refresh the available plugin browser.
    fn add_scan_folder(&mut self) {
        let Some(dir) = pick_folder() else {
            return;
        };
        self.state.record_scan_dir(dir);
        self.refresh_catalog();
    }

    fn remove_scan_folder(&mut self, index: usize) {
        self.state.remove_scan_dir(index);
        self.refresh_catalog();
    }

    /// Start a background scan of the Windows system VST3 folders plus configured custom folders.
    /// The previous cached catalog remains visible until the scan completes.
    fn refresh_catalog(&mut self) {
        let roots = self.state.plugin_scan_roots();
        diagnostics::log(format!("ui: refresh_catalog spawn roots={roots:?}"));
        let (tx, rx) = channel();
        thread::spawn(move || {
            diagnostics::log("catalog-scan: thread start");
            let paths = HostUiState::scan_roots(&roots);
            diagnostics::log(format!("catalog-scan: found {} vst3 paths", paths.len()));
            let probes = paths
                .into_iter()
                .map(|path| {
                    diagnostics::log(format!("catalog-scan: validate {}", path.display()));
                    let result = validate_plugin(&path).map_err(|error| format!("{error:?}"));
                    (path, result)
                })
                .collect();
            diagnostics::log("catalog-scan: sending result");
            let _ = tx.send(PluginCatalog::from_probes(probes));
            diagnostics::log("catalog-scan: thread done");
        });
        self.catalog_scan = Some(rx);
        self.state.set_notice("scanning VST3 folders...");
    }

    fn poll_catalog_scan(&mut self) {
        let Some(receiver) = &self.catalog_scan else {
            return;
        };
        match receiver.try_recv() {
            Ok(catalog) => {
                self.state.set_catalog(catalog);
                let compatible = self.state.catalog.compatible().count();
                let total = self.state.catalog.entries.len();
                self.state.set_notice(format!(
                    "found {total} VST3 plugin(s), {compatible} compatible"
                ));
                diagnostics::log(format!(
                    "ui: catalog scan complete total={total} compatible={compatible}"
                ));
                self.catalog_scan = None;
                self.persist_settings();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.catalog_scan = None;
                diagnostics::log("ui: catalog scan disconnected");
                self.state.set_notice("VST3 scan failed");
            }
        }
    }

    /// Add a compatible plugin from the scanned catalog (already validated during the scan).
    fn add_from_catalog(&mut self, index: usize) {
        let Some(entry) = self.state.catalog.entries.get(index) else {
            return;
        };
        if !entry.compatible {
            return;
        }
        let path = entry.path.clone();
        if let Err(error) = validate_plugin(&path) {
            self.fail(format!("rejecting {}: {error:?}", path.display()));
            return;
        }
        match self.load_into_pool(&path) {
            Ok(()) => {
                apply(&mut self.state, &UiCommand::AddPlugin(path));
                self.state.clear_notice();
                self.republish();
            }
            Err(error) => self.fail(format!("failed to load {}: {error:?}", path.display())),
        }
    }

    fn remove_plugin(&mut self, index: usize) {
        diagnostics::log(format!(
            "ui: remove_plugin begin index={index} pool_len={} running={}",
            self.runtime.pool.len(),
            self.runtime.is_running()
        ));
        // Drop the pool slot (one `Arc`); any live/retired chain still referencing the instance keeps
        // it alive until reclaimed, so its teardown stays off the audio thread.
        if index < self.runtime.pool.len() {
            diagnostics::log("ui: remove_plugin drop pool slot");
            self.runtime.pool.remove(index);
            diagnostics::log("ui: remove_plugin pool slot dropped");
        }
        apply(&mut self.state, &UiCommand::RemovePlugin(index));
        diagnostics::log("ui: remove_plugin republish");
        self.republish();
        diagnostics::log("ui: remove_plugin done");
    }

    fn reorder(&mut self, index: usize, dir: Dir) {
        // Mirror `HostUiState::move_slot`'s bounds so `pool` stays index-aligned with `chain`. The
        // instances are not recreated — only their order changes — so plugin state is preserved.
        let len = self.runtime.pool.len();
        let target = match dir {
            Dir::Up if index > 0 => Some(index - 1),
            Dir::Down if index + 1 < len => Some(index + 1),
            _ => None,
        };
        if let Some(target) = target {
            self.runtime.pool.swap(index, target);
        }
        apply(&mut self.state, &UiCommand::MoveSlot(index, dir));
        self.republish();
    }

    fn toggle_bypass(&mut self, index: usize) {
        apply(&mut self.state, &UiCommand::ToggleBypass(index));
        self.republish();
    }

    fn open_editor(&mut self, index: usize) {
        let Some(slot) = self.state.chain.get(index) else {
            return;
        };
        let path = slot.path.clone();
        if let Err(error) = self.runtime.editor.open(&path) {
            self.fail(format!(
                "failed to open editor for {}: {error:?}",
                path.display()
            ));
        }
    }

    /// Capture the current host state as a [`HostSession`], reading each plugin's opaque state straight
    /// from the pool (a UI-thread `getState`, valid whether stopped or running — gapless, no restart).
    fn capture_current_session(&self) -> HostSession {
        let slots: Vec<SessionSlot> = self
            .state
            .chain
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                self.runtime.pool.get(index).map(|pooled| SessionSlot {
                    plugin_path: slot.path.clone(),
                    component: pooled.instance.component(),
                    bypassed: slot.bypassed,
                })
            })
            .collect();
        capture_session(
            &slots,
            self.state.selected_input.clone(),
            self.state.selected_output.clone(),
            self.state.settings(),
        )
    }

    /// Restore a session into the UI state and rebuild the pool with each plugin's state restored.
    /// Shared by the manual `Load` and the launch-time auto-restore. Never starts audio. On a restore
    /// failure (e.g. a plugin is no longer installed) the chain and pool are cleared together so they
    /// stay index-aligned.
    fn apply_session(&mut self, session: &HostSession) {
        if let Some(mut engine) = self.runtime.engine.take() {
            engine.stop();
        }
        self.state.load_session(session);
        self.state.clear_notice();
        self.persist_settings();
        match restore_pool(session, &self.runtime.host) {
            Ok(pool) => self.runtime.pool = pool,
            Err(error) => {
                self.fail(format!("failed to restore session: {error:?}"));
                self.runtime.pool.clear();
                self.state.chain.clear();
            }
        }
        self.state.running = false;
        self.state.set_meter(MeterSnapshot::default());
    }

    /// Auto-save the current session to Galad's default "last session" path (devices, chain, per-plugin
    /// opaque state, master gain/mute). Best-effort and quiet — failures are logged, not surfaced.
    fn autosave_session(&self) {
        let session = self.capture_current_session();
        match session.save(HostSession::default_path()) {
            Ok(()) => diagnostics::log("ui: autosave session ok"),
            Err(error) => diagnostics::log(format!("ui: autosave session failed: {error:?}")),
        }
    }

    /// Restore the default "last session" over the launch defaults, if one exists. Falls back to the
    /// pre-selected default devices when the session did not name one. Never starts audio.
    fn restore_default_session(&mut self) {
        match HostSession::load(HostSession::default_path()) {
            Ok(session) => {
                diagnostics::log(format!(
                    "ui: restoring default session chain={} input={:?} output={:?}",
                    session.chain.len(),
                    session.input.as_ref().map(|d| &d.name),
                    session.output.as_ref().map(|d| &d.name),
                ));
                let fallback_input = self.state.selected_input.clone();
                let fallback_output = self.state.selected_output.clone();
                self.apply_session(&session);
                // Keep the system-default device pre-selection if the session didn't name one.
                if self.state.selected_input.is_none() {
                    self.state.selected_input = fallback_input;
                }
                if self.state.selected_output.is_none() {
                    self.state.selected_output = fallback_output;
                }
            }
            Err(error) => diagnostics::log(format!("ui: no default session restored: {error:?}")),
        }
    }

    fn save_session(&mut self) {
        let Some(path) = save_session_path() else {
            return;
        };
        if let Err(error) = self.capture_current_session().save(&path) {
            self.fail(format!("failed to save session: {error:?}"));
        }
    }

    fn load_session(&mut self) {
        let Some(path) = open_session_path() else {
            return;
        };
        match HostSession::load(&path) {
            Ok(session) => self.apply_session(&session),
            Err(error) => self.fail(format!("failed to load {}: {error:?}", path.display())),
        }
    }

    fn set_master_gain(&mut self, gain_db: f32) {
        self.state.set_master_gain_db(gain_db);
        if let Some(engine) = &self.runtime.engine {
            engine.set_master(self.master_settings());
        }
        self.persist_settings();
    }

    fn toggle_master_mute(&mut self) {
        self.state.set_master_muted(!self.state.master_muted);
        if let Some(engine) = &self.runtime.engine {
            engine.set_master(self.master_settings());
        }
        self.persist_settings();
    }

    fn tick(&mut self) {
        if !self.first_tick_logged {
            self.first_tick_logged = true;
            diagnostics::log("ui: first timer tick");
        }
        self.tick_count = self.tick_count.wrapping_add(1);
        // Low-frequency safety-net autosave (~every 30 s) to capture plugin-editor-only changes that
        // Galad cannot observe; structural edits and window close autosave immediately.
        if self.tick_count % 900 == 0 && !self.runtime.pool.is_empty() {
            self.autosave_session();
        }
        self.poll_catalog_scan();
        let Some(engine) = &self.runtime.engine else {
            return;
        };
        // `status`/`meter` are `Copy`, so the engine borrow ends before we mutate `self`.
        let status = engine.status();
        let meter = engine.meter_reader().read();
        self.state.set_meter(meter);
        // On a runtime fault (e.g. the device was unplugged) stop + notify (M7 Step 7), then drop the
        // already-exited engine. The realtime thread has ended, so do not call `stop` again.
        if self.state.react_to_engine_status(status) {
            self.runtime.engine = None;
        }
    }
}

impl Model for AppData {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        // Auto-save the session when the window is closing (captures plugin-editor changes Galad does
        // not otherwise observe). Audio on/off is intentionally not persisted.
        event.map(|window_event, _| {
            if matches!(window_event, WindowEvent::WindowClose) {
                diagnostics::log("ui: window close — autosaving session");
                self.autosave_session();
            }
        });
        event.map(|app_event, _| {
            // Log every interactive event (not the meter tick) so the diagnostic log shows whether a
            // click actually reaches the controller.
            if !matches!(app_event, AppEvent::Tick) {
                diagnostics::log(format!("ui: event {app_event:?}"));
            }
            // Structural changes are auto-saved immediately (cheap, infrequent). Master gain is left to
            // the AppSettings persistence + the close/periodic autosave to avoid per-drag churn.
            let persist = matches!(
                app_event,
                AppEvent::SelectInput(_)
                    | AppEvent::SelectOutput(_)
                    | AppEvent::AddFromCatalog(_)
                    | AppEvent::RemovePlugin(_)
                    | AppEvent::MoveUp(_)
                    | AppEvent::MoveDown(_)
                    | AppEvent::ToggleBypass(_)
                    | AppEvent::ToggleMasterMute
            );
            match app_event {
                AppEvent::SelectInput(index) => {
                    if let Some(device) = self.state.inputs.get(*index).cloned() {
                        apply(&mut self.state, &UiCommand::SelectInput(device));
                    }
                }
                AppEvent::SelectOutput(index) => {
                    if let Some(device) = self.state.outputs.get(*index).cloned() {
                        apply(&mut self.state, &UiCommand::SelectOutput(device));
                    }
                }
                AppEvent::RemovePlugin(index) => self.remove_plugin(*index),
                AppEvent::MoveUp(index) => self.reorder(*index, Dir::Up),
                AppEvent::MoveDown(index) => self.reorder(*index, Dir::Down),
                AppEvent::ToggleBypass(index) => self.toggle_bypass(*index),
                AppEvent::OpenEditor(index) => self.open_editor(*index),
                AppEvent::AddScanFolder => self.add_scan_folder(),
                AppEvent::RemoveScanFolder(index) => self.remove_scan_folder(*index),
                AppEvent::RescanPlugins => self.refresh_catalog(),
                AppEvent::AddFromCatalog(index) => {
                    self.add_from_catalog(*index);
                    // Adding from the browser dismisses the overlay.
                    self.signals.browser_open.set(false);
                }
                AppEvent::SetMasterGain(gain_db) => self.set_master_gain(*gain_db),
                AppEvent::ToggleMasterMute => self.toggle_master_mute(),
                AppEvent::Start => self.start_engine(),
                AppEvent::Stop => self.stop_engine(),
                AppEvent::Save => self.save_session(),
                AppEvent::Load => self.load_session(),
                AppEvent::OpenBrowser => {
                    // Refresh the catalog when the browser opens so it reflects the current folders.
                    self.refresh_catalog();
                    self.signals.browser_open.set(true);
                }
                AppEvent::CloseBrowser => self.signals.browser_open.set(false),
                AppEvent::Tick => self.tick(),
            }
            self.sync();
            if persist {
                self.autosave_session();
            }
        });
    }
}

/// Launch the standalone Galad host window. Blocks until the window is closed.
pub fn run() {
    diagnostics::log("ui: run enter");
    let idle_count = Arc::new(AtomicUsize::new(0));
    let idle_count_for_callback = idle_count.clone();
    diagnostics::log("ui: Application::new begin");
    let result = vizia::Application::new(|cx| {
        diagnostics::log("ui: application closure start");
        let _ = cx.add_stylesheet(STYLE);
        diagnostics::log("ui: stylesheet added");

        diagnostics::log("ui: enumerate inputs begin");
        let inputs = enumerate(AudioDirection::Input).unwrap_or_default();
        diagnostics::log(format!("ui: enumerate inputs done count={}", inputs.len()));
        diagnostics::log("ui: enumerate outputs begin");
        let outputs = enumerate(AudioDirection::Output).unwrap_or_default();
        diagnostics::log(format!(
            "ui: enumerate outputs done count={}",
            outputs.len()
        ));
        let mut state = HostUiState::default();
        state.set_devices(inputs, outputs);
        diagnostics::log(format!(
            "ui: load settings begin path={}",
            AppSettings::default_path().display()
        ));
        match AppSettings::load_default() {
            Ok(settings) => {
                diagnostics::log(format!(
                    "ui: load settings done folders={} cached_catalog={}",
                    settings.plugin_scan_dirs.len(),
                    settings.plugin_catalog.len()
                ));
                state.apply_settings(&settings);
            }
            Err(error) => {
                diagnostics::log(format!("ui: load settings error {error:?}"));
                state.set_notice(format!("failed to load app settings: {error:?}"));
            }
        }
        // Pre-select the system default input/output so the host is usable immediately; the user can
        // change either from the pickers.
        diagnostics::log("ui: default input begin");
        match default_device(AudioDirection::Input) {
            Ok(device) => {
                diagnostics::log(format!("ui: default input selected {}", device.name));
                state.select_input(device);
            }
            Err(error) => diagnostics::log(format!("ui: default input error {error:?}")),
        }
        diagnostics::log("ui: default output begin");
        match default_device(AudioDirection::Output) {
            Ok(device) => {
                diagnostics::log(format!("ui: default output selected {}", device.name));
                state.select_output(device);
            }
            Err(error) => diagnostics::log(format!("ui: default output error {error:?}")),
        }

        let signals = Signals::new();
        diagnostics::log("ui: AppData build begin");
        let mut model = AppData::new(state, signals);
        // Restore the last auto-saved session (devices, chain + per-plugin state, master/mute) over the
        // launch defaults. Audio stays stopped until the user presses Start.
        model.restore_default_session();
        model.sync();
        model.build(cx);
        diagnostics::log("ui: AppData build done");

        // Drive the meters off the audio thread: tick ~30 Hz, pulling the latest seqlock snapshot.
        diagnostics::log("ui: timer create begin");
        let meter_timer = cx.add_timer(Duration::from_millis(33), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(AppEvent::Tick);
            }
        });
        cx.start_timer(meter_timer);
        diagnostics::log("ui: timer started");

        diagnostics::log("ui: build_ui begin");
        build_ui(cx, signals);
        diagnostics::log("ui: build_ui done");
    })
    .on_idle(move |cx| {
        let idle_index = idle_count_for_callback.fetch_add(1, Ordering::Relaxed);
        if idle_index == 0 {
            diagnostics::log("ui: first on_idle");
            diagnostics::log_window_probe("first-on-idle-before-redraw");
            cx.needs_redraw(Entity::root());
            diagnostics::log("ui: first on_idle requested redraw");
        } else if idle_index == 1 {
            diagnostics::log("ui: second on_idle");
            diagnostics::log_window_probe("second-on-idle");
        }
    })
    .title("Galad")
    .inner_size((1120u32, 720u32))
    .min_inner_size(Some((900u32, 600u32)));

    diagnostics::log("ui: Application::new done; run begin");
    diagnostics::log("ui: spawn window probe");
    diagnostics::spawn_window_probe("ui-run");
    if let Err(error) = result.run() {
        eprintln!("galad: UI error: {error:?}");
        diagnostics::log(format!("ui: run error {error:?}"));
    }
    diagnostics::log("ui: run exit");
}

/// Build the host UI view tree. The window is one channel: a top strip (brand, run state, devices,
/// session) over a full-width SIGNAL CHAIN (the hero), over a master/transport bar. The plugin
/// browser is an on-demand overlay opened from the chain's "Add plugin" slot — not a permanent panel
/// (mirrors hot-mic, where the browser is its own window). The overlay is the top `ZStack` child,
/// shown only while `browser_open` is set.
pub fn build_ui(cx: &mut Context, signals: Signals) {
    ZStack::new(cx, move |cx| {
        main_view(cx, signals);
        // The overlay is always built as a full-size top layer; it is shown/hidden via `display`
        // (toggled by `browser_open`). Building it conditionally inside a `Binding` collapses it to a
        // zero-size node, so the modal would never appear — hence the always-present layer.
        browser_overlay(cx, signals);
    })
    .class("root")
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

/// The main (non-overlay) channel view.
fn main_view(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        top_strip(cx, signals);
        chain_panel(cx, signals);
        master_bar(cx, signals);
    })
    .padding(Pixels(10.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

/// A bordered icon button wrapping a Tabler `Svg`. Callers chain `.on_press`, sizing, and extra
/// classes onto the returned handle.
fn icon_button<'a>(cx: &'a mut Context, icon: &'static str) -> Handle<'a, Button> {
    Button::new(cx, move |cx| Svg::new(cx, icon).class("btn-icon")).class("icon-btn")
}

/// A full-width 1px divider.
fn divider(cx: &mut Context) {
    Element::new(cx)
        .class("divider")
        .width(Stretch(1.0))
        .height(Pixels(1.0));
}

/// A 1px vertical divider for the master bar.
fn v_divider(cx: &mut Context) {
    Element::new(cx)
        .class("v-divider")
        .width(Pixels(1.0))
        .height(Pixels(56.0));
}

/// A section header: a colored accent bar, a title, a reactive sub-line, and optional trailing
/// controls (built by `trailing`).
fn section_header<D, F>(
    cx: &mut Context,
    title: &'static str,
    detail: D,
    accent: &'static str,
    trailing: F,
) where
    D: Res<String> + Clone + 'static,
    F: FnOnce(&mut Context),
{
    HStack::new(cx, move |cx| {
        Element::new(cx).class("accent-bar").class(accent);
        VStack::new(cx, move |cx| {
            Label::new(cx, title).class("section-title");
            Label::new(cx, detail.clone())
                .class("section-sub")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(1.0));
        trailing(cx);
    })
    .height(Pixels(34.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

/// Top strip: brand, live/idle chip, In/Out device pickers, the status line, and session load/save.
fn top_strip(cx: &mut Context, signals: Signals) {
    HStack::new(cx, move |cx| {
        Element::new(cx).class("brand-mark");
        Label::new(cx, "Galad").class("wordmark");
        Label::new(
            cx,
            Memo::new(move |_| {
                if signals.running.get() {
                    "LIVE".to_string()
                } else {
                    "IDLE".to_string()
                }
            }),
        )
        .class("run-chip")
        .toggle_class("is-live", signals.running)
        .height(Pixels(22.0));

        device_pick(
            cx,
            "In",
            "Input",
            signals.input_names,
            signals.selected_input_index,
            true,
        );
        device_pick(
            cx,
            "Out",
            "Output",
            signals.output_names,
            signals.selected_output_index,
            false,
        );

        Spacer::new(cx);

        Label::new(cx, signals.status)
            .class("status-text")
            .width(Stretch(1.0))
            .min_width(Pixels(0.0));

        Button::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_FOLDER).class("btn-icon");
                Label::new(cx, "Open");
            })
            .width(Auto)
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(6.0))
        })
        .on_press(|cx| cx.emit(AppEvent::Load))
        .class("tool-button")
        .width(Pixels(80.0))
        .height(Pixels(30.0));
        Button::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_DEVICE_FLOPPY).class("btn-icon");
                Label::new(cx, "Save");
            })
            .width(Auto)
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(6.0))
        })
        .on_press(|cx| cx.emit(AppEvent::Save))
        .class("tool-button")
        .width(Pixels(80.0))
        .height(Pixels(30.0));
    })
    .class("top-strip")
    .height(Pixels(46.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(10.0));
}

/// One compact device picker for the top strip: a short label and a dropdown.
fn device_pick(
    cx: &mut Context,
    label: &'static str,
    placeholder: &'static str,
    names: Signal<Vec<String>>,
    selected: Signal<Option<usize>>,
    input: bool,
) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("field-label");
        Select::new(cx, names, selected, true)
            .placeholder(placeholder)
            .on_select(move |cx, index| {
                if input {
                    cx.emit(AppEvent::SelectInput(index));
                } else {
                    cx.emit(AppEvent::SelectOutput(index));
                }
            })
            .class("device-select")
            .width(Pixels(150.0));
    })
    .width(Auto)
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

/// The hero: the full-width signal chain. A vertical list of insert slots in signal order, ending in
/// an "Add plugin" slot that opens the browser overlay.
fn chain_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        section_header(
            cx,
            "SIGNAL CHAIN",
            Memo::new(move |_| count_text(signals.chain.get().len(), "plugin")),
            "accent-tone",
            |_| {},
        );
        ScrollView::new(cx, move |cx| {
            let chain = signals.chain;
            Binding::new(cx, chain, move |cx| {
                let rows = chain.get();
                VStack::new(cx, move |cx| {
                    if rows.is_empty() {
                        Label::new(
                            cx,
                            "Your signal chain is empty — add a plugin to start processing.",
                        )
                        .class("muted");
                    } else {
                        for (index, row) in rows.into_iter().enumerate() {
                            insert_slot(cx, index, row);
                        }
                    }
                    add_slot(cx);
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(6.0));
            });
        })
        .class("v-scroll")
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .class("panel")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .padding(Pixels(14.0))
    .vertical_gap(Pixels(10.0));
}

/// One insert slot, full width: power/bypass, index, plugin name, editor, reorder, remove.
fn insert_slot(cx: &mut Context, index: usize, row: ChainRow) {
    let bypassed = row.bypassed;
    let name = row.name;
    HStack::new(cx, move |cx| {
        icon_button(cx, ICON_POWER)
            .class("slot-power")
            .toggle_class("is-bypassed", bypassed)
            .width(Pixels(28.0))
            .height(Pixels(28.0))
            .on_press(move |cx| cx.emit(AppEvent::ToggleBypass(index)));
        Label::new(cx, format!("{}", index + 1))
            .class("slot-index")
            .width(Pixels(20.0));
        Label::new(cx, name.clone())
            .class("slot-name")
            .width(Stretch(1.0))
            .min_width(Pixels(0.0));
        icon_button(cx, ICON_ADJUSTMENTS)
            .width(Pixels(28.0))
            .height(Pixels(28.0))
            .on_press(move |cx| cx.emit(AppEvent::OpenEditor(index)));
        icon_button(cx, ICON_CHEVRON_UP)
            .width(Pixels(26.0))
            .height(Pixels(28.0))
            .on_press(move |cx| cx.emit(AppEvent::MoveUp(index)));
        icon_button(cx, ICON_CHEVRON_DOWN)
            .width(Pixels(26.0))
            .height(Pixels(28.0))
            .on_press(move |cx| cx.emit(AppEvent::MoveDown(index)));
        icon_button(cx, ICON_TRASH)
            .class("danger")
            .width(Pixels(28.0))
            .height(Pixels(28.0))
            .on_press(move |cx| cx.emit(AppEvent::RemovePlugin(index)));
    })
    .class("slot")
    .toggle_class("is-bypassed", bypassed)
    .height(Pixels(44.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

/// The trailing "Add plugin" slot — opens the browser overlay.
fn add_slot(cx: &mut Context) {
    Button::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Svg::new(cx, ICON_PLUS).class("btn-icon");
            Label::new(cx, "Add plugin").class("add-slot-label");
        })
        .width(Auto)
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0))
    })
    .on_press(|cx| cx.emit(AppEvent::OpenBrowser))
    .class("add-slot")
    .width(Stretch(1.0))
    .height(Pixels(42.0));
}

/// Bottom master/transport bar: start/stop, IN + OUT meters, master fader + mute.
fn master_bar(cx: &mut Context, signals: Signals) {
    HStack::new(cx, move |cx| {
        transport_button(cx, signals);
        v_divider(cx);
        meter_group(
            cx,
            "IN",
            signals.input_left_level,
            signals.input_right_level,
        );
        meter_group(
            cx,
            "OUT",
            signals.output_left_level,
            signals.output_right_level,
        );
        v_divider(cx);
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, "MASTER")
                    .class("field-label")
                    .width(Stretch(1.0))
                    .min_width(Pixels(0.0));
                Label::new(
                    cx,
                    Memo::new(move |_| gain_text(signals.master_gain_db.get())),
                )
                .class("value-strong");
            })
            .width(Stretch(1.0))
            .alignment(Alignment::Center);
            HStack::new(cx, move |cx| {
                Svg::new(cx, ICON_VOLUME).class("inline-icon");
                Slider::new(cx, signals.master_gain_db)
                    .range(MASTER_GAIN_MIN_DB..MASTER_GAIN_MAX_DB)
                    .step(0.5f32)
                    .on_change(|cx, gain| cx.emit(AppEvent::SetMasterGain(gain)))
                    .class("master-slider")
                    .width(Stretch(1.0));
                icon_button(cx, ICON_VOLUME)
                    .class("mute-btn")
                    .toggle_class("mute-on", signals.master_muted)
                    .width(Pixels(36.0))
                    .height(Pixels(30.0))
                    .on_press(|cx| cx.emit(AppEvent::ToggleMasterMute));
            })
            .width(Stretch(1.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(260.0))
        .height(Auto)
        .vertical_gap(Pixels(6.0));
    })
    .class("panel")
    .height(Pixels(96.0))
    .width(Stretch(1.0))
    .padding(Pixels(12.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(14.0));
}

/// The large start/stop transport button.
fn transport_button(cx: &mut Context, signals: Signals) {
    let running = signals.running;
    Button::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Binding::new(cx, running, move |cx| {
                Svg::new(
                    cx,
                    if running.get() {
                        ICON_PLAYER_STOP
                    } else {
                        ICON_PLAYER_PLAY
                    },
                )
                .class("btn-icon");
            });
            Label::new(
                cx,
                Memo::new(move |_| {
                    if running.get() {
                        "Stop".to_string()
                    } else {
                        "Start".to_string()
                    }
                }),
            );
        })
        .width(Auto)
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0))
    })
    .on_press(move |cx| {
        if running.get() {
            cx.emit(AppEvent::Stop);
        } else {
            cx.emit(AppEvent::Start);
        }
    })
    .class("transport-btn")
    .toggle_class("is-stop", signals.running)
    .width(Pixels(104.0))
    .height(Pixels(64.0));
}

/// A labelled L/R meter pair (IN or OUT).
fn meter_group(cx: &mut Context, title: &'static str, left: Signal<f32>, right: Signal<f32>) {
    VStack::new(cx, move |cx| {
        Label::new(cx, title).class("field-label");
        meter(cx, "L", left);
        meter(cx, "R", right);
    })
    .width(Pixels(128.0))
    .height(Auto)
    .vertical_gap(Pixels(3.0));
}

/// One meter channel: a label, a fill bar (hot near 0 dBFS), and a dB readout.
fn meter(cx: &mut Context, label: &'static str, level: Signal<f32>) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("meter-label")
            .width(Pixels(10.0));
        HStack::new(cx, move |cx| {
            Binding::new(cx, level, move |cx| {
                let value = level.get();
                let width = meter_width_percent(value);
                let hot = value >= 0.99;
                Element::new(cx)
                    .class("meter-fill")
                    .toggle_class("is-hot", hot)
                    .width(Percentage(width))
                    .height(Stretch(1.0));
            });
            Spacer::new(cx);
        })
        .class("meter-track")
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .height(Pixels(7.0));
        Label::new(cx, Memo::new(move |_| level_text(level.get())))
            .class("meter-label")
            .width(Pixels(50.0));
    })
    .width(Stretch(1.0))
    .height(Pixels(16.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

/// The add-plugin browser overlay: a dimmed backdrop (click to dismiss) under a centered modal with
/// the vendor-grouped catalog and a scan-folders footer.
fn browser_overlay(cx: &mut Context, signals: Signals) {
    ZStack::new(cx, move |cx| {
        Button::new(cx, |cx| Element::new(cx))
            .on_press(|cx| cx.emit(AppEvent::CloseBrowser))
            .class("backdrop")
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        modal_panel(cx, signals);
    })
    .class("overlay-layer")
    .toggle_class("is-open", signals.browser_open)
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .alignment(Alignment::Center);
}

/// The centered browser modal.
fn modal_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx).class("accent-bar").class("accent-tone");
            Label::new(cx, "Add Plugin")
                .class("modal-title")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(
                cx,
                Memo::new(move |_| count_text(signals.catalog.get().len(), "found")),
            )
            .class("section-sub");
            icon_button(cx, ICON_REFRESH)
                .width(Pixels(28.0))
                .height(Pixels(26.0))
                .on_press(|cx| cx.emit(AppEvent::RescanPlugins));
            icon_button(cx, ICON_X)
                .width(Pixels(28.0))
                .height(Pixels(26.0))
                .on_press(|cx| cx.emit(AppEvent::CloseBrowser));
        })
        .height(Pixels(30.0))
        .width(Stretch(1.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));

        divider(cx);

        ScrollView::new(cx, move |cx| {
            let catalog = signals.catalog;
            Binding::new(cx, catalog, move |cx| {
                let rows = catalog.get();
                if rows.is_empty() {
                    Label::new(
                        cx,
                        "No plugins found. Add a scan folder below, then Rescan.",
                    )
                    .class("muted");
                    return;
                }
                VStack::new(cx, move |cx| {
                    for group in catalog_groups(rows) {
                        Label::new(cx, group.vendor)
                            .class("vendor-header")
                            .width(Stretch(1.0))
                            .min_width(Pixels(0.0));
                        for row in group.rows {
                            catalog_row(cx, row);
                        }
                    }
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(3.0));
            });
        })
        .class("v-scroll")
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .width(Stretch(1.0))
        .height(Stretch(1.0));

        divider(cx);

        HStack::new(cx, move |cx| {
            Element::new(cx).class("accent-bar").class("accent-warn");
            Label::new(cx, "SCAN FOLDERS")
                .class("section-title")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(
                cx,
                Memo::new(move |_| count_text(signals.scan_folders.get().len(), "folder")),
            )
            .class("section-sub");
            icon_button(cx, ICON_FOLDER_PLUS)
                .width(Pixels(28.0))
                .height(Pixels(26.0))
                .on_press(|cx| cx.emit(AppEvent::AddScanFolder));
        })
        .height(Pixels(28.0))
        .width(Stretch(1.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));

        ScrollView::new(cx, move |cx| {
            let folders = signals.scan_folders;
            Binding::new(cx, folders, move |cx| {
                let rows = folders.get();
                if rows.is_empty() {
                    Label::new(cx, "No folders").class("muted");
                    return;
                }
                VStack::new(cx, move |cx| {
                    for row in rows {
                        folder_row(cx, row);
                    }
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(4.0));
            });
        })
        .class("v-scroll")
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .width(Stretch(1.0))
        .height(Pixels(110.0));
    })
    .class("modal")
    .width(Pixels(560.0))
    .height(Percentage(82.0))
    .padding(Pixels(14.0))
    .vertical_gap(Pixels(10.0));
}

/// One scanned plugin: status dot, name, and an Add button — or the failure reason if incompatible.
/// Dense single-line rows; the file path is intentionally omitted (the vendor group conveys it).
fn catalog_row(cx: &mut Context, row: CatalogRow) {
    let index = row.catalog_index;
    let compatible = row.compatible;
    let name = row.name;
    let reason = if compatible {
        String::new()
    } else if row.detail.is_empty() {
        "incompatible".to_string()
    } else {
        row.detail
    };
    HStack::new(cx, move |cx| {
        Element::new(cx)
            .class("dot")
            .toggle_class("is-ok", compatible)
            .toggle_class("is-error", !compatible);
        Label::new(cx, name.clone())
            .class("row-name")
            .width(Stretch(1.0))
            .min_width(Pixels(0.0));
        if compatible {
            icon_button(cx, ICON_PLUS)
                .class("add")
                .width(Pixels(26.0))
                .height(Pixels(24.0))
                .on_press(move |cx| cx.emit(AppEvent::AddFromCatalog(index)));
        } else {
            Label::new(cx, reason.clone())
                .class("row-reason")
                .width(Auto)
                .min_width(Pixels(0.0));
        }
    })
    .class("browse-row")
    .toggle_class("is-compatible", compatible)
    .height(Pixels(28.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}

/// One scan-folder row: folder icon, name + path (clipped), a SYS/USR chip, and remove for custom.
fn folder_row(cx: &mut Context, row: ScanFolderRow) {
    let system = row.system;
    let custom_index = row.custom_index;
    let path = row.path;
    let name = row.name;
    HStack::new(cx, move |cx| {
        Svg::new(cx, ICON_FOLDER).class("inline-icon-sm");
        VStack::new(cx, move |cx| {
            Label::new(cx, name.clone())
                .class("row-name")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(cx, path.clone())
                .class("row-path")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(0.0));
        Label::new(cx, if system { "SYS" } else { "USR" }).class("mini-chip");
        if let Some(custom_index) = custom_index {
            icon_button(cx, ICON_TRASH)
                .class("danger")
                .width(Pixels(24.0))
                .height(Pixels(24.0))
                .on_press(move |cx| cx.emit(AppEvent::RemoveScanFolder(custom_index)));
        }
    })
    .class("folder-row")
    .height(Pixels(38.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}

/// Set a signal only when its value actually changed, so a no-op `sync` does not notify (and rebuild)
/// the views bound to it. Keeps the per-tick meter `sync` from churning the whole tree.
fn set_if_changed<T: Clone + PartialEq + 'static>(signal: &Signal<T>, value: T) {
    if signal.get() != value {
        signal.set(value);
    }
}

fn count_text(count: usize, noun: &str) -> String {
    let suffix = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{suffix}")
}

fn meter_width_percent(level: f32) -> f32 {
    level.clamp(0.0, 1.0).sqrt() * 100.0
}

fn level_text(level: f32) -> String {
    let level = level.clamp(0.0, 1.0);
    if level <= 0.000_001 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * level.log10())
    }
}

fn gain_text(gain_db: f32) -> String {
    format!("{gain_db:.1} dB")
}

fn scan_folder_rows(state: &HostUiState) -> Vec<ScanFolderRow> {
    let mut rows: Vec<ScanFolderRow> = HostUiState::system_vst3_dirs()
        .into_iter()
        .map(|path| scan_folder_row(path, true, None))
        .collect();
    rows.extend(
        state
            .scan_dirs
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, path)| scan_folder_row(path, false, Some(index))),
    );
    rows
}

fn scan_folder_row(path: PathBuf, system: bool, custom_index: Option<usize>) -> ScanFolderRow {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(if system { "System VST3" } else { "VST3 folder" })
        .to_string();
    ScanFolderRow {
        name,
        path: path.display().to_string(),
        system,
        custom_index,
    }
}

fn catalog_groups(rows: Vec<CatalogRow>) -> Vec<CatalogGroup> {
    let mut grouped: BTreeMap<String, Vec<CatalogRow>> = BTreeMap::new();
    for row in rows {
        grouped.entry(row.vendor.clone()).or_default().push(row);
    }
    grouped
        .into_iter()
        .map(|(vendor, mut rows)| {
            rows.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
            CatalogGroup { vendor, rows }
        })
        .collect()
}

fn catalog_vendor(path: &Path, scan_roots: &[PathBuf]) -> String {
    let Some(parent) = path.parent() else {
        return "Unsorted".to_string();
    };
    if scan_roots.iter().any(|root| same_path_text(root, parent)) {
        return "Unsorted".to_string();
    }
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unsorted")
        .to_string()
}

fn same_path_text(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn selected_device_index(devices: &[DeviceRef], selected: &Option<DeviceRef>) -> Option<usize> {
    selected
        .as_ref()
        .and_then(|selected| devices.iter().position(|device| device == selected))
}

// --- Native file dialogs (Windows IFileDialog via rfd) -----------------------------------------

fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Add VST3 scan folder")
        .pick_folder()
}

fn save_session_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Galad session", &["toml"])
        .set_file_name("session.toml")
        .set_title("Save session")
        .save_file()
}

fn open_session_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Galad session", &["toml"])
        .set_title("Load session")
        .pick_file()
}
