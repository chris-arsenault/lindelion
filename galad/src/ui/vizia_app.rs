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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::Duration;

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
        background-color: #101213;
        color: #d8e0dc;
        font-size: 12px;
    }

    label { color: #cad3cf; }

    .root {
        background-color: #101213;
    }

    .topbar,
    .panel,
    .transport-strip {
        background-color: #171b1d;
        border-width: 1px;
        border-color: #2a3337;
        border-radius: 8px;
        padding: 12px;
    }

    .title {
        color: #edf5ef;
        font-size: 20px;
    }

    .section-title {
        color: #eef5f1;
        font-size: 12px;
    }

    .muted,
    .slot-meta,
    .catalog-detail,
    .meter-label {
        color: #81908a;
        font-size: 10px;
    }

    .value-label {
        color: #edf5ef;
        font-size: 11px;
    }

    .status-chip,
    .count-chip,
    .state-chip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        border-radius: 6px;
        color: #dce6e0;
        padding-left: 8px;
        padding-right: 8px;
    }

    .status-running {
        background-color: #20372f;
        border-color: #68ad8b;
        color: #e6f7ec;
    }

    .state-bypassed {
        background-color: #3a3022;
        border-color: #bd8846;
        color: #f0d5aa;
    }

    .state-error {
        background-color: #3a2528;
        border-color: #b05d63;
        color: #f1c2c6;
    }

    button.primary-button,
    button.tool-button,
    button.device-row,
    button.slot-row,
    button.catalog-row,
    select.device-select button {
        background-color: #20272a;
        border-width: 1px;
        border-color: #384448;
        corner-radius: 6px;
        color: #dce6e0;
    }

    button.primary-button:hover,
    button.tool-button:hover,
    button.device-row:hover,
    button.slot-row:hover,
    button.catalog-row:hover,
    select.device-select button:hover {
        background-color: #273235;
        border-color: #72a994;
    }

    .slot-row,
    .catalog-row-view,
    .empty-state {
        background-color: #151a1c;
        border-width: 1px;
        border-color: #2e383c;
        corner-radius: 6px;
        padding: 7px;
    }

    button.primary-button {
        background-color: #254133;
        border-color: #6cb18b;
        color: #edf8f0;
    }

    button.stop-button {
        background-color: #3a2528;
        border-color: #b05d63;
    }

    button.device-selected,
    button.catalog-compatible {
        background-color: #21342e;
        border-color: #72b894;
    }

    button.slot-bypassed {
        background-color: #282725;
        border-color: #7b6240;
    }

    .slot-bypassed {
        background-color: #1d1c19;
        border-color: #6c573b;
    }

    .catalog-compatible {
        border-color: #4f7c68;
    }

    .device-dot,
    .catalog-dot {
        background-color: #53605b;
        border-radius: 4px;
        width: 8px;
        height: 8px;
    }

    .dot-selected,
    .dot-compatible {
        background-color: #72c299;
    }

    .dot-error {
        background-color: #c96a6d;
    }

    .device-control {
        background-color: #151a1c;
        border-width: 1px;
        border-color: #2e383c;
        corner-radius: 6px;
        padding: 8px;
    }

    select.device-select {
        width: 1s;
        min-width: auto;
        height: 38px;
    }

    select.device-select button {
        width: 1s;
        height: 38px;
        alignment: left;
        padding-left: 10px;
        padding-right: 10px;
    }

    select.device-select button > * {
        width: 1s;
        min-width: auto;
        alignment: center;
    }

    select.device-select label {
        color: #edf5ef;
        text-wrap: false;
        text-overflow: ellipsis;
    }

    select.device-select svg {
        fill: #96a69f;
    }

    select.device-select popup {
        background-color: #171b1d;
        border-width: 1px;
        border-color: #3a474c;
        corner-radius: 6px;
    }

    select.device-select list.selectable list-item {
        background-color: #171b1d;
        color: #dce6e0;
        height: 34px;
    }

    select.device-select list.selectable list-item:hover,
    select.device-select list.selectable list-item.focused,
    select.device-select list.selectable list-item:focus-visible {
        background-color: #233034;
    }

    select.device-select list.selectable list-item .checkmark {
        fill: #72c299;
    }

    scrollview {
        overflow: hidden;
    }

    scrollview > scrollbar {
        display: none;
    }

    scrollview.h-scroll > scrollbar,
    scrollview.v-scroll > scrollbar {
        display: flex;
    }

    scrollbar.vertical {
        width: 8px;
    }

    scrollbar.horizontal {
        height: 8px;
    }

    scrollbar .thumb {
        background-color: #596a6f;
        corner-radius: 4px;
        opacity: 0.8;
    }

    slider {
        height: 18px;
        width: 1s;
        alignment: left;
    }

    slider .track {
        background-color: #0d1112;
        height: 5px;
        corner-radius: 3px;
    }

    slider .range {
        background-color: #79b99d;
        corner-radius: 3px;
    }

    slider .thumb {
        background-color: #dce6e0;
        border-width: 1px;
        border-color: #79b99d;
        corner-radius: 6px;
        width: 13px;
        height: 13px;
    }

    .vendor-header {
        color: #eef5f1;
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        corner-radius: 6px;
        height: 26px;
        padding-left: 8px;
        padding-right: 8px;
    }

    .meter-group {
        background-color: #141819;
        border-width: 1px;
        border-color: #2a3337;
        corner-radius: 6px;
        padding: 6px;
    }

    .master-control {
        background-color: #141819;
        border-width: 1px;
        border-color: #2a3337;
        corner-radius: 6px;
        padding: 6px;
    }

    .meter-track {
        background-color: #0d1112;
        border-width: 1px;
        border-color: #303a3e;
        corner-radius: 4px;
    }

    .meter-fill {
        background-color: #6db7a6;
        corner-radius: 3px;
    }

    .meter-fill-hot {
        background-color: #d99a4a;
    }

    .divider {
        background-color: #2a3337;
        height: 1px;
    }

    .vertical-divider {
        background-color: #2a3337;
        width: 1px;
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
        }
    }
}

