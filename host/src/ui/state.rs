//! Framework-neutral host UI state + transitions. The Vizia `Model` (M6 Step 6) wraps this, and the
//! controller (Step 7) drives it; keeping it plain Rust makes the bulk of the UI testable without
//! Vizia.

use std::path::{Path, PathBuf};

use crate::audio::{EngineStatus, MeterSnapshot};
use crate::session::{AppSettings, ChainSlot, DeviceRef, HostSession};

/// Direction for reordering a chain slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
}

/// One plugin slot as shown in the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct UiSlot {
    pub path: PathBuf,
    pub name: String,
    pub bypassed: bool,
}

impl UiSlot {
    fn from_path(path: PathBuf, bypassed: bool) -> Self {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("plugin")
            .to_string();
        UiSlot {
            path,
            name,
            bypassed,
        }
    }
}

/// One discovered plugin from a folder scan, with its validation status.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub path: PathBuf,
    pub name: String,
    /// Whether the plugin passed the load-time validation probe (M7 Step 3).
    pub compatible: bool,
    /// Why it failed validation, if incompatible.
    pub reason: Option<String>,
}

/// The result of scanning folders for plugins: each discovered `.vst3` plus its validation status.
/// On-demand and transient — not cached or persisted (only the scanned *folders* persist).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginCatalog {
    pub entries: Vec<CatalogEntry>,
}

