const AUDIO_INPUT_MODE_EDITOR_LABELS: &[&str] = &["Off", "Audio Notes", "MIDI + Audio"];
const LIVE_EXCITATION_MODE_EDITOR_LABELS: &[&str] = &["Off", "Cont", "Latch", "Both"];
const ROUTING_MODE_EDITOR_LABELS: &[&str] = &["Parallel", "Series", "Body"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterApplyKind {
    Live,
    Structural(StructuralChangePolicy),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParameterTarget {
    None,
    Patch,
    Output,
    Routing,
}

impl RuntimeParameterTarget {
    pub(crate) const fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RuntimeSmoothing {
    Identity {
        smoothing_ms: f32,
        epsilon: f32,
    },
    Mapped {
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: fn(f32) -> f32,
    },
}

impl RuntimeSmoothing {
    pub(crate) const fn identity(smoothing_ms: f32, epsilon: f32) -> Self {
        Self::Identity {
            smoothing_ms,
            epsilon,
        }
    }

    pub(crate) const fn mapped(
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: fn(f32) -> f32,
    ) -> Self {
        Self::Mapped {
            smoothed,
            plain_to_smoothed,
        }
    }

    fn spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
        match self {
            Self::Identity {
                smoothing_ms,
                epsilon,
            } => SmoothedAtomicParamSpec::from_parameter(info, smoothing_ms, epsilon),
            Self::Mapped {
                smoothed,
                plain_to_smoothed,
            } => SmoothedAtomicParamSpec::mapped(info, smoothed, plain_to_smoothed),
        }
    }
}

impl ParameterSmoothingSpec for RuntimeSmoothing {
    fn smoothed_atomic_spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
        self.spec(info)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorParameterBinding {
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    slot: EditorSurfaceSlot,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    group: EditorSurfaceGroup,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    order: u8,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    label: &'static str,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    control: EditorControlKind,
}

impl EditorParameterBinding {
    pub(crate) const fn knob(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Knob,
        }
    }

    pub(crate) const fn slider(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::slider(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn binary(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Binary {
                left_label,
                right_label,
                width,
            },
        }
    }

    pub(crate) const fn segmented(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
        labels: &'static [&'static str],
        width: f32,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Segmented { labels, width },
        }
    }

    pub(crate) const fn selector(
        slot: EditorSurfaceSlot,
        group: EditorSurfaceGroup,
        order: u8,
        label: &'static str,
        labels: &'static [&'static str],
        width: f32,
    ) -> Self {
        Self {
            slot,
            group,
            order,
            label,
            control: EditorControlKind::Selector { labels, width },
        }
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn slot(self) -> EditorSurfaceSlot {
        self.slot
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn group(self) -> EditorSurfaceGroup {
        self.group
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn order(self) -> u8 {
        self.order
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn label(self) -> &'static str {
        self.label
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn control(self) -> EditorControlKind {
        self.control
    }
}

impl
    ParameterEditorBindingProjection<
        lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding,
    > for EditorParameterBinding
{
    fn project_editor_binding(
        self,
        id: ParameterId,
    ) -> lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding {
        lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding::new(
            id.0,
            self.slot,
            self.label,
            self.control,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorSurfaceGroup {
    ResonatorHeader,
    ResonatorAHeader,
    ResonatorAControls,
    ResonatorBHeader,
    ResonatorBControls,
    Routing,
    OutputKnobs,
    OutputFilter,
    OutputEnvelope,
    OutputModulation,
    LiveInput,
    NoteDetection,
    AudioExpression,
    LiveExcitation,
}

impl EditorSurfaceGroup {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const REQUIRED: [Self; 14] = [
        Self::ResonatorHeader,
        Self::ResonatorAHeader,
        Self::ResonatorAControls,
        Self::ResonatorBHeader,
        Self::ResonatorBControls,
        Self::Routing,
        Self::OutputKnobs,
        Self::OutputFilter,
        Self::OutputEnvelope,
        Self::OutputModulation,
        Self::LiveInput,
        Self::NoteDetection,
        Self::AudioExpression,
        Self::LiveExcitation,
    ];
}
