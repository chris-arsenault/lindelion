//! Vizia host UI — Model + views + the standalone `Application`, controller, and entry point (M6
//! Steps 6–7). Windows-only. The neutral `HostUiState`/`UiCommand` (Steps 3–4, Linux-tested) are the
//! source of truth; the `Model` wraps a `HostUiState` and mirrors its bindable parts into reactive
//! `Signal`s the views read. The **controller** (`Runtime`, folded into the `Model`) owns the live
//! `AudioEngine` (M2/M3), the `EditorHost` (M5), and the loaded `.vst3` modules, and executes the
//! effectful commands: start/stop, live chain edits republished through the M3 `Handoff`, plugin
//! editors, and session save/load (M4). A Vizia timer ticks the meters off the audio thread.
//!
//! Known limitation (M7 robustness): a live chain edit (add/remove/reorder/bypass while running)
//! rebuilds the chain from the loaded modules and republishes it gaplessly — fresh plugin instances,
//! so per-plugin parameter edits made via a plugin's own editor are not preserved across a live edit.
//! Session save/load *do* preserve per-plugin state (M4 capture/restore). Live state hand-off across
//! edits, and negotiating the chain's sample rate to the device rate (fixed 48 kHz here, matching the
//! existing `main` field-check default), are deferred to M7.

use std::path::PathBuf;
use std::time::Duration;

use vizia::prelude::*;
use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;

use crate::audio::{AudioDirection, AudioEngine, MeterSnapshot, enumerate};
use crate::session::{AppSettings, HostSession};
use crate::ui::command::{UiCommand, apply};
use crate::ui::state::{Dir, HostUiState, PluginCatalog};
use crate::vst3_host::{
    ChainProcessor, EditorHost, HostContext, HostError, LoadedModule, PluginInstance, SessionSlot,
    capture_session, load_module, restore_chain, validate_plugin,
};

