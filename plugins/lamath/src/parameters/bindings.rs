#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterApplyKind {
    Live,
    Structural(StructuralChangePolicy),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParameterTarget {
    None,
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

    pub(crate) fn spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec {
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParameterBinding {
    info: ParameterInfo,
    path: ParameterPath,
    apply_kind: ParameterApplyKind,
    runtime_target: RuntimeParameterTarget,
    smoothing: Option<RuntimeSmoothing>,
    formatter: ParameterFormatter,
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    editor: Option<EditorParameterBinding>,
}

impl ParameterBinding {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        info: ParameterInfo,
        path: ParameterPath,
        apply_kind: ParameterApplyKind,
        runtime_target: RuntimeParameterTarget,
        smoothing: Option<RuntimeSmoothing>,
        formatter: ParameterFormatter,
        editor: Option<EditorParameterBinding>,
    ) -> Self {
        Self {
            info,
            path,
            apply_kind,
            runtime_target,
            smoothing,
            formatter,
            editor,
        }
    }

    pub(crate) const fn info(self) -> ParameterInfo {
        self.info
    }

    pub(crate) const fn id(self) -> ParameterId {
        self.info.id
    }

    pub(crate) const fn runtime_target(self) -> RuntimeParameterTarget {
        self.runtime_target
    }

    pub(crate) fn smoothed_atomic_spec(self) -> Option<SmoothedAtomicParamSpec> {
        self.smoothing.map(|smoothing| smoothing.spec(self.info))
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn editor(self) -> Option<EditorParameterBinding> {
        self.editor
    }

    pub(crate) fn plain_value(self, patch: &ResonatorSynthPatch) -> f32 {
        self.path.plain_value(patch)
    }

    pub(crate) fn apply_plain(
        self,
        patch: &mut ResonatorSynthPatch,
        value: f32,
    ) -> ParameterApplyKind {
        self.path.apply_plain(patch, value);
        self.apply_kind
    }

    pub(crate) fn format_plain_value(self, value: f32) -> String {
        self.formatter.format(value)
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
            control: EditorControlKind::Slider,
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

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn label(self) -> &'static str {
        self.label
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) const fn control(self) -> EditorControlKind {
        self.control
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
}

impl EditorSurfaceGroup {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const REQUIRED: [Self; 10] = [
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
    ];
}

#[derive(Debug, Clone, Copy)]
enum ParameterFormatter {
    Plain,
    Label(fn(f32) -> &'static str),
}

impl ParameterFormatter {
    fn format(self, value: f32) -> String {
        match self {
            Self::Plain => format_plain_value(value),
            Self::Label(label) => label(value).to_string(),
        }
    }
}
