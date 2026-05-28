impl LinnodVst3Processor {
    pub(super) fn restore_plugin_state(&self, state: PluginState) -> tresult {
        self.restore_plugin_state_with_source_runner(state, SourceAnalysisJob::run)
    }

    pub(super) fn restore_plugin_state_with_source_runner(
        &self,
        state: PluginState,
        run_source_analysis: impl FnOnce(SourceAnalysisJob) -> SourceAnalysisJobResult,
    ) -> tresult {
        let job = {
            let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
                return kResultFalse;
            };
            plugin.load_state(state);
            plugin.request_source_load_job()
        };

        let accepted = if let Some(job) = job {
            let result = run_source_analysis(job);
            let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
                return kResultFalse;
            };
            plugin.publish_source_analysis_result(result)
        } else {
            false
        };

        if accepted {
            let patch_result = self.send_patch_update();
            if patch_result != kResultOk {
                return patch_result;
            }
            let source_result = send_source_summary_update(&self.plugin, &self.peer);
            if source_result != kResultOk {
                return source_result;
            }
            self.send_status_update(LinnodStatusMessage::Analysis)
        } else {
            self.send_status_update(LinnodStatusMessage::Status)
        }
    }
}
