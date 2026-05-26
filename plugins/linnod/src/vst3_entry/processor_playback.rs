use super::super::{messages::LinnodPlaybackEditMessage, patch_edits::apply_playback_edit_message};
use super::*;

impl LinnodVst3Processor {
    pub(super) fn apply_playback_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodPlaybackEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !apply_playback_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        plugin.set_patch_preserving_source_analysis(patch);
        drop(plugin);
        self.send_patch_update()
    }
}