/// UI events emitted by the views and the meter timer.
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
        };
        model.refresh_catalog();
        model.sync();
        diagnostics::log("ui: AppData::new done");
        model
    }

    /// Push the bindable parts of `state` into the signals.
    pub fn sync(&self) {
        self.signals
            .input_names
            .set(self.state.inputs.iter().map(|d| d.name.clone()).collect());
        self.signals
            .output_names
            .set(self.state.outputs.iter().map(|d| d.name.clone()).collect());
        self.signals.selected_input_index.set(selected_device_index(
            &self.state.inputs,
            &self.state.selected_input,
        ));
        self.signals
            .selected_output_index
            .set(selected_device_index(
                &self.state.outputs,
                &self.state.selected_output,
            ));
        self.signals.chain.set(
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
        self.signals.catalog.set(
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
        self.signals.scan_folders.set(scan_folder_rows(&self.state));
        self.signals.running.set(self.state.running);
        self.signals
            .input_left_level
            .set(self.state.meter.input_left_peak);
        self.signals
            .input_right_level
            .set(self.state.meter.input_right_peak);
        self.signals
            .output_left_level
            .set(self.state.meter.output_left_peak);
        self.signals
            .output_right_level
            .set(self.state.meter.output_right_peak);
        self.signals.master_gain_db.set(self.state.master_gain_db);
        self.signals.master_muted.set(self.state.master_muted);
        // A notice (last error/info) takes precedence in the status line; otherwise run state.
        self.signals.status.set(match &self.state.notice {
            Some(notice) => notice.clone(),
            None => if self.state.running {
                "audio live"
            } else {
                "audio off"
            }
            .to_string(),
        });
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
        // Drop the pool slot (one `Arc`); any live/retired chain still referencing the instance keeps
        // it alive until reclaimed, so its teardown stays off the audio thread.
        if index < self.runtime.pool.len() {
            self.runtime.pool.remove(index);
        }
        apply(&mut self.state, &UiCommand::RemovePlugin(index));
        self.republish();
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

    fn save_session(&mut self) {
        let Some(path) = save_session_path() else {
            return;
        };
        // Capture each plugin's opaque state straight from the pool (a UI-thread `getState`, valid
        // while audio runs) — gapless, whether running or stopped, no stop/restart needed.
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
        let session = capture_session(
            &slots,
            self.state.selected_input.clone(),
            self.state.selected_output.clone(),
            self.state.settings(),
        );
        if let Err(error) = session.save(&path) {
            self.fail(format!("failed to save session: {error:?}"));
        }
    }

    fn load_session(&mut self) {
        let Some(path) = open_session_path() else {
            return;
        };
        let session = match HostSession::load(&path) {
            Ok(session) => session,
            Err(error) => {
                self.fail(format!("failed to load {}: {error:?}", path.display()));
                return;
            }
        };
        if let Some(mut engine) = self.runtime.engine.take() {
            engine.stop();
        }
        self.state.load_session(&session);
        self.state.clear_notice();
        self.persist_settings();
        // Rebuild the pool with each plugin's state restored (unprepared — `Start` prepares it at the
        // device rate; the restored parameters survive the prepare's setActive cycle).
        match restore_pool(&session, &self.runtime.host) {
            Ok(pool) => self.runtime.pool = pool,
            Err(error) => {
                self.fail(format!("failed to restore session: {error:?}"));
                self.runtime.pool.clear();
            }
        }
        self.state.running = false;
        self.state.set_meter(MeterSnapshot::default());
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
        event.map(|app_event, _| {
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
                AppEvent::AddFromCatalog(index) => self.add_from_catalog(*index),
                AppEvent::SetMasterGain(gain_db) => self.set_master_gain(*gain_db),
                AppEvent::ToggleMasterMute => self.toggle_master_mute(),
                AppEvent::Start => self.start_engine(),
                AppEvent::Stop => self.stop_engine(),
                AppEvent::Save => self.save_session(),
                AppEvent::Load => self.load_session(),
                AppEvent::Tick => self.tick(),
            }
            self.sync();
        });
    }
}

/// Launch the standalone Galad host window. Blocks until the window is closed.
pub fn run() {
    diagnostics::log("ui: run enter");
    let idle_logged = Arc::new(AtomicBool::new(false));
    let idle_logged_for_callback = idle_logged.clone();
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
        diagnostics::log(format!("ui: enumerate outputs done count={}", outputs.len()));
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
        AppData::new(state, signals).build(cx);
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
    .on_idle(move |_| {
        if !idle_logged_for_callback.swap(true, Ordering::Relaxed) {
            diagnostics::log("ui: first on_idle");
        }
    })
    .title("Galad")
        .inner_size((1280u32, 780u32))
        .min_inner_size(Some((1120u32, 700u32)));

    diagnostics::log("ui: Application::new done; run begin");
    diagnostics::log("ui: spawn window probe");
    diagnostics::spawn_window_probe("ui-run");
    if let Err(error) = result.run() {
        eprintln!("galad: UI error: {error:?}");
        diagnostics::log(format!("ui: run error {error:?}"));
    }
    diagnostics::log("ui: run exit");
}

/// Build the host UI view tree, binding to `signals`.
pub fn build_ui(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        topbar(cx, signals);
        HStack::new(cx, move |cx| {
            device_panel(cx, signals);
            chain_panel(cx, signals);
            side_panel(cx, signals);
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0))
        .horizontal_gap(Pixels(10.0));
        transport_strip(cx, signals);
    })
    .class("root")
    .padding(Pixels(12.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn topbar(cx: &mut Context, signals: Signals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "Galad").class("title");
            Label::new(cx, signals.status).class("muted");
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(2.0));
        Binding::new(cx, signals.running, move |cx| {
            let running = signals.running.get();
            Label::new(cx, if running { "RUNNING" } else { "STOPPED" })
                .class("status-chip")
                .toggle_class("status-running", running)
                .height(Pixels(26.0));
        });
        Button::new(cx, |cx| Label::new(cx, "Open Session"))
            .on_press(|cx| cx.emit(AppEvent::Load))
            .class("tool-button")
            .width(Pixels(110.0))
            .height(Pixels(30.0));
        Button::new(cx, |cx| Label::new(cx, "Save Session"))
            .on_press(|cx| cx.emit(AppEvent::Save))
            .class("tool-button")
            .width(Pixels(110.0))
            .height(Pixels(30.0));
    })
    .class("topbar")
    .height(Pixels(62.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn device_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        section_header(cx, "Audio Devices", None);
        device_select(
            cx,
            "Input Device",
            "Select input device",
            signals.input_names,
            signals.selected_input_index,
            true,
        );
        Element::new(cx)
            .class("divider")
            .width(Stretch(1.0))
            .height(Pixels(1.0));
        device_select(
            cx,
            "Output Device",
            "Select output device",
            signals.output_names,
            signals.selected_output_index,
            false,
        );
    })
    .class("panel")
    .width(Pixels(300.0))
    .height(Stretch(1.0))
    .min_height(Pixels(260.0))
    .vertical_gap(Pixels(8.0));
}

