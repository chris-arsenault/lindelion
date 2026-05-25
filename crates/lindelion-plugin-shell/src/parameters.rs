#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParameterRange {
    pub const fn linear(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }

    pub fn normalize(self, value: f32) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }

        let value = if value.is_finite() {
            value
        } else {
            self.default
        };
        ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }

    pub fn denormalize(self, normalized: f32) -> f32 {
        let normalized = if normalized.is_finite() {
            normalized
        } else {
            self.normalize(self.default)
        };
        self.min + normalized.clamp(0.0, 1.0) * (self.max - self.min)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterFlags {
    pub automatable: bool,
    pub read_only: bool,
}

impl ParameterFlags {
    pub const AUTOMATABLE: Self = Self {
        automatable: true,
        read_only: false,
    };

    pub const READ_ONLY: Self = Self {
        automatable: false,
        read_only: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterInfo {
    pub id: ParameterId,
    pub name: &'static str,
    pub units: &'static str,
    pub range: ParameterRange,
    pub step_count: Option<u32>,
    pub flags: ParameterFlags,
}

impl ParameterInfo {
    pub const fn continuous(
        id: u32,
        name: &'static str,
        units: &'static str,
        range: ParameterRange,
    ) -> Self {
        Self {
            id: ParameterId(id),
            name,
            units,
            range,
            step_count: None,
            flags: ParameterFlags::AUTOMATABLE,
        }
    }

    pub const fn stepped(
        id: u32,
        name: &'static str,
        units: &'static str,
        range: ParameterRange,
        step_count: u32,
    ) -> Self {
        Self {
            id: ParameterId(id),
            name,
            units,
            range,
            step_count: Some(step_count),
            flags: ParameterFlags::AUTOMATABLE,
        }
    }
}

pub trait ParameterPatchPath<Patch>: Copy {
    fn plain_value(self, patch: &Patch) -> f32;
    fn apply_plain(self, patch: &mut Patch, value: f32);
}

pub trait ParameterSmoothingSpec: Copy {
    fn smoothed_atomic_spec(self, info: ParameterInfo) -> SmoothedAtomicParamSpec;
}

pub trait ParameterValueFormatter: Copy {
    fn format_plain_value(self, value: f32) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterApplyOutcome<ApplyKind, RuntimeTarget> {
    pub id: ParameterId,
    pub normalized: f32,
    pub plain: f32,
    pub apply_kind: ApplyKind,
    pub runtime_target: RuntimeTarget,
}

pub trait ParameterApplyDispatcher<Patch, ApplyKind, RuntimeTarget> {
    fn handle_parameter_apply(
        &mut self,
        patch: &mut Patch,
        outcome: ParameterApplyOutcome<ApplyKind, RuntimeTarget>,
    );
}

pub trait ParameterEditorBindingProjection<EditorBinding>: Copy {
    fn project_editor_binding(self, id: ParameterId) -> EditorBinding;
}

#[derive(Debug, Clone, Copy)]
pub enum ParameterFormatter {
    Plain,
    Label(fn(f32) -> &'static str),
}

impl ParameterFormatter {
    pub const fn plain() -> Self {
        Self::Plain
    }

    pub const fn label(label: fn(f32) -> &'static str) -> Self {
        Self::Label(label)
    }
}

impl ParameterValueFormatter for ParameterFormatter {
    fn format_plain_value(self, value: f32) -> String {
        match self {
            Self::Plain => format_plain_value(value),
            Self::Label(label) => label(value).to_string(),
        }
    }
}

pub fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

pub trait ParameterCodec: Copy + Sized {
    const MAX_INDEX: u32;
    const LABELS: &'static [&'static str];

    fn from_index(index: u32) -> Self;
    fn to_index(self) -> u32;
    fn label(self) -> &'static str;

    fn from_plain(value: f32) -> Self {
        Self::from_index(stepped_parameter_index(value, Self::MAX_INDEX))
    }

    fn plain(self) -> f32 {
        self.to_index() as f32
    }

    fn label_from_plain(value: f32) -> &'static str {
        Self::from_plain(value).label()
    }
}

pub fn stepped_parameter_index(value: f32, max: u32) -> u32 {
    let value = if value.is_finite() { value } else { 0.0 };
    value.clamp(0.0, max as f32).round() as u32
}

#[derive(Debug, Clone, Copy)]
pub struct ParameterBinding<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor> {
    info: ParameterInfo,
    path: Path,
    apply_kind: ApplyKind,
    runtime_target: RuntimeTarget,
    smoothing: Option<Smoothing>,
    formatter: Formatter,
    editor: Option<Editor>,
}

impl<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>
    ParameterBinding<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>
{
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        info: ParameterInfo,
        path: Path,
        apply_kind: ApplyKind,
        runtime_target: RuntimeTarget,
        smoothing: Option<Smoothing>,
        formatter: Formatter,
        editor: Option<Editor>,
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

    pub fn info(self) -> ParameterInfo {
        self.info
    }

    pub fn id(self) -> ParameterId {
        self.info.id
    }

    pub fn path(self) -> Path {
        self.path
    }

    pub fn apply(self) -> ApplyKind {
        self.apply_kind
    }

    pub fn apply_kind(self) -> ApplyKind {
        self.apply_kind
    }

    pub fn runtime_target(self) -> RuntimeTarget {
        self.runtime_target
    }

    pub fn smoothing(self) -> Option<Smoothing> {
        self.smoothing
    }

    pub fn formatter(self) -> Formatter {
        self.formatter
    }

    pub fn editor(self) -> Option<Editor> {
        self.editor
    }

    pub fn plain_value<Patch>(self, patch: &Patch) -> f32
    where
        Path: ParameterPatchPath<Patch>,
    {
        self.path.plain_value(patch)
    }

    pub fn apply_plain<Patch>(self, patch: &mut Patch, value: f32) -> ApplyKind
    where
        Path: ParameterPatchPath<Patch>,
    {
        let value = self
            .info
            .range
            .denormalize(self.info.range.normalize(value));
        self.path.apply_plain(patch, value);
        self.apply_kind
    }

    pub fn smoothed_atomic_spec(self) -> Option<SmoothedAtomicParamSpec>
    where
        Smoothing: ParameterSmoothingSpec,
    {
        self.smoothing
            .map(|smoothing| smoothing.smoothed_atomic_spec(self.info))
    }

    pub fn format_plain_value(self, value: f32) -> String
    where
        Formatter: ParameterValueFormatter,
    {
        self.formatter.format_plain_value(value)
    }
}

pub trait ParameterBindingMetadata: Copy {
    fn info(self) -> ParameterInfo;

    fn id(self) -> ParameterId {
        self.info().id
    }
}

impl<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor> ParameterBindingMetadata
    for ParameterBinding<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>
where
    Self: Copy,
{
    fn info(self) -> ParameterInfo {
        self.info
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ParameterRegistry<B: 'static> {
    bindings: &'static [B],
}

impl<B> ParameterRegistry<B>
where
    B: ParameterBindingMetadata + 'static,
{
    pub const fn new(bindings: &'static [B]) -> Self {
        Self { bindings }
    }

    pub const fn bindings(self) -> &'static [B] {
        self.bindings
    }

    pub const fn len(self) -> usize {
        self.bindings.len()
    }

    pub const fn is_empty(self) -> bool {
        self.bindings.is_empty()
    }

    pub fn binding(self, id: u32) -> Option<&'static B> {
        self.bindings
            .iter()
            .find(|binding| binding.id() == ParameterId(id))
    }

    pub fn binding_by_index(self, index: usize) -> Option<&'static B> {
        self.bindings.get(index)
    }

    pub fn binding_index(self, id: u32) -> Option<usize> {
        self.bindings
            .iter()
            .position(|binding| binding.id() == ParameterId(id))
    }

    pub fn info(self, id: u32) -> Option<ParameterInfo> {
        self.binding(id).map(|binding| binding.info())
    }

    pub fn default_normalized_value_by_index(self, index: usize) -> Option<f32> {
        self.binding_by_index(index)
            .map(|binding| binding.info().range)
            .map(|range| range.normalize(range.default))
    }

    pub fn default_normalized_values<const N: usize>(self) -> [f64; N] {
        let mut values = [0.0; N];
        for (index, binding) in self.bindings.iter().enumerate().take(N) {
            let range = binding.info().range;
            values[index] = f64::from(range.normalize(range.default));
        }
        values
    }

    pub fn normalized_value(self, id: u32, plain: f32) -> Option<f32> {
        self.info(id)
            .map(|parameter| parameter.range.normalize(plain))
    }

    pub fn denormalized_value(self, id: u32, normalized: f32) -> Option<f32> {
        self.info(id)
            .map(|parameter| parameter.range.denormalize(normalized))
    }
}

impl<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>
    ParameterRegistry<
        ParameterBinding<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>,
    >
where
    ParameterBinding<Path, ApplyKind, RuntimeTarget, Smoothing, Formatter, Editor>: Copy + 'static,
{
    pub fn editor_bindings(
        self,
    ) -> impl Iterator<
        Item = &'static ParameterBinding<
            Path,
            ApplyKind,
            RuntimeTarget,
            Smoothing,
            Formatter,
            Editor,
        >,
    > {
        self.bindings
            .iter()
            .filter(|binding| binding.editor().is_some())
    }

    pub fn projected_editor_bindings<EditorBinding>(self) -> impl Iterator<Item = EditorBinding>
    where
        Editor: ParameterEditorBindingProjection<EditorBinding>,
    {
        self.bindings.iter().filter_map(|binding| {
            binding
                .editor()
                .map(|editor| editor.project_editor_binding(binding.id()))
        })
    }

    pub fn patch_plain_value<Patch>(self, patch: &Patch, id: u32) -> Option<f32>
    where
        Path: ParameterPatchPath<Patch>,
    {
        self.binding(id).map(|binding| binding.plain_value(patch))
    }

    pub fn normalized_patch_value<Patch>(self, patch: &Patch, id: u32) -> Option<f32>
    where
        Path: ParameterPatchPath<Patch>,
    {
        let binding = self.binding(id)?;
        Some(binding.info().range.normalize(binding.plain_value(patch)))
    }

    pub fn normalized_patch_values<Patch, const N: usize>(
        self,
        patch: &Patch,
        mut values: [f64; N],
    ) -> [f64; N]
    where
        Path: ParameterPatchPath<Patch>,
    {
        for (index, binding) in self.bindings.iter().enumerate().take(N) {
            let binding = *binding;
            values[index] = f64::from(binding.info().range.normalize(binding.plain_value(patch)));
        }
        values
    }

    pub fn apply_plain<Patch>(self, patch: &mut Patch, id: u32, value: f32) -> Option<ApplyKind>
    where
        Path: ParameterPatchPath<Patch>,
    {
        self.binding(id)
            .map(|binding| binding.apply_plain(patch, value))
    }

    pub fn apply_normalized<Patch>(
        self,
        patch: &mut Patch,
        id: u32,
        normalized: f32,
    ) -> Option<ParameterApplyOutcome<ApplyKind, RuntimeTarget>>
    where
        Path: ParameterPatchPath<Patch>,
    {
        let binding = *self.binding(id)?;
        let parameter = binding.info();
        let plain = parameter.range.denormalize(normalized);
        let normalized = parameter.range.normalize(plain);
        let apply_kind = binding.apply_plain(patch, plain);

        Some(ParameterApplyOutcome {
            id: parameter.id,
            normalized,
            plain,
            apply_kind,
            runtime_target: binding.runtime_target(),
        })
    }

    pub fn dispatch_normalized<Patch, Dispatcher>(
        self,
        patch: &mut Patch,
        id: u32,
        normalized: f32,
        dispatcher: &mut Dispatcher,
    ) -> Option<ParameterApplyOutcome<ApplyKind, RuntimeTarget>>
    where
        Path: ParameterPatchPath<Patch>,
        ApplyKind: Copy,
        RuntimeTarget: Copy,
        Dispatcher: ParameterApplyDispatcher<Patch, ApplyKind, RuntimeTarget>,
    {
        let outcome = self.apply_normalized(patch, id, normalized)?;
        dispatcher.handle_parameter_apply(patch, outcome);
        Some(outcome)
    }

    pub fn smoothed_atomic_param(
        self,
        id: u32,
        sample_rate: f32,
        initial_plain: f32,
    ) -> Option<SmoothedAtomicParam>
    where
        Smoothing: ParameterSmoothingSpec,
    {
        let spec = self.binding(id)?.smoothed_atomic_spec()?;
        Some(SmoothedAtomicParam::with_initial_plain(
            spec,
            sample_rate,
            initial_plain,
        ))
    }

    pub fn formatted_plain_value(self, id: u32, plain: f32) -> String
    where
        Formatter: ParameterValueFormatter,
    {
        self.binding(id)
            .map(|binding| binding.format_plain_value(plain))
            .unwrap_or_else(|| format_plain_value(plain))
    }
}

mod atomic;
mod macros;

pub use atomic::{
    AtomicParameter, PlainToSmoothedValue, SmoothedAtomicParam, SmoothedAtomicParamSpec,
};

#[cfg(test)]
mod tests;