impl PluginCatalog {
    /// Assemble a catalog from `(path, validation-result)` pairs — `Ok(())` = compatible, `Err(reason)`
    /// = incompatible with a reason — sorted by path. Pure: the fs scan and probing happen in the
    /// caller (the probe via M7 Step 3's `validate_plugin`).
    pub fn from_probes(probes: Vec<(PathBuf, Result<(), String>)>) -> Self {
        let mut entries: Vec<CatalogEntry> = probes
            .into_iter()
            .map(|(path, result)| {
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("plugin")
                    .to_string();
                CatalogEntry {
                    path,
                    name,
                    compatible: result.is_ok(),
                    reason: result.err(),
                }
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        PluginCatalog { entries }
    }

    /// The compatible plugins (those offered for adding).
    pub fn compatible(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries.iter().filter(|entry| entry.compatible)
    }
}

/// The host's UI state: available + selected devices, the plugin chain, scan folders, the run state,
/// the latest meter snapshot, the last notice, and the scanned-plugin catalog.
#[derive(Debug, Clone, Default)]
pub struct HostUiState {
    pub inputs: Vec<DeviceRef>,
    pub outputs: Vec<DeviceRef>,
    pub selected_input: Option<DeviceRef>,
    pub selected_output: Option<DeviceRef>,
    pub chain: Vec<UiSlot>,
    pub scan_dirs: Vec<PathBuf>,
    pub running: bool,
    pub meter: MeterSnapshot,
    /// The latest error/info to surface in the UI (transient — not persisted in a session).
    pub notice: Option<String>,
    /// The most recent folder-scan result (transient — only the scanned folders persist).
    pub catalog: PluginCatalog,
}

impl HostUiState {
    /// Set the available device lists (from enumeration).
    pub fn set_devices(&mut self, inputs: Vec<DeviceRef>, outputs: Vec<DeviceRef>) {
        self.inputs = inputs;
        self.outputs = outputs;
    }

    /// Select the input device.
    pub fn select_input(&mut self, device: DeviceRef) {
        self.selected_input = Some(device);
    }

    /// Select the output device.
    pub fn select_output(&mut self, device: DeviceRef) {
        self.selected_output = Some(device);
    }

    /// Append a plugin slot (not bypassed) to the end of the chain.
    pub fn add_slot(&mut self, path: PathBuf) {
        self.chain.push(UiSlot::from_path(path, false));
    }

    /// Remove the slot at `index` (no-op if out of range).
    pub fn remove_slot(&mut self, index: usize) {
        if index < self.chain.len() {
            self.chain.remove(index);
        }
    }

    /// Move the slot at `index` one position in `dir`; a no-op at the ends.
    pub fn move_slot(&mut self, index: usize, dir: Dir) {
        let target = match dir {
            Dir::Up if index > 0 => index - 1,
            Dir::Down if index + 1 < self.chain.len() => index + 1,
            _ => return,
        };
        self.chain.swap(index, target);
    }

    /// Toggle the bypass of the slot at `index` (no-op if out of range).
    pub fn toggle_bypass(&mut self, index: usize) {
        if let Some(slot) = self.chain.get_mut(index) {
            slot.bypassed = !slot.bypassed;
        }
    }

    /// Update the displayed meter snapshot.
    pub fn set_meter(&mut self, meter: MeterSnapshot) {
        self.meter = meter;
    }

    /// Surface an error/info notice in the UI.
    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
    }

    /// Clear any displayed notice.
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    /// React to the engine's published run status. On a runtime `Faulted` (e.g. the device was
    /// invalidated) this stops the UI's running state and posts a notice — the **stop + notify**
    /// recovery policy (M7 Step 7). Returns `true` when a fault was handled (the caller then tears
    /// down the dead engine). `Running`/`StoppedByUser` are no-ops here.
    pub fn react_to_engine_status(&mut self, status: EngineStatus) -> bool {
        if status == EngineStatus::Faulted {
            self.running = false;
            self.meter = MeterSnapshot::default();
            self.set_notice("Audio device disconnected — select a device and press Start.");
            true
        } else {
            false
        }
    }

    /// Replace the scanned-plugin catalog (from a folder scan).
    pub fn set_catalog(&mut self, catalog: PluginCatalog) {
        self.catalog = catalog;
    }

    /// Remember a scanned folder (deduplicated) so it persists with the session.
    pub fn record_scan_dir(&mut self, dir: PathBuf) {
        if !self.scan_dirs.contains(&dir) {
            self.scan_dirs.push(dir);
        }
    }

    /// List the `*.vst3` entries under `dir`, sorted.
    pub fn scan_dir(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("vst3") {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }

    /// The session structure for the current chain + devices (paths + bypass; plugin state is
    /// captured live by the controller's M4 `capture_session`).
    pub fn to_session(&self) -> HostSession {
        HostSession {
            input: self.selected_input.clone(),
            output: self.selected_output.clone(),
            chain: self
                .chain
                .iter()
                .map(|slot| ChainSlot {
                    plugin_path: slot.path.clone(),
                    bypassed: slot.bypassed,
                    state: None,
                })
                .collect(),
            settings: AppSettings {
                plugin_scan_dirs: self.scan_dirs.clone(),
            },
        }
    }

    /// Replace the devices + chain + scan folders from a loaded session.
    pub fn load_session(&mut self, session: &HostSession) {
        self.selected_input = session.input.clone();
        self.selected_output = session.output.clone();
        self.chain = session
            .chain
            .iter()
            .map(|slot| UiSlot::from_path(slot.plugin_path.clone(), slot.bypassed))
            .collect();
        self.scan_dirs = session.settings.plugin_scan_dirs.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(state: &HostUiState) -> Vec<&str> {
        state
            .chain
            .iter()
            .map(|slot| slot.path.to_str().unwrap())
            .collect()
    }

    #[test]
    fn add_move_remove_bypass_transitions() {
        let mut state = HostUiState::default();
        state.add_slot(PathBuf::from("/p/A.vst3"));
        state.add_slot(PathBuf::from("/p/B.vst3"));
        assert_eq!(paths(&state), ["/p/A.vst3", "/p/B.vst3"]);

        state.move_slot(1, Dir::Up);
        assert_eq!(paths(&state), ["/p/B.vst3", "/p/A.vst3"]);

        // Moving at the ends is a no-op.
        state.move_slot(0, Dir::Up);
        state.move_slot(1, Dir::Down);
        assert_eq!(paths(&state), ["/p/B.vst3", "/p/A.vst3"]);

        state.toggle_bypass(0);
        assert!(state.chain[0].bypassed);

        state.remove_slot(0);
        assert_eq!(paths(&state), ["/p/A.vst3"]);
    }

    #[test]
    fn chain_round_trips_through_session() {
        let mut state = HostUiState::default();
        state.add_slot(PathBuf::from("/p/A.vst3"));
        state.add_slot(PathBuf::from("/p/B.vst3"));
        state.toggle_bypass(1);
        state.select_input(DeviceRef {
            id: "in".to_string(),
            name: "Mic".to_string(),
        });
        state.select_output(DeviceRef {
            id: "out".to_string(),
            name: "Out".to_string(),
        });

        let session = state.to_session();
        let mut restored = HostUiState::default();
        restored.load_session(&session);

        assert_eq!(restored.chain.len(), 2);
        assert_eq!(restored.chain[0].path, PathBuf::from("/p/A.vst3"));
        assert!(!restored.chain[0].bypassed);
        assert!(restored.chain[1].bypassed);
        assert_eq!(restored.selected_input, state.selected_input);
        assert_eq!(restored.selected_output, state.selected_output);
    }

    #[test]
    fn notice_sets_and_clears() {
        let mut state = HostUiState::default();
        assert!(state.notice.is_none());
        state.set_notice("boom");
        assert_eq!(state.notice.as_deref(), Some("boom"));
        state.clear_notice();
        assert!(state.notice.is_none());
    }

    #[test]
    fn engine_fault_stops_and_notifies() {
        let mut state = HostUiState::default();
        state.running = true;

        // A non-fault status leaves the running state alone and posts no notice.
        assert!(!state.react_to_engine_status(EngineStatus::Running));
        assert!(state.running);
        assert!(state.notice.is_none());

        // A fault stops + notifies and reports that it handled a fault.
        assert!(state.react_to_engine_status(EngineStatus::Faulted));
        assert!(!state.running);
        assert!(state.notice.is_some());
    }

    #[test]
    fn notice_is_not_persisted_in_a_session() {
        let mut state = HostUiState::default();
        state.add_slot(PathBuf::from("/p/A.vst3"));
        state.set_notice("transient error");

        let session = state.to_session();
        let mut restored = HostUiState::default();
        restored.load_session(&session);

        // The notice is transient: it never travels through a saved session.
        assert!(restored.notice.is_none());
        assert_eq!(restored.chain.len(), 1);
    }

    #[test]
    fn catalog_partitions_compatible_from_incompatible() {
        let catalog = PluginCatalog::from_probes(vec![
            (PathBuf::from("/p/C.vst3"), Ok(())),
            (PathBuf::from("/p/A.vst3"), Err("NoAudioClass".to_string())),
            (PathBuf::from("/p/B.vst3"), Ok(())),
        ]);

        // Sorted by path.
        let paths: Vec<&str> = catalog
            .entries
            .iter()
            .map(|entry| entry.path.to_str().unwrap())
            .collect();
        assert_eq!(paths, ["/p/A.vst3", "/p/B.vst3", "/p/C.vst3"]);

        // A is incompatible with its reason; B and C are offered for adding.
        assert!(!catalog.entries[0].compatible);
        assert_eq!(catalog.entries[0].reason.as_deref(), Some("NoAudioClass"));
        let compatible: Vec<String> = catalog.compatible().map(|e| e.name.clone()).collect();
        assert_eq!(compatible, ["B", "C"]);
    }

    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration (touches the filesystem)"
    )]
    #[test]
    fn scan_dir_lists_only_vst3_entries() {
        let dir = std::env::temp_dir().join(format!("galad_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("a.vst3"), b"").expect("write a");
        std::fs::write(dir.join("b.txt"), b"").expect("write b");
        std::fs::write(dir.join("c.vst3"), b"").expect("write c");

        let found = HostUiState::scan_dir(&dir);
        let names: Vec<&str> = found
            .iter()
            .filter_map(|path| path.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["a.vst3", "c.vst3"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