fn device_select(
    cx: &mut Context,
    label: &'static str,
    placeholder: &'static str,
    names: Signal<Vec<String>>,
    selected: Signal<Option<usize>>,
    input: bool,
) {
    VStack::new(cx, move |cx| {
        Label::new(cx, label).class("meter-label");
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
            .width(Stretch(1.0));
    })
    .class("device-control")
    .width(Stretch(1.0))
    .height(Pixels(82.0))
    .vertical_gap(Pixels(6.0));
}

fn chain_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        section_header(
            cx,
            "Signal Path",
            Some(Memo::new(move |_| {
                count_text(signals.chain.get().len(), "loaded plugin")
            })),
        );
        ScrollView::new(cx, move |cx| {
            let chain = signals.chain;
            Binding::new(cx, chain, move |cx| {
                let rows = chain.get();
                if rows.is_empty() {
                    empty_state(cx, "No loaded plugins");
                    return;
                }
                VStack::new(cx, move |cx| {
                    for (index, row) in rows.into_iter().enumerate() {
                        chain_row(cx, index, row);
                    }
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(6.0));
            });
        })
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .class("panel")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn chain_row(cx: &mut Context, index: usize, row: ChainRow) {
    let bypassed = row.bypassed;
    let name = row.name;
    HStack::new(cx, move |cx| {
        Label::new(cx, format!("{:02}", index + 1))
            .class("count-chip")
            .width(Pixels(34.0))
            .height(Pixels(24.0));
        VStack::new(cx, move |cx| {
            Label::new(cx, name.clone())
                .class("value-label")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(cx, if bypassed { "bypassed" } else { "active" }).class("slot-meta");
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(1.0));
        Label::new(cx, if bypassed { "BYPASS" } else { "ACTIVE" })
            .class("state-chip")
            .toggle_class("state-bypassed", bypassed)
            .width(Pixels(66.0))
            .height(Pixels(24.0));
        Button::new(cx, |cx| Label::new(cx, "Editor"))
            .on_press(move |cx| cx.emit(AppEvent::OpenEditor(index)))
            .class("tool-button")
            .width(Pixels(62.0))
            .height(Pixels(28.0));
        Button::new(cx, move |cx| {
            Label::new(cx, if bypassed { "Enable" } else { "Bypass" })
        })
        .on_press(move |cx| cx.emit(AppEvent::ToggleBypass(index)))
        .class("tool-button")
        .width(Pixels(66.0))
        .height(Pixels(28.0));
        Button::new(cx, |cx| Label::new(cx, "Up"))
            .on_press(move |cx| cx.emit(AppEvent::MoveUp(index)))
            .class("tool-button")
            .width(Pixels(44.0))
            .height(Pixels(28.0));
        Button::new(cx, |cx| Label::new(cx, "Down"))
            .on_press(move |cx| cx.emit(AppEvent::MoveDown(index)))
            .class("tool-button")
            .width(Pixels(52.0))
            .height(Pixels(28.0));
        Button::new(cx, |cx| Label::new(cx, "Remove"))
            .on_press(move |cx| cx.emit(AppEvent::RemovePlugin(index)))
            .class("tool-button")
            .width(Pixels(68.0))
            .height(Pixels(28.0));
    })
    .class("slot-row")
    .toggle_class("slot-bypassed", bypassed)
    .height(Pixels(52.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}

fn side_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        vst3_folders_panel(cx, signals);
        catalog_panel(cx, signals);
    })
    .width(Pixels(440.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn vst3_folders_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            section_header(
                cx,
                "VST3 Folders",
                Some(Memo::new(move |_| {
                    count_text(signals.scan_folders.get().len(), "folder")
                })),
            );
            Button::new(cx, |cx| Label::new(cx, "Add Folder"))
                .on_press(|cx| cx.emit(AppEvent::AddScanFolder))
                .class("tool-button")
                .width(Pixels(96.0))
                .height(Pixels(30.0));
        })
        .height(Pixels(32.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));
        ScrollView::new(cx, move |cx| {
            let folders = signals.scan_folders;
            Binding::new(cx, folders, move |cx| {
                let rows = folders.get();
                if rows.is_empty() {
                    empty_state(cx, "No VST3 folders");
                    return;
                }
                VStack::new(cx, move |cx| {
                    for row in rows {
                        scan_folder_view(cx, row);
                    }
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(5.0));
            });
        })
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .height(Stretch(1.0))
        .width(Stretch(1.0));
    })
    .class("panel")
    .width(Stretch(1.0))
    .height(Pixels(162.0))
    .vertical_gap(Pixels(8.0));
}

