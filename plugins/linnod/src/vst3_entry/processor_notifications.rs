use std::cell::RefCell;

use lindelion_plugin_shell::vst3::Vst3PeerConnection;
use vst3::Steinberg::*;

use crate::Linnod;

use super::{LinnodPluginMessage, controller_helpers::source_summary_payload_from_plugin};

pub(super) fn send_source_summary_update(
    plugin: &RefCell<Linnod>,
    peer: &Vst3PeerConnection,
) -> tresult {
    let Ok(plugin) = plugin.try_borrow() else {
        return kResultFalse;
    };
    let Some(summary) = source_summary_payload_from_plugin(&plugin) else {
        return kResultOk;
    };
    let Ok(payload) = summary.encode() else {
        return kResultFalse;
    };
    drop(plugin);
    peer.notify_if_connected(LinnodPluginMessage::SourceSummaryResponse(payload).into_com_message())
}
