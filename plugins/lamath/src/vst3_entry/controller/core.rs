pub(super) struct ResonatorVst3Controller {
    pub(super) values: Vst3ParameterMirror<VST3_PARAMETER_COUNT>,
    pub(super) handler: Cell<*mut IComponentHandler>,
    pub(super) editor_summary: RefCell<EditorPatchSummary>,
    pub(super) patch: RefCell<ResonatorSynthPatch>,
    peer: Vst3PeerConnection,
    telemetry: Cell<EditorTelemetry>,
    library_samples: RefCell<Vec<SampleMetadata>>,
}

impl Class for ResonatorVst3Controller {
    type Interfaces = (IEditController, IMidiMapping, IConnectionPoint);
}

impl ResonatorVst3Controller {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.controller_cid[0],
        crate::VST3_BUNDLE_METADATA.controller_cid[1],
        crate::VST3_BUNDLE_METADATA.controller_cid[2],
        crate::VST3_BUNDLE_METADATA.controller_cid[3],
    );

    pub(super) fn new() -> Self {
        Self {
            values: Vst3ParameterMirror::new(default_parameter_values()),
            handler: Cell::new(ptr::null_mut()),
            editor_summary: RefCell::new(EditorPatchSummary::from_patch(
                &crate::ResonatorSynthPatch::default(),
            )),
            patch: RefCell::new(ResonatorSynthPatch::default()),
            peer: Vst3PeerConnection::new(),
            telemetry: Cell::new(EditorTelemetry::default()),
            library_samples: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let Some(value) =
            self.values
                .set_normalized(index, normalized, default_parameter_values()[index])
        else {
            return kInvalidArgument;
        };
        if id != PITCH_BEND_PARAMETER_ID
            && crate::apply_parameter_normalized_for_controller(
                &mut self.patch.borrow_mut(),
                id,
                value as f32,
            )
        {
            self.editor_summary
                .replace(EditorPatchSummary::from_patch_and_library(
                    &self.patch.borrow(),
                    &self.library_samples.borrow(),
                ));
        }
        kResultOk
    }

    pub(super) fn editor_summary(&self) -> EditorPatchSummary {
        self.editor_summary.borrow().clone()
    }

    pub(super) fn telemetry(&self) -> EditorTelemetry {
        self.telemetry.get()
    }

    fn replace_patch_mirror(&self, patch: ResonatorSynthPatch) {
        self.values.replace(parameter_values_from_patch(&patch));
        self.patch.replace(patch);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
    }

    fn notify_parameter_values_changed(&self) {
        unsafe {
            restart_vst3_parameter_values_changed(self.handler.get());
        }
    }

    fn send_patch_to_processor(&self) -> tresult {
        let payload_patch = processor_patch_from_controller_patch(&self.patch.borrow());
        notify_vst3_patch_update(
            &self.peer,
            &payload_patch,
            patch_io::to_toml_string,
            |payload| ResonatorPluginMessage::patch_update(payload).into_com_message(),
        )
    }

    pub(super) fn request_telemetry(&self) {
        let _ = self
            .peer
            .notify(ResonatorPluginMessage::telemetry_request().into_com_message());
    }

    pub(super) fn reset_audio_engine(&self) -> tresult {
        self.peer
            .notify(ResonatorPluginMessage::reset_audio_engine().into_com_message())
    }

    pub(super) fn save_patch_to_path(&self, path: &Path) -> Result<(), patch_io::PatchIoError> {
        patch_io::save_patch(path, &self.patch.borrow())
    }

    pub(super) fn load_patch_from_path(
        &self,
        path: &Path,
    ) -> Result<tresult, patch_io::PatchIoError> {
        let mut patch = patch_io::load_patch(path)?;
        resolve_patch_samples_for_loaded_path(&mut patch, path);
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        Ok(self.send_patch_to_processor())
    }

    pub(super) fn export_patch_bundle(&self, directory: &Path) -> io::Result<PathBuf> {
        export_patch_bundle(directory, &self.patch.borrow())
    }

    pub(super) fn refresh_library(&self) -> io::Result<()> {
        let samples = open_default_sample_library()
            .map_err(io::Error::other)?
            .list_samples()
            .map_err(io::Error::other)?;
        self.library_samples.replace(samples);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
        Ok(())
    }

    pub(super) fn ingest_sample(&self, path: PathBuf) -> io::Result<SampleReference> {
        let mut library = open_default_sample_library().map_err(io::Error::other)?;
        let metadata = library.ingest(path).map_err(io::Error::other)?;
        let reference = metadata.reference.clone();
        self.refresh_library()?;
        Ok(reference)
    }

    pub(super) fn assign_library_sample_to_slot(
        &self,
        sample_index: usize,
        slot_index: usize,
    ) -> tresult {
        let Some(metadata) = self.library_samples.borrow().get(sample_index).cloned() else {
            return kInvalidArgument;
        };
        self.assign_sample_reference_to_slot(metadata.reference, slot_index)
    }

    pub(super) fn assign_sample_reference_to_slot(
        &self,
        reference: SampleReference,
        slot_index: usize,
    ) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = Some(reference);
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }

    pub(super) fn clear_slot(&self, slot_index: usize) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = None;
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }
}

impl IConnectionPointTrait for ResonatorVst3Controller {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.connect(other)
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.disconnect(other)
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        let message = match ResonatorPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };
        match message {
            ResonatorPluginMessage::TelemetryResponse(payload) => {
                let Some(telemetry) = decode_telemetry(&payload) else {
                    return kResultFalse;
                };
                self.telemetry.set(telemetry);
                kResultOk
            }
            ResonatorPluginMessage::PatchUpdate(_)
            | ResonatorPluginMessage::TelemetryRequest
            | ResonatorPluginMessage::ResetAudioEngine => kNotImplemented,
        }
    }
}
