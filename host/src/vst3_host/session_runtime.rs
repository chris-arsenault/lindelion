//! Session capture/restore orchestration — bridge a live chain (paths + components + bypass) and a
//! serializable [`HostSession`]. `capture_session` reads each plugin's opaque state out;
//! `restore_chain` loads the modules back, instantiates, restores state, and rebuilds the chain.

use std::path::PathBuf;

use vst3::ComPtr;
use vst3::Steinberg::Vst::{IComponent, IHostApplication};

use super::chain::ChainProcessor;
use super::instance::{HostError, PluginInstance};
use super::module::{LoadedModule, load_module};
use super::state::{capture_state, restore_state};
use crate::session::{
    AppSettings, ChainSlot, DeviceRef, HostSession, PluginStateBlob, SESSION_FORMAT_VERSION,
};

/// One live slot to capture: where the plugin was loaded from, its component, and its bypass state.
pub struct SessionSlot<'a> {
    pub plugin_path: PathBuf,
    pub component: &'a ComPtr<IComponent>,
    pub bypassed: bool,
}

/// Build a [`HostSession`] from the live `slots` + selected devices + settings, capturing each
/// plugin's opaque state into the session.
pub fn capture_session(
    slots: &[SessionSlot],
    input: Option<DeviceRef>,
    output: Option<DeviceRef>,
    settings: AppSettings,
) -> HostSession {
    let chain = slots
        .iter()
        .map(|slot| ChainSlot {
            plugin_path: slot.plugin_path.clone(),
            bypassed: slot.bypassed,
            state: Some(PluginStateBlob {
                format_version: SESSION_FORMAT_VERSION,
                payload: capture_state(slot.component),
            }),
        })
        .collect();
    HostSession {
        input,
        output,
        chain,
        settings,
    }
}

/// Rebuild a chain from a [`HostSession`]: load each module, instantiate, restore its state, and
/// build the [`ChainProcessor`]. Returns the loaded modules (which must outlive the chain — they own
/// the DLLs) together with the chain.
pub fn restore_chain(
    session: &HostSession,
    host: &ComPtr<IHostApplication>,
    sample_rate: f64,
    max_frames: usize,
) -> Result<(Vec<LoadedModule>, ChainProcessor), HostError> {
    let mut modules = Vec::with_capacity(session.chain.len());
    let mut instances = Vec::with_capacity(session.chain.len());
    let mut bypass = Vec::with_capacity(session.chain.len());

    for slot in &session.chain {
        let module = load_module(&slot.plugin_path)?;
        let instance = PluginInstance::from_factory(module.factory(), host)?;
        if let Some(blob) = &slot.state {
            restore_state(instance.component(), &blob.payload);
        }
        instances.push(instance);
        bypass.push(slot.bypassed);
        modules.push(module);
    }

    let chain = ChainProcessor::new(instances, bypass, sample_rate, max_frames)?;
    Ok((modules, chain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::HostContext;
    use crate::vst3_host::fixture::fixture_factory;
    use vst3::Steinberg::Vst::IHostApplication;

    fn fixture_instance() -> PluginInstance {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        PluginInstance::from_factory(&factory, &host).expect("instance")
    }

    #[test]
    fn capture_session_records_paths_bypass_and_state() {
        let a = fixture_instance();
        let b = fixture_instance();
        restore_state(a.component(), b"state-A");
        restore_state(b.component(), b"state-B");

        let slots = vec![
            SessionSlot {
                plugin_path: PathBuf::from("/p/A.vst3"),
                component: a.component(),
                bypassed: false,
            },
            SessionSlot {
                plugin_path: PathBuf::from("/p/B.vst3"),
                component: b.component(),
                bypassed: true,
            },
        ];
        let session = capture_session(&slots, None, None, AppSettings::default());

        assert_eq!(session.chain.len(), 2);
        assert_eq!(session.chain[0].plugin_path, PathBuf::from("/p/A.vst3"));
        assert!(!session.chain[0].bypassed);
        assert_eq!(session.chain[0].state.as_ref().unwrap().payload, b"state-A");
        assert!(session.chain[1].bypassed);
        assert_eq!(session.chain[1].state.as_ref().unwrap().payload, b"state-B");
    }

    #[test]
    fn session_state_round_trips_through_disk_into_fresh_plugins() {
        let a = fixture_instance();
        let b = fixture_instance();
        restore_state(a.component(), b"alpha");
        restore_state(b.component(), b"beta");

        let slots = vec![
            SessionSlot {
                plugin_path: PathBuf::from("/p/A.vst3"),
                component: a.component(),
                bypassed: false,
            },
            SessionSlot {
                plugin_path: PathBuf::from("/p/B.vst3"),
                component: b.component(),
                bypassed: true,
            },
        ];
        let session = capture_session(&slots, None, None, AppSettings::default());

        let path =
            std::env::temp_dir().join(format!("galad-session-rt-{}.toml", std::process::id()));
        session.save(&path).expect("save");
        let loaded = HostSession::load(&path).expect("load");
        let _ = std::fs::remove_file(&path);

        // Restore each captured state into a fresh plugin and verify it matches exactly.
        for slot in &loaded.chain {
            let fresh = fixture_instance();
            let payload = &slot.state.as_ref().expect("state").payload;
            restore_state(fresh.component(), payload);
            assert_eq!(&capture_state(fresh.component()), payload);
        }
    }
}
