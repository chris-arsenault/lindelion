#[derive(Debug, Clone, Copy)]
pub struct LinnodEditorCallbacks {
    pub parameter_value: unsafe fn(usize, u32) -> f32,
    pub set_parameter: unsafe fn(usize, u32, f64),
    pub parameter_value_text: unsafe fn(usize, u32, f64) -> String,
    pub default_normalized: unsafe fn(usize, u32) -> f32,
    pub status: unsafe fn(usize) -> LinnodEditorStatus,
    pub telemetry: unsafe fn(usize) -> LinnodEditorTelemetry,
    pub summary: unsafe fn(usize) -> LinnodEditorPatchSummary,
    pub directories: unsafe fn(usize) -> LinnodEditorDirectories,
    pub request_status: unsafe fn(usize),
    pub request_telemetry: unsafe fn(usize),
    pub handle_command: for<'a> unsafe fn(usize, LinnodEditorCommandRequest<'a>),
    pub edit_marker: unsafe fn(usize, LinnodEditorMarkerEdit),
    pub edit_pad: unsafe fn(usize, LinnodEditorPadEdit),
    pub edit_playback: unsafe fn(usize, LinnodEditorPlaybackEdit),
    pub edit_auto_tune: unsafe fn(usize, LinnodEditorAutoTuneEdit),
    pub edit_detection: unsafe fn(usize, LinnodEditorDetectionEdit),
    pub edit_slice: unsafe fn(usize, LinnodEditorSliceEdit),
}

#[derive(Debug, Clone, Copy)]
pub struct LinnodEditorHost {
    context: usize,
    surface: CompleteSurfaceHost<
        LinnodEditorSurfaceSlot,
        LinnodEditorParameterBinding,
        LINNOD_EDITOR_PARAMETER_BINDING_COUNT,
    >,
    callbacks: LinnodEditorCallbacks,
}

impl LinnodEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = LinnodEditorParameterBinding>,
        callbacks: LinnodEditorCallbacks,
    ) -> Result<Self, LinnodEditorHostError> {
        Ok(Self {
            context,
            surface: CompleteSurfaceHost::new(bindings)?,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<LinnodEditorParameterBinding>; LINNOD_EDITOR_PARAMETER_BINDING_COUNT] {
        self.surface.parameter_bindings()
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value(self, id: u32) -> f32 {
        unsafe { (self.callbacks.parameter_value)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn set_parameter(self, id: u32, normalized: f64) {
        unsafe { (self.callbacks.set_parameter)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value_text(self, id: u32, normalized: f64) -> String {
        unsafe { (self.callbacks.parameter_value_text)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn default_normalized(self, id: u32) -> f32 {
        unsafe { (self.callbacks.default_normalized)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn status(self) -> LinnodEditorStatus {
        unsafe { (self.callbacks.status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn telemetry(self) -> LinnodEditorTelemetry {
        unsafe { (self.callbacks.telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn summary(self) -> LinnodEditorPatchSummary {
        unsafe { (self.callbacks.summary)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn directories(self) -> LinnodEditorDirectories {
        unsafe { (self.callbacks.directories)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_status(self) {
        unsafe { (self.callbacks.request_status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_telemetry(self) {
        unsafe { (self.callbacks.request_telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn handle_command(self, request: LinnodEditorCommandRequest<'_>) {
        unsafe { (self.callbacks.handle_command)(self.context, request) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_marker(self, edit: LinnodEditorMarkerEdit) {
        unsafe { (self.callbacks.edit_marker)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_pad(self, edit: LinnodEditorPadEdit) {
        unsafe { (self.callbacks.edit_pad)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_playback(self, edit: LinnodEditorPlaybackEdit) {
        unsafe { (self.callbacks.edit_playback)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_auto_tune(self, edit: LinnodEditorAutoTuneEdit) {
        unsafe { (self.callbacks.edit_auto_tune)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_detection(self, edit: LinnodEditorDetectionEdit) {
        unsafe { (self.callbacks.edit_detection)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_slice(self, edit: LinnodEditorSliceEdit) {
        unsafe { (self.callbacks.edit_slice)(self.context, edit) }
    }
}
