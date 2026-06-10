//! Framework-neutral host UI state + transitions. The Vizia `Model` (M6 Step 6) wraps this, and the
//! controller (Step 7) drives it; keeping it plain Rust makes the bulk of the UI testable without
//! Vizia.

use std::path::{Path, PathBuf};

use crate::audio::{EngineStatus, MeterSnapshot};
use crate::session::{AppSettings, CachedPluginEntry, ChainSlot, DeviceRef, HostSession};

/// Minimum visible master gain in the UI.
pub const MASTER_GAIN_MIN_DB: f32 = -60.0;
/// Maximum visible master gain in the UI.
pub const MASTER_GAIN_MAX_DB: f32 = 12.0;

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
    /// Vendor reported by the VST3 factory/class metadata, if present.
    pub vendor: Option<String>,
    /// Whether the plugin passed the load-time validation probe (M7 Step 3).
    pub compatible: bool,
    /// Why it failed validation, if incompatible.
    pub reason: Option<String>,
}

/// One path probe returned by the VST3 scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProbe {
    pub path: PathBuf,
    pub vendor: Option<String>,
    pub result: Result<(), String>,
}

/// The result of scanning folders for plugins: each discovered `.vst3`, its vendor metadata, and
/// validation status. The latest scan is cached in settings so startup can show the previous catalog
/// before loading any plugin DLLs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginCatalog {
    pub entries: Vec<CatalogEntry>,
}

impl PluginCatalog {
    /// Assemble a catalog from probe records — `Ok(())` = compatible, `Err(reason)` = incompatible
    /// with a reason — sorted by path. Pure: the fs scan and probing happen in the caller.
    pub fn from_probes(probes: Vec<CatalogProbe>) -> Self {
        let mut entries: Vec<CatalogEntry> = probes
            .into_iter()
            .map(|probe| {
                let path = probe.path;
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("plugin")
                    .to_string();
                CatalogEntry {
                    path,
                    name,
                    vendor: probe.vendor.and_then(non_empty_string),
                    compatible: probe.result.is_ok(),
                    reason: probe.result.err(),
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

    /// Restore a catalog from persisted scan cache without loading/probing plugin DLLs.
    pub fn from_cached(entries: &[CachedPluginEntry]) -> Self {
        PluginCatalog {
            entries: entries
                .iter()
                .map(|entry| {
                    let name = entry
                        .path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("plugin")
                        .to_string();
                    CatalogEntry {
                        path: entry.path.clone(),
                        name,
                        vendor: entry.vendor.clone().and_then(non_empty_string),
                        compatible: entry.compatible,
                        reason: entry.reason.clone(),
                    }
                })
                .collect(),
        }
    }

    /// Convert the current catalog into persisted scan cache.
    pub fn to_cached(&self) -> Vec<CachedPluginEntry> {
        self.entries
            .iter()
            .map(|entry| CachedPluginEntry {
                path: entry.path.clone(),
                vendor: entry.vendor.clone(),
                compatible: entry.compatible,
                reason: entry.reason.clone(),
            })
            .collect()
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
    pub master_gain_db: f32,
    pub master_muted: bool,
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

    /// Remove a custom scan folder by index (system VST3 folders are implicit and not stored here).
    pub fn remove_scan_dir(&mut self, index: usize) {
        if index < self.scan_dirs.len() {
            self.scan_dirs.remove(index);
        }
    }

    /// Set the post-chain master gain, clamped to the UI-supported range.
    pub fn set_master_gain_db(&mut self, gain_db: f32) {
        self.master_gain_db = gain_db.clamp(MASTER_GAIN_MIN_DB, MASTER_GAIN_MAX_DB);
    }

    /// Set the post-chain master mute.
    pub fn set_master_muted(&mut self, muted: bool) {
        self.master_muted = muted;
    }

    /// Gather app-level settings from the current UI state.
    pub fn settings(&self) -> AppSettings {
        AppSettings {
            plugin_scan_dirs: self.scan_dirs.clone(),
            plugin_catalog: self.catalog.to_cached(),
            master_gain_db: self.master_gain_db,
            master_muted: self.master_muted,
        }
    }

    /// Apply app-level settings into the UI state.
    pub fn apply_settings(&mut self, settings: &AppSettings) {
        self.scan_dirs = settings.plugin_scan_dirs.clone();
        self.catalog = PluginCatalog::from_cached(&settings.plugin_catalog);
        self.set_master_gain_db(settings.master_gain_db);
        self.master_muted = settings.master_muted;
    }

    /// The folders Galad scans for available VST3s: Windows system folders first, then user-added
    /// custom folders from session settings.
    pub fn plugin_scan_roots(&self) -> Vec<PathBuf> {
        let mut roots = Self::system_vst3_dirs();
        for dir in &self.scan_dirs {
            push_unique_path(&mut roots, dir.clone());
        }
        roots
    }

    /// Windows system VST3 folders. These are implicit app defaults, not session settings.
    pub fn system_vst3_dirs() -> Vec<PathBuf> {
        system_vst3_dirs()
    }

    /// List the `*.vst3` entries under every configured root, sorted and deduplicated.
    pub fn scan_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for root in roots {
            scan_dir_into(root, &mut found);
        }
        found.sort();
        found.dedup();
        found
    }

    /// List the `*.vst3` entries under `dir`, sorted. Searches nested vendor folders, but does not
    /// descend into a `.vst3` bundle once it has been found.
    pub fn scan_dir(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        scan_dir_into(dir, &mut found);
        found.sort();
        found.dedup();
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
            settings: self.settings(),
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
        self.apply_settings(&session.settings);
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_string())
    }
}

fn scan_dir_into(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_vst3_path(&path) {
            found.push(path);
            continue;
        }
        if entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false) {
            scan_dir_into(&path, found);
        }
    }
}

fn is_vst3_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("vst3"))
}

