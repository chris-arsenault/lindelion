use vst3::Steinberg::{kInvalidArgument, kResultFalse, tresult};

use super::super::{
    messages::LinnodDetectionEditMessage, patch_edits::apply_detection_edit_message,
};
use super::LinnodVst3Processor;

impl LinnodVst3Processor {
    pub(super) fn apply_detection_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodDetectionEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !apply_detection_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        plugin.set_patch_redetecting_source(patch);
        drop(plugin);
        self.send_patch_update()
    }
}
