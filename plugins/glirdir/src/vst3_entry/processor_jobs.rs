impl GlirdirVst3Processor {
    fn schedule_deferred_jobs(&self) -> tresult {
        if self.schedule_pending_analysis() == kResultFalse {
            return kResultFalse;
        }
        if self.pending_requantize.replace(false) {
            return self.schedule_pending_requantize();
        }
        kResultOk
    }

    fn schedule_pending_analysis(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = request_pending_analysis_job(&mut plugin);
        drop(plugin);
        if let Some(job) = job {
            self.pending_requantize.set(false);
            if self.worker.borrow().schedule_analysis(job) {
                kResultOk
            } else {
                kResultFalse
            }
        } else {
            kResultOk
        }
    }

    fn schedule_pending_requantize(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = plugin.request_requantize_job();
        drop(plugin);
        if let Some(job) = job
            && !self.worker.borrow().schedule_requantize(job)
        {
            return kResultFalse;
        }
        kResultOk
    }

    fn schedule_midi_export(&self) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let Some(analysis) = plugin.analysis() else {
            return kResultFalse;
        };
        let sequence = plugin.analysis_cache().sequence();
        let job = MidiExportJob::new(sequence, plugin.patch(), &analysis.midi_clip);
        drop(plugin);

        if self.worker.borrow().schedule_midi_export(job) {
            kResultOk
        } else {
            kResultFalse
        }
    }

    fn schedule_sample_library_save(&self) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let job = match SampleLibrarySaveJob::new(plugin.analysis_cache().sequence(), plugin.patch())
        {
            Ok(job) => job,
            Err(payload) => {
                drop(plugin);
                return self.send_sample_library_save_response(payload);
            }
        };
        drop(plugin);

        if self.worker.borrow().schedule_sample_library_save(job) {
            kResultOk
        } else {
            self.send_sample_library_save_response(SampleLibrarySavePayload::error(
                "worker queue unavailable",
            ))
        }
    }

    fn send_sample_library_save_response(&self, payload: SampleLibrarySavePayload) -> tresult {
        self.send_to_peer(GlirdirPluginMessage::SampleLibrarySaveResponse(
            payload.encode(),
        ))
    }

    fn drain_worker_results(&self) -> tresult {
        let mut status = kResultOk;
        self.worker
            .borrow()
            .drain_results(&mut |result| match result {
                GlirdirWorkerResult::Analysis(result) => {
                    if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
                        plugin.publish_analysis_result(result);
                    } else {
                        status = kResultFalse;
                    }
                }
                GlirdirWorkerResult::MidiExport { sequence, payload } => {
                    if self.midi_export_is_current(sequence) {
                        let result =
                            self.send_to_peer(GlirdirPluginMessage::MidiExportResponse(payload));
                        if result != kResultOk {
                            status = result;
                        }
                    }
                }
                GlirdirWorkerResult::SampleLibrarySave { sequence: _, payload } => {
                    let result =
                        self.send_to_peer(GlirdirPluginMessage::SampleLibrarySaveResponse(payload));
                    if result != kResultOk {
                        status = result;
                    }
                }
            });
        status
    }

    fn midi_export_is_current(&self, sequence: u64) -> bool {
        self.plugin
            .try_borrow()
            .map(|plugin| plugin.analysis_cache().sequence() == sequence)
            .unwrap_or(false)
    }

    fn send_status_update(&self, kind: GlirdirStatusMessage) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let status = GlirdirStatusPayload::from_plugin(&plugin);
        drop(plugin);

        match kind {
            GlirdirStatusMessage::Analysis => {
                self.send_to_peer(GlirdirPluginMessage::AnalysisStatusResponse(status))
            }
            GlirdirStatusMessage::Status => {
                self.send_to_peer(GlirdirPluginMessage::StatusResponse(status))
            }
            GlirdirStatusMessage::Telemetry => {
                self.send_to_peer(GlirdirPluginMessage::TelemetryResponse(status))
            }
        }
    }

    fn send_to_peer(&self, message: GlirdirPluginMessage) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultOk;
        };
        let message = message.into_com_message();
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlirdirStatusMessage {
    Analysis,
    Status,
    Telemetry,
}

fn request_pending_analysis_job(plugin: &mut Glirdir) -> Option<crate::AnalysisJob> {
    (plugin.analysis_status() == AnalysisStatus::CapturedPendingAnalysis)
        .then(|| plugin.request_analysis_job())
        .flatten()
}