#[cfg(windows)]
fn system_vst3_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_common_files_vst3(&mut dirs, "COMMONPROGRAMFILES");
    push_common_files_vst3(&mut dirs, "COMMONPROGRAMFILES(X86)");
    push_unique_path(
        &mut dirs,
        PathBuf::from(r"C:\Program Files\Common Files\VST3"),
    );
    push_unique_path(
        &mut dirs,
        PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"),
    );
    dirs
}

#[cfg(windows)]
fn push_common_files_vst3(dirs: &mut Vec<PathBuf>, env_var: &str) {
    if let Some(path) = std::env::var_os(env_var) {
        push_unique_path(dirs, PathBuf::from(path).join("VST3"));
    }
}

#[cfg(not(windows))]
fn system_vst3_dirs() -> Vec<PathBuf> {
    Vec::new()
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
        state.set_master_gain_db(-6.0);
        state.set_master_muted(true);
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
        assert_eq!(restored.settings(), state.settings());
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
        let mut state = HostUiState {
            running: true,
            ..HostUiState::default()
        };

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
            CatalogProbe {
                path: PathBuf::from("/p/C.vst3"),
                vendor: Some("Vendor C".to_string()),
                result: Ok(()),
            },
            CatalogProbe {
                path: PathBuf::from("/p/A.vst3"),
                vendor: Some("Vendor A".to_string()),
                result: Err("NoAudioClass".to_string()),
            },
            CatalogProbe {
                path: PathBuf::from("/p/B.vst3"),
                vendor: Some("Vendor B".to_string()),
                result: Ok(()),
            },
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
        assert_eq!(catalog.entries[0].vendor.as_deref(), Some("Vendor A"));
        assert_eq!(catalog.entries[0].reason.as_deref(), Some("NoAudioClass"));
        let compatible: Vec<String> = catalog.compatible().map(|e| e.name.clone()).collect();
        assert_eq!(compatible, ["B", "C"]);
    }

    #[test]
    fn catalog_cache_preserves_vendor_metadata() {
        let catalog = PluginCatalog::from_probes(vec![CatalogProbe {
            path: PathBuf::from("/p/Caloma.vst3"),
            vendor: Some("Ahara".to_string()),
            result: Ok(()),
        }]);

        let cached = catalog.to_cached();
        let restored = PluginCatalog::from_cached(&cached);

        assert_eq!(restored.entries[0].vendor.as_deref(), Some("Ahara"));
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
        std::fs::create_dir_all(dir.join("vendor")).expect("create vendor");
        std::fs::write(dir.join("vendor").join("c.vst3"), b"").expect("write c");

        let found = HostUiState::scan_dir(&dir);
        let names: Vec<&str> = found
            .iter()
            .filter_map(|path| path.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["a.vst3", "c.vst3"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