fn scan_folder_view(cx: &mut Context, row: ScanFolderRow) {
    let system = row.system;
    let custom_index = row.custom_index;
    let path = row.path;
    let name = row.name;
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, name.clone())
                .class("value-label")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(cx, path.clone())
                .class("catalog-detail")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(1.0));
        Label::new(cx, if system { "SYSTEM" } else { "CUSTOM" })
            .class("state-chip")
            .width(Pixels(66.0))
            .height(Pixels(24.0));
        if let Some(custom_index) = custom_index {
            Button::new(cx, |cx| Label::new(cx, "Remove"))
                .on_press(move |cx| cx.emit(AppEvent::RemoveScanFolder(custom_index)))
                .class("tool-button")
                .width(Pixels(66.0))
                .height(Pixels(26.0));
        }
    })
    .class("catalog-row-view")
    .height(Pixels(46.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}

fn catalog_panel(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            section_header(
                cx,
                "Available VST3s",
                Some(Memo::new(move |_| {
                    count_text(signals.catalog.get().len(), "plugin")
                })),
            );
            Button::new(cx, |cx| Label::new(cx, "Rescan"))
                .on_press(|cx| cx.emit(AppEvent::RescanPlugins))
                .class("tool-button")
                .width(Pixels(70.0))
                .height(Pixels(30.0));
        })
        .height(Pixels(32.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));
        ScrollView::new(cx, move |cx| {
            let catalog = signals.catalog;
            Binding::new(cx, catalog, move |cx| {
                let rows = catalog.get();
                if rows.is_empty() {
                    empty_state(cx, "No available VST3s");
                    return;
                }
                VStack::new(cx, move |cx| {
                    for group in catalog_groups(rows) {
                        Label::new(cx, group.vendor)
                            .class("vendor-header")
                            .width(Stretch(1.0));
                        for row in group.rows {
                            catalog_row(cx, row);
                        }
                    }
                })
                .width(Stretch(1.0))
                .vertical_gap(Pixels(5.0));
            });
        })
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(true)
        .height(Stretch(1.0))
        .width(Stretch(1.0));
    })
    .class("panel")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn catalog_row(cx: &mut Context, row: CatalogRow) {
    let index = row.catalog_index;
    let compatible = row.compatible;
    let detail = if compatible {
        "compatible".to_string()
    } else if row.detail.is_empty() {
        "incompatible".to_string()
    } else {
        row.detail
    };
    let path = row.path;
    let name = row.name;
    HStack::new(cx, move |cx| {
        Element::new(cx)
            .class("catalog-dot")
            .toggle_class("dot-compatible", compatible)
            .toggle_class("dot-error", !compatible);
        VStack::new(cx, move |cx| {
            Label::new(cx, name.clone())
                .class("value-label")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(cx, detail.clone())
                .class("catalog-detail")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
            Label::new(cx, path.clone())
                .class("catalog-detail")
                .width(Stretch(1.0))
                .min_width(Pixels(0.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .vertical_gap(Pixels(1.0));
        if compatible {
            Button::new(cx, |cx| Label::new(cx, "Load"))
                .on_press(move |cx| cx.emit(AppEvent::AddFromCatalog(index)))
                .class("tool-button")
                .width(Pixels(54.0))
                .height(Pixels(26.0));
        } else {
            Label::new(cx, "BLOCK")
                .class("state-chip")
                .class("state-error")
                .width(Pixels(54.0))
                .height(Pixels(24.0));
        }
    })
    .class("catalog-row-view")
    .toggle_class("catalog-compatible", compatible)
    .height(Pixels(62.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}

fn meter(cx: &mut Context, label: &'static str, level: Signal<f32>, hot: bool) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, label)
                .class("meter-label")
                .width(Stretch(1.0));
            Label::new(cx, Memo::new(move |_| level_text(level.get()))).class("meter-label");
        })
        .height(Pixels(16.0))
        .alignment(Alignment::Center);
        Binding::new(cx, level, move |cx| {
            let width = meter_width_percent(level.get());
            HStack::new(cx, move |cx| {
                Element::new(cx)
                    .class("meter-fill")
                    .toggle_class("meter-fill-hot", hot)
                    .width(Percentage(width))
                    .height(Stretch(1.0));
                Spacer::new(cx);
            })
            .class("meter-track")
            .width(Stretch(1.0))
            .height(Pixels(12.0));
        });
    })
    .width(Stretch(1.0))
    .height(Pixels(34.0))
    .vertical_gap(Pixels(3.0));
}

fn transport_strip(cx: &mut Context, signals: Signals) {
    HStack::new(cx, move |cx| {
        let running = signals.running;
        Button::new(cx, move |cx| {
            Label::new(
                cx,
                Memo::new(move |_| {
                    if running.get() {
                        "Audio Off"
                    } else {
                        "Audio On"
                    }
                }),
            )
        })
        .on_press(move |cx| {
            if running.get() {
                cx.emit(AppEvent::Stop);
            } else {
                cx.emit(AppEvent::Start);
            }
        })
        .class("primary-button")
        .toggle_class("stop-button", signals.running)
        .width(Pixels(96.0))
        .height(Pixels(34.0));
        Element::new(cx)
            .class("vertical-divider")
            .width(Pixels(1.0))
            .height(Pixels(58.0));
        meter_group(
            cx,
            "Input",
            signals.input_left_level,
            signals.input_right_level,
            false,
        );
        meter_group(
            cx,
            "Output",
            signals.output_left_level,
            signals.output_right_level,
            true,
        );
        master_control(cx, signals);
        Label::new(cx, signals.status)
            .class("muted")
            .width(Stretch(1.0))
            .min_width(Pixels(0.0));
    })
    .class("transport-strip")
    .height(Pixels(96.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(10.0));
}

fn meter_group(
    cx: &mut Context,
    title: &'static str,
    left: Signal<f32>,
    right: Signal<f32>,
    hot: bool,
) {
    VStack::new(cx, move |cx| {
        Label::new(cx, title).class("meter-label");
        meter(cx, "L", left, hot);
        meter(cx, "R", right, hot);
    })
    .class("meter-group")
    .width(Pixels(190.0))
    .height(Pixels(72.0))
    .vertical_gap(Pixels(2.0));
}

fn master_control(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Master")
                .class("meter-label")
                .width(Stretch(1.0));
            Label::new(
                cx,
                Memo::new(move |_| gain_text(signals.master_gain_db.get())),
            )
            .class("meter-label");
        })
        .height(Pixels(16.0))
        .alignment(Alignment::Center);
        HStack::new(cx, move |cx| {
            Slider::new(cx, signals.master_gain_db)
                .range(MASTER_GAIN_MIN_DB..MASTER_GAIN_MAX_DB)
                .step(0.5f32)
                .on_change(|cx, gain| cx.emit(AppEvent::SetMasterGain(gain)));
            Button::new(cx, move |cx| {
                Label::new(
                    cx,
                    Memo::new(move |_| {
                        if signals.master_muted.get() {
                            "Muted"
                        } else {
                            "Mute"
                        }
                    }),
                )
            })
            .on_press(|cx| cx.emit(AppEvent::ToggleMasterMute))
            .class("tool-button")
            .toggle_class("stop-button", signals.master_muted)
            .width(Pixels(62.0))
            .height(Pixels(28.0));
        })
        .height(Pixels(34.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));
    })
    .class("master-control")
    .width(Pixels(250.0))
    .height(Pixels(72.0))
    .vertical_gap(Pixels(4.0));
}

fn section_header(cx: &mut Context, title: &'static str, count: Option<Memo<String>>) {
    HStack::new(cx, move |cx| {
        Label::new(cx, title)
            .class("section-title")
            .width(Stretch(1.0))
            .min_width(Pixels(0.0));
        if let Some(count) = count {
            Label::new(cx, count)
                .class("count-chip")
                .height(Pixels(24.0));
        }
    })
    .height(Pixels(26.0))
    .width(Stretch(1.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn empty_state(cx: &mut Context, text: &'static str) {
    VStack::new(cx, move |cx| {
        Spacer::new(cx);
        Label::new(cx, text).class("muted");
        Spacer::new(cx);
    })
    .class("empty-state")
    .width(Stretch(1.0))
    .height(Pixels(96.0))
    .alignment(Alignment::Center);
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
