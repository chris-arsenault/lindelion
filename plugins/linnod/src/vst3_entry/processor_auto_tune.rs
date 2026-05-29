use super::super::{
    messages::LinnodAutoTuneEditMessage, patch_edits::apply_auto_tune_edit_message,
};
use super::*;

impl LinnodVst3Processor {
    pub(super) fn apply_auto_tune_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodAutoTuneEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !apply_auto_tune_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        plugin.set_patch_preserving_source_analysis(patch);
        drop(plugin);
        self.send_patch_update()
    }
}