const STYLE: &str = r#"
    :root { background-color: #14191d; color: #cfe0d8; font-size: 13px; }
    .root { child-space: 14px; row-between: 10px; }
    .title { color: #e2e8ea; font-size: 22px; }
    .section { color: #93a1a8; font-size: 13px; }
    .row { col-between: 6px; height: 30px; }
"#;

/// Sample rate / block size the chain is prepared at. Fixed for now (the device-rate negotiation is
/// an M7 refinement); matches the existing `main` field-check default.
const CHAIN_SAMPLE_RATE: f64 = 48_000.0;
const CHAIN_MAX_FRAMES: usize = 4096;

/// One chain row, bound into the list view.
#[derive(Clone, PartialEq)]
pub struct ChainRow {
    pub name: String,
    pub bypassed: bool,
}

/// One scanned-plugin row, bound into the catalog view.
#[derive(Clone, PartialEq)]
pub struct CatalogRow {
    pub name: String,
    pub compatible: bool,
    /// Why it is incompatible (empty when compatible).
    pub detail: String,
}

/// The reactive signals the views bind to. `Signal` is `Copy`, so this is cheap to pass around.
#[derive(Clone, Copy)]
pub struct Signals {
    pub input_names: Signal<Vec<String>>,
    pub output_names: Signal<Vec<String>>,
    pub chain: Signal<Vec<ChainRow>>,
    pub catalog: Signal<Vec<CatalogRow>>,
    pub running: Signal<bool>,
    pub input_level: Signal<f32>,
    pub output_level: Signal<f32>,
    pub status: Signal<String>,
}

impl Signals {
    /// Create the signals (must run inside a Vizia reactive scope — the `Application` closure).
    pub fn new() -> Self {
        Signals {
            input_names: Signal::new(Vec::new()),
            output_names: Signal::new(Vec::new()),
            chain: Signal::new(Vec::new()),
            catalog: Signal::new(Vec::new()),
            running: Signal::new(false),
            input_level: Signal::new(0.0),
            output_level: Signal::new(0.0),
            status: Signal::new("stopped".to_string()),
        }
    }
}

/// UI events emitted by the views and the meter timer.
pub enum AppEvent {
    SelectInput(usize),
    SelectOutput(usize),
    AddPlugin,
    RemovePlugin(usize),
    MoveUp(usize),
    MoveDown(usize),
    ToggleBypass(usize),
    OpenEditor(usize),
    /// Scan a folder for plugins and validate each.
    Scan,
    /// Add a compatible plugin from the scanned catalog by index.
    AddFromCatalog(usize),
    Start,
    Stop,
    Save,
    Load,
    /// Meter timer tick — pull the latest snapshot off the audio thread.
    Tick,
}

/// The controller: the live host runtime the Model drives. Owns the engine, the editor host, the
/// loaded modules (kept alive and parallel to `HostUiState.chain`), and a pending restored chain.
struct Runtime {
    host: ComPtr<IHostApplication>,
    engine: Option<AudioEngine>,
    editor: EditorHost,
    /// Loaded `.vst3` modules, index-aligned with `HostUiState.chain`. Kept alive because they own
    /// the DLLs the plugin instances live in.
    modules: Vec<LoadedModule>,
    /// A chain built with restored per-plugin state (from `LoadSession`), consumed by the next
    /// `Start` so the restored state survives until the engine runs. Invalidated by any chain edit.
    pending_chain: Option<Box<ChainProcessor>>,
}

impl Runtime {
    fn new() -> Self {
        Runtime {
            host: HostContext::new()
                .to_com_ptr::<IHostApplication>()
                .expect("host exposes IHostApplication"),
            engine: None,
            editor: EditorHost::new(),
            modules: Vec::new(),
            pending_chain: None,
        }
    }

    fn is_running(&self) -> bool {
        self.engine.is_some()
    }

    /// Instantiate the loaded modules into a fresh prepared chain (fresh plugin instances).
    fn build_chain(&self, state: &HostUiState) -> Result<Box<ChainProcessor>, HostError> {
        let mut instances = Vec::with_capacity(self.modules.len());
        for module in &self.modules {
            instances.push(PluginInstance::from_factory(module.factory(), &self.host)?);
        }
        let bypass: Vec<bool> = state.chain.iter().map(|slot| slot.bypassed).collect();
        Ok(Box::new(ChainProcessor::new(
            instances,
            bypass,
            CHAIN_SAMPLE_RATE,
            CHAIN_MAX_FRAMES,
        )?))
    }
}

/// The Vizia model: the neutral host state (source of truth), the reactive signals (view mirror),
/// and the live runtime (controller).
pub struct AppData {
    pub state: HostUiState,
    pub signals: Signals,
    runtime: Runtime,
}

impl AppData {
    pub fn new(state: HostUiState, signals: Signals) -> Self {
        let model = AppData {
            state,
            signals,
            runtime: Runtime::new(),
        };
        model.sync();
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
        self.signals.catalog.set(
            self.state
                .catalog
                .entries
                .iter()
                .map(|entry| CatalogRow {
                    name: entry.name.clone(),
                    compatible: entry.compatible,
                    detail: entry.reason.clone().unwrap_or_default(),
                })
                .collect(),
        );
        self.signals.running.set(self.state.running);
        self.signals.input_level.set(self.state.meter.input_peak);
        self.signals.output_level.set(self.state.meter.output_peak);
        // A notice (last error/info) takes precedence in the status line; otherwise run state.
        self.signals.status.set(match &self.state.notice {
            Some(notice) => notice.clone(),
            None => if self.state.running {
                "running"
            } else {
                "stopped"
            }
            .to_string(),
        });
    }

    // --- Controller commands (effectful) -------------------------------------------------------

    /// Log a failure and surface it as a UI notice.
    fn fail(&mut self, message: String) {
        eprintln!("galad: {message}");
        self.state.set_notice(message);
    }

    /// If running, rebuild the chain from the loaded modules and hand it to the audio thread.
    fn republish(&mut self) {
        if !self.runtime.is_running() {
            return;
        }
        match self.runtime.build_chain(&self.state) {
            Ok(chain) => {
                if let Some(engine) = &self.runtime.engine {
                    engine.publish_chain(chain);
                }
            }
            Err(error) => self.fail(format!("failed to rebuild chain: {error:?}")),
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
        let chain = match self.runtime.pending_chain.take() {
            Some(chain) => chain,
            None => match self.runtime.build_chain(&self.state) {
                Ok(chain) => chain,
                Err(error) => {
                    self.fail(format!("failed to build chain: {error:?}"));
                    return;
                }
            },
        };
        match AudioEngine::start_with_chain(input, output, chain) {
            Ok(engine) => {
                self.runtime.engine = Some(engine);
                apply(&mut self.state, &UiCommand::Start);
                self.state.clear_notice();
            }
            Err(error) => self.fail(format!("failed to start engine: {error:?}")),
        }
    }

    fn stop_engine(&mut self) {
        if let Some(mut engine) = self.runtime.engine.take() {
            engine.stop();
        }
        apply(&mut self.state, &UiCommand::Stop);
        self.state.set_meter(MeterSnapshot::default());
        self.state.clear_notice();
    }

    fn add_plugin(&mut self) {
        let Some(path) = pick_vst3() else {
            return;
        };
        // Probe the plugin before adding it — an incompatible or mid-process-failing plugin is
        // rejected here (with a UI notice) instead of failing later on the audio thread (M7).
        if let Err(error) = validate_plugin(&path) {
            self.fail(format!("rejecting {}: {error:?}", path.display()));
            return;
        }
        match load_module(&path) {
            Ok(module) => {
                self.runtime.modules.push(module);
                self.runtime.pending_chain = None;
                apply(&mut self.state, &UiCommand::AddPlugin(path));
                self.state.clear_notice();
                self.republish();
            }
            Err(error) => self.fail(format!("failed to load {}: {error:?}", path.display())),
        }
    }

    /// Scan a folder for `.vst3` plugins, validate each (M7 Step 3), and present the catalog. The
    /// scanned folder is remembered so it persists with the session.
    fn scan_folder(&mut self) {
        let Some(dir) = pick_folder() else {
            return;
        };
        let probes = HostUiState::scan_dir(&dir)
            .into_iter()
            .map(|path| {
                let result = validate_plugin(&path).map_err(|error| format!("{error:?}"));
                (path, result)
            })
            .collect();
        self.state.set_catalog(PluginCatalog::from_probes(probes));
        self.state.record_scan_dir(dir);
        let compatible = self.state.catalog.compatible().count();
        let total = self.state.catalog.entries.len();
        self.state.set_notice(format!(
            "scanned {total} plugin(s), {compatible} compatible"
        ));
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
        match load_module(&path) {
            Ok(module) => {
                self.runtime.modules.push(module);
                self.runtime.pending_chain = None;
                apply(&mut self.state, &UiCommand::AddPlugin(path));
                self.state.clear_notice();
                self.republish();
            }
            Err(error) => self.fail(format!("failed to load {}: {error:?}", path.display())),
        }
    }

    fn remove_plugin(&mut self, index: usize) {
        if index < self.runtime.modules.len() {
            self.runtime.modules.remove(index);
        }
        self.runtime.pending_chain = None;
        apply(&mut self.state, &UiCommand::RemovePlugin(index));
        self.republish();
    }

    fn reorder(&mut self, index: usize, dir: Dir) {
        // Mirror `HostUiState::move_slot`'s bounds so `modules` stays index-aligned with `chain`.
        let len = self.runtime.modules.len();
        let target = match dir {
            Dir::Up if index > 0 => Some(index - 1),
            Dir::Down if index + 1 < len => Some(index + 1),
            _ => None,
        };
        if let Some(target) = target {
            self.runtime.modules.swap(index, target);
        }
        self.runtime.pending_chain = None;
        apply(&mut self.state, &UiCommand::MoveSlot(index, dir));
        self.republish();
    }

    fn toggle_bypass(&mut self, index: usize) {
        self.runtime.pending_chain = None;
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
        // Running: capture each plugin's live state, then resume from the captured session so the
        // live edits persist across the brief save gap. Stopped: structure-only (paths/order/bypass).
        let Some(mut engine) = self.runtime.engine.take() else {
            if let Err(error) = self.state.to_session().save(&path) {
                self.fail(format!("failed to save session: {error:?}"));
            }
            return;
        };
        let Some(chain) = engine.stop_and_take() else {
            apply(&mut self.state, &UiCommand::Stop);
            return;
        };
        let slots: Vec<SessionSlot> = self
            .state
            .chain
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                chain.component(index).map(|component| SessionSlot {
                    plugin_path: slot.path.clone(),
                    component,
                    bypassed: slot.bypassed,
                })
            })
            .collect();
        let session = capture_session(
            &slots,
            self.state.selected_input.clone(),
            self.state.selected_output.clone(),
            AppSettings::default(),
        );
        if let Err(error) = session.save(&path) {
            self.fail(format!("failed to save session: {error:?}"));
        }
        // Resume from the captured session (state-preserving), keeping the run going.
        match restore_chain(
            &session,
            &self.runtime.host,
            CHAIN_SAMPLE_RATE,
            CHAIN_MAX_FRAMES,
        ) {
            Ok((modules, new_chain)) => {
                self.runtime.modules = modules;
                let (Some(input), Some(output)) = (
                    self.state.selected_input.clone(),
                    self.state.selected_output.clone(),
                ) else {
                    apply(&mut self.state, &UiCommand::Stop);
                    return;
                };
                match AudioEngine::start_with_chain(input, output, Box::new(new_chain)) {
                    Ok(engine) => self.runtime.engine = Some(engine),
                    Err(error) => {
                        self.fail(format!("failed to resume after save: {error:?}"));
                        apply(&mut self.state, &UiCommand::Stop);
                    }
                }
            }
            Err(error) => {
                self.fail(format!("failed to rebuild after save: {error:?}"));
                apply(&mut self.state, &UiCommand::Stop);
            }
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
        // Rebuild the modules + a state-restored chain; hold it for the next Start (press Start to run).
        match restore_chain(
            &session,
            &self.runtime.host,
            CHAIN_SAMPLE_RATE,
            CHAIN_MAX_FRAMES,
        ) {
            Ok((modules, chain)) => {
                self.runtime.modules = modules;
                self.runtime.pending_chain = Some(Box::new(chain));
            }
            Err(error) => {
                self.fail(format!("failed to restore chain: {error:?}"));
                self.runtime.modules.clear();
                self.runtime.pending_chain = None;
            }
        }
        self.state.running = false;
        self.state.set_meter(MeterSnapshot::default());
    }

    fn tick(&mut self) {
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
            self.runtime.pending_chain = None;
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
                AppEvent::AddPlugin => self.add_plugin(),
                AppEvent::RemovePlugin(index) => self.remove_plugin(*index),
                AppEvent::MoveUp(index) => self.reorder(*index, Dir::Up),
                AppEvent::MoveDown(index) => self.reorder(*index, Dir::Down),
                AppEvent::ToggleBypass(index) => self.toggle_bypass(*index),
                AppEvent::OpenEditor(index) => self.open_editor(*index),
                AppEvent::Scan => self.scan_folder(),
                AppEvent::AddFromCatalog(index) => self.add_from_catalog(*index),
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
    let result = vizia::Application::new(|cx| {
        let _ = cx.add_stylesheet(STYLE);

        let inputs = enumerate(AudioDirection::Input).unwrap_or_default();
        let outputs = enumerate(AudioDirection::Output).unwrap_or_default();
        let mut state = HostUiState::default();
        state.set_devices(inputs, outputs);

        let signals = Signals::new();
        AppData::new(state, signals).build(cx);

        // Drive the meters off the audio thread: tick ~30 Hz, pulling the latest seqlock snapshot.
        let meter_timer = cx.add_timer(Duration::from_millis(33), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(AppEvent::Tick);
            }
        });
        cx.start_timer(meter_timer);

        build_ui(cx, signals);
    })
    .title("Galad")
    .inner_size((760u32, 620u32));

    if let Err(error) = result.run() {
        eprintln!("galad: UI error: {error:?}");
    }
}

/// Build the host UI view tree, binding to `signals`.
pub fn build_ui(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Galad").class("title");
        device_pickers(cx, signals);
        chain_view(cx, signals);
        scan_view(cx, signals);
        meters(cx, signals);
        controls(cx, signals);
    })
    .class("root");
}

fn device_pickers(cx: &mut Context, signals: Signals) {
    Label::new(cx, "Input").class("section");
    let inputs = signals.input_names;
    Binding::new(cx, inputs, move |cx| {
        HStack::new(cx, move |cx| {
            for (index, name) in inputs.get().into_iter().enumerate() {
                Button::new(cx, move |cx| Label::new(cx, name))
                    .on_press(move |cx| cx.emit(AppEvent::SelectInput(index)));
            }
        })
        .class("row");
    });

    Label::new(cx, "Output").class("section");
    let outputs = signals.output_names;
    Binding::new(cx, outputs, move |cx| {
        HStack::new(cx, move |cx| {
            for (index, name) in outputs.get().into_iter().enumerate() {
                Button::new(cx, move |cx| Label::new(cx, name))
                    .on_press(move |cx| cx.emit(AppEvent::SelectOutput(index)));
            }
        })
        .class("row");
    });
}

fn chain_view(cx: &mut Context, signals: Signals) {
    Label::new(cx, "Chain").class("section");
    let chain = signals.chain;
    Binding::new(cx, chain, move |cx| {
        VStack::new(cx, move |cx| {
            for (index, row) in chain.get().into_iter().enumerate() {
                HStack::new(cx, move |cx| {
                    Label::new(cx, row.name);
                    Button::new(cx, |cx| Label::new(cx, "bypass"))
                        .on_press(move |cx| cx.emit(AppEvent::ToggleBypass(index)));
                    Button::new(cx, |cx| Label::new(cx, "up"))
                        .on_press(move |cx| cx.emit(AppEvent::MoveUp(index)));
                    Button::new(cx, |cx| Label::new(cx, "down"))
                        .on_press(move |cx| cx.emit(AppEvent::MoveDown(index)));
                    Button::new(cx, |cx| Label::new(cx, "editor"))
                        .on_press(move |cx| cx.emit(AppEvent::OpenEditor(index)));
                    Button::new(cx, |cx| Label::new(cx, "remove"))
                        .on_press(move |cx| cx.emit(AppEvent::RemovePlugin(index)));
                })
                .class("row");
            }
        });
    });
    Button::new(cx, |cx| Label::new(cx, "add plugin…")).on_press(|cx| cx.emit(AppEvent::AddPlugin));
}

fn scan_view(cx: &mut Context, signals: Signals) {
    Label::new(cx, "Scan folder").class("section");
    Button::new(cx, |cx| Label::new(cx, "scan folder…")).on_press(|cx| cx.emit(AppEvent::Scan));
    let catalog = signals.catalog;
    Binding::new(cx, catalog, move |cx| {
        VStack::new(cx, move |cx| {
            for (index, row) in catalog.get().into_iter().enumerate() {
                HStack::new(cx, move |cx| {
                    Label::new(cx, row.name.clone());
                    if row.compatible {
                        Button::new(cx, |cx| Label::new(cx, "add"))
                            .on_press(move |cx| cx.emit(AppEvent::AddFromCatalog(index)));
                    } else {
                        Label::new(cx, format!("incompatible: {}", row.detail)).class("section");
                    }
                })
                .class("row");
            }
        });
    });
}

fn meters(cx: &mut Context, signals: Signals) {
    Label::new(cx, "Meters").class("section");
    Label::new(cx, signals.input_level);
    Label::new(cx, signals.output_level);
}

fn controls(cx: &mut Context, signals: Signals) {
    HStack::new(cx, |cx| {
        Button::new(cx, |cx| Label::new(cx, "start")).on_press(|cx| cx.emit(AppEvent::Start));
        Button::new(cx, |cx| Label::new(cx, "stop")).on_press(|cx| cx.emit(AppEvent::Stop));
        Button::new(cx, |cx| Label::new(cx, "save session…"))
            .on_press(|cx| cx.emit(AppEvent::Save));
        Button::new(cx, |cx| Label::new(cx, "load session…"))
            .on_press(|cx| cx.emit(AppEvent::Load));
    })
    .class("row");
    Label::new(cx, signals.status).class("section");
}

// --- Native file dialogs (Windows IFileDialog via rfd) -----------------------------------------

fn pick_vst3() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("VST3 plugin", &["vst3"])
        .set_title("Add VST3 plugin")
        .pick_file()
}

fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Scan folder for VST3 plugins")
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
