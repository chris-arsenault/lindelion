use lindelion_dsp_utils::smoothing::{SmoothedParam, SmoothedParamSpec};
use std::sync::atomic::{AtomicU32, Ordering};

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

/// Defines the public host parameter list and typed registry binding list for a plugin.
///
/// Plugin-specific patch paths, apply policy, runtime target, smoothing metadata, formatter,
/// and editor metadata stay in the caller. The macro only owns the shared registry shape.
#[macro_export]
macro_rules! define_parameter_bindings {
    (
        binding: $binding:ty;
        parameters: $parameters_vis:vis const $parameters_name:ident;
        bindings: $bindings_vis:vis const $bindings_name:ident;
        defaults {
            runtime: $default_runtime:expr,
            smoothing: $default_smoothing:expr $(,)?
        }

        $(
            $info:expr => {
                path: $path:expr,
                apply: $apply:expr,
                $(runtime: $runtime:expr,)?
                $(smoothing: $smoothing:expr,)?
                format: $format:expr,
                editor: $editor:expr $(,)?
            }
        ),+ $(,)?
    ) => {
        $parameters_vis const $parameters_name: &[$crate::parameters::ParameterInfo] = &[
            $($info),+
        ];

        $bindings_vis const $bindings_name: &[$binding] = &[
            $(<$binding>::new(
                $info,
                $path,
                $apply,
                $crate::__define_parameter_bindings_runtime!($default_runtime $(, $runtime)?),
                $crate::__define_parameter_bindings_smoothing!($default_smoothing $(, $smoothing)?),
                $format,
                $editor,
            )),+
        ];
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __define_parameter_bindings_runtime {
    ($default:expr) => {
        $default
    };
    ($default:expr, $runtime:expr) => {
        $runtime
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __define_parameter_bindings_smoothing {
    ($default:expr) => {
        $default
    };
    ($default:expr, $smoothing:expr) => {
        Some($smoothing)
    };
}

/// Defines the plain-value/index/label mapping for a stepped parameter enum.
#[macro_export]
macro_rules! define_parameter_codec {
    (
        impl $trait:ident for $type:ty {
            max: $max:expr;
            fallback: $fallback:path;
            $($index:literal => $variant:path, $label:expr;)+
        }
    ) => {
        impl $trait for $type {
            const MAX_INDEX: u32 = $max;
            const LABELS: &'static [&'static str] = &[$($label),+];

            fn from_index(index: u32) -> Self {
                match index {
                    $($index => $variant,)+
                    _ => $fallback,
                }
            }

            fn to_index(self) -> u32 {
                match self {
                    $($variant => $index,)+
                }
            }

            fn label(self) -> &'static str {
                match self {
                    $($variant => $label,)+
                }
            }
        }
    };
}

#[derive(Debug)]
pub struct AtomicParameter {
    id: ParameterId,
    normalized_bits: AtomicU32,
}

impl AtomicParameter {
    pub fn new(id: ParameterId, normalized: f32) -> Self {
        Self {
            id,
            normalized_bits: AtomicU32::new(sanitize_normalized(normalized).to_bits()),
        }
    }

    pub const fn id(&self) -> ParameterId {
        self.id
    }

    pub fn load_normalized(&self) -> f32 {
        sanitize_normalized(f32::from_bits(self.normalized_bits.load(Ordering::Relaxed)))
    }

    pub fn store_normalized(&self, normalized: f32) {
        self.normalized_bits
            .store(sanitize_normalized(normalized).to_bits(), Ordering::Relaxed);
    }
}

pub type PlainToSmoothedValue = fn(f32) -> f32;

#[derive(Debug, Clone, Copy)]
pub struct SmoothedAtomicParamSpec {
    pub info: ParameterInfo,
    pub smoothed: SmoothedParamSpec,
    plain_to_smoothed: PlainToSmoothedValue,
}

impl SmoothedAtomicParamSpec {
    pub const fn from_parameter(info: ParameterInfo, smoothing_ms: f32, epsilon: f32) -> Self {
        Self::mapped(
            info,
            SmoothedParamSpec::new(
                info.range.min,
                info.range.max,
                info.range.default,
                smoothing_ms,
                epsilon,
            ),
            identity_plain_value,
        )
    }

    pub const fn mapped(
        info: ParameterInfo,
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: PlainToSmoothedValue,
    ) -> Self {
        Self {
            info,
            smoothed,
            plain_to_smoothed,
        }
    }

    pub fn smoothed_value(self, plain: f32) -> f32 {
        let normalized = self.info.range.normalize(plain);
        let plain = self.info.range.denormalize(normalized);
        self.smoothed.sanitize((self.plain_to_smoothed)(plain))
    }

    pub fn normalized_for_plain(self, plain: f32) -> f32 {
        self.info.range.normalize(plain)
    }
}

#[derive(Debug)]
pub struct SmoothedAtomicParam {
    spec: SmoothedAtomicParamSpec,
    atomic: AtomicParameter,
    smoothed: SmoothedParam,
    last_normalized_bits: u32,
}

impl SmoothedAtomicParam {
    pub fn new(spec: SmoothedAtomicParamSpec, sample_rate: f32) -> Self {
        Self::with_initial_plain(spec, sample_rate, spec.info.range.default)
    }

    pub fn with_initial_plain(spec: SmoothedAtomicParamSpec, sample_rate: f32, plain: f32) -> Self {
        let normalized = spec.normalized_for_plain(plain);
        let smoothed_value = spec.smoothed_value(plain);
        Self {
            spec,
            atomic: AtomicParameter::new(spec.info.id, normalized),
            smoothed: SmoothedParam::with_initial(spec.smoothed, sample_rate, smoothed_value),
            last_normalized_bits: normalized.to_bits(),
        }
    }

    pub const fn spec(&self) -> SmoothedAtomicParamSpec {
        self.spec
    }

    pub const fn atomic(&self) -> &AtomicParameter {
        &self.atomic
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.smoothed.set_sample_rate(sample_rate);
    }

    pub fn reset_plain(&mut self, plain: f32) {
        let normalized = self.spec.normalized_for_plain(plain);
        self.atomic.store_normalized(normalized);
        self.last_normalized_bits = normalized.to_bits();
        self.smoothed.reset(self.spec.smoothed_value(plain));
    }

    pub fn set_plain_target(&mut self, plain: f32) {
        self.atomic
            .store_normalized(self.spec.normalized_for_plain(plain));
        self.sync_from_atomic();
    }

    pub fn sync_from_atomic(&mut self) -> bool {
        let normalized = self.atomic.load_normalized();
        let normalized_bits = normalized.to_bits();
        if normalized_bits == self.last_normalized_bits {
            return false;
        }

        self.last_normalized_bits = normalized_bits;
        let plain = self.spec.info.range.denormalize(normalized);
        self.smoothed.set_target(self.spec.smoothed_value(plain));
        true
    }

    pub fn next_sample(&mut self) -> f32 {
        self.smoothed.next_sample()
    }

    pub const fn current(&self) -> f32 {
        self.smoothed.current()
    }

    pub const fn target(&self) -> f32 {
        self.smoothed.target()
    }

    pub const fn is_smoothing(&self) -> bool {
        self.smoothed.is_smoothing()
    }
}

fn identity_plain_value(value: f32) -> f32 {
    value
}

fn sanitize_normalized(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_denormalizes() {
        let range = ParameterRange::linear(-12.0, 12.0, 0.0);
        assert_eq!(range.normalize(0.0), 0.5);
        assert_eq!(range.denormalize(0.5), 0.0);
    }

    #[test]
    fn non_finite_values_fall_back_to_default() {
        let range = ParameterRange::linear(20.0, 20_000.0, 20_000.0);

        assert_eq!(range.normalize(f32::NAN), 1.0);
        assert_eq!(range.denormalize(f32::NAN), 20_000.0);
        assert_eq!(range.denormalize(f32::INFINITY), 20_000.0);
    }

    #[test]
    fn atomic_parameter_sanitizes_normalized_values() {
        let parameter = AtomicParameter::new(ParameterId(1), f32::NAN);

        assert_eq!(parameter.load_normalized(), 0.0);

        parameter.store_normalized(2.0);
        assert_eq!(parameter.load_normalized(), 1.0);
    }

    #[test]
    fn smoothed_atomic_param_round_trips_atomic_write_to_sample_accurate_ramp() {
        let info =
            ParameterInfo::continuous(1, "Target", "", ParameterRange::linear(0.0, 1.0, 0.0));
        let spec = SmoothedAtomicParamSpec::from_parameter(info, 4.0, 0.0);
        let mut parameter = SmoothedAtomicParam::new(spec, 1_000.0);

        parameter.atomic().store_normalized(1.0);

        assert!(parameter.sync_from_atomic());
        assert_eq!(parameter.target(), 1.0);
        assert_eq!(parameter.next_sample(), 0.25);
        assert_eq!(parameter.next_sample(), 0.5);
        assert_eq!(parameter.next_sample(), 0.75);
        assert_eq!(parameter.next_sample(), 1.0);
        assert_eq!(parameter.next_sample(), 1.0);
        assert!(!parameter.is_smoothing());
    }

    #[test]
    fn smoothed_atomic_param_maps_plain_values_before_smoothing() {
        fn square(value: f32) -> f32 {
            value * value
        }

        let info =
            ParameterInfo::continuous(2, "Mapped", "", ParameterRange::linear(0.0, 2.0, 1.0));
        let spec = SmoothedAtomicParamSpec::mapped(
            info,
            SmoothedParamSpec::new(0.0, 4.0, 1.0, 0.0, 0.0),
            square,
        );
        let mut parameter = SmoothedAtomicParam::new(spec, 1_000.0);

        parameter.set_plain_target(2.0);

        assert_eq!(parameter.target(), 4.0);
        assert_eq!(parameter.next_sample(), 4.0);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn parameter_registry_finds_bindings_and_applies_patch_paths() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestApply {
            Live,
        }

        #[derive(Debug, Clone, Copy)]
        enum TestPath {
            Gain,
        }

        #[derive(Debug, Default)]
        struct TestPatch {
            gain: f32,
        }

        impl ParameterPatchPath<TestPatch> for TestPath {
            fn plain_value(self, patch: &TestPatch) -> f32 {
                match self {
                    Self::Gain => patch.gain,
                }
            }

            fn apply_plain(self, patch: &mut TestPatch, value: f32) {
                match self {
                    Self::Gain => patch.gain = value,
                }
            }
        }

        type TestBinding = ParameterBinding<TestPath, TestApply, (), (), ParameterFormatter, ()>;
        const BINDINGS: &[TestBinding] = &[TestBinding::new(
            ParameterInfo::continuous(7, "Gain", "dB", ParameterRange::linear(-12.0, 12.0, 0.0)),
            TestPath::Gain,
            TestApply::Live,
            (),
            None,
            ParameterFormatter::Plain,
            None,
        )];
        const REGISTRY: ParameterRegistry<TestBinding> = ParameterRegistry::new(BINDINGS);

        let binding = REGISTRY.binding(7).unwrap();
        let mut patch = TestPatch::default();

        assert_eq!(REGISTRY.binding_index(7), Some(0));
        assert_eq!(REGISTRY.binding_by_index(0).unwrap().id(), ParameterId(7));
        assert_eq!(REGISTRY.info(7).unwrap().name, "Gain");
        assert_eq!(REGISTRY.default_normalized_value_by_index(0), Some(0.5));
        assert_eq!(REGISTRY.normalized_value(7, 6.0), Some(0.75));
        assert_eq!(REGISTRY.denormalized_value(7, 0.25), Some(-6.0));
        assert_eq!(REGISTRY.patch_plain_value(&patch, 7), Some(0.0));
        assert_eq!(binding.apply_plain(&mut patch, 24.0), TestApply::Live);
        assert_eq!(patch.gain, 12.0);
        assert_eq!(binding.plain_value(&patch), 12.0);
        assert_eq!(REGISTRY.normalized_patch_value(&patch, 7), Some(1.0));
        assert_eq!(binding.format_plain_value(12.0), "12.0");
        assert_eq!(REGISTRY.formatted_plain_value(7, 12.0), "12.0");
        assert_eq!(
            REGISTRY.apply_plain(&mut patch, 7, -24.0),
            Some(TestApply::Live)
        );
        assert_eq!(patch.gain, -12.0);
    }

    #[test]
    fn parameter_registry_projects_editor_metadata_with_parameter_ids() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestApply {
            Live,
        }

        #[derive(Debug, Clone, Copy)]
        enum TestPath {
            Gain,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct TestEditorMetadata {
            slot: u8,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct TestEditorBinding {
            id: ParameterId,
            slot: u8,
        }

        impl ParameterEditorBindingProjection<TestEditorBinding> for TestEditorMetadata {
            fn project_editor_binding(self, id: ParameterId) -> TestEditorBinding {
                TestEditorBinding {
                    id,
                    slot: self.slot,
                }
            }
        }

        type TestBinding =
            ParameterBinding<TestPath, TestApply, (), (), ParameterFormatter, TestEditorMetadata>;
        const BINDINGS: &[TestBinding] = &[
            TestBinding::new(
                ParameterInfo::continuous(
                    7,
                    "Gain",
                    "dB",
                    ParameterRange::linear(-12.0, 12.0, 0.0),
                ),
                TestPath::Gain,
                TestApply::Live,
                (),
                None,
                ParameterFormatter::Plain,
                Some(TestEditorMetadata { slot: 2 }),
            ),
            TestBinding::new(
                ParameterInfo::continuous(8, "Hidden", "", ParameterRange::linear(0.0, 1.0, 0.0)),
                TestPath::Gain,
                TestApply::Live,
                (),
                None,
                ParameterFormatter::Plain,
                None,
            ),
        ];
        const REGISTRY: ParameterRegistry<TestBinding> = ParameterRegistry::new(BINDINGS);

        let editor_bindings = REGISTRY.projected_editor_bindings().collect::<Vec<_>>();

        assert_eq!(
            editor_bindings,
            vec![TestEditorBinding {
                id: ParameterId(7),
                slot: 2,
            }]
        );
    }

    #[test]
    fn parameter_registry_dispatches_normalized_apply_with_plugin_policy() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestApply {
            Live,
            Rebuild,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestRuntimeTarget {
            None,
            Output,
        }

        #[derive(Debug, Clone, Copy)]
        enum TestPath {
            Gain,
            Mode,
        }

        #[derive(Debug, Default, PartialEq)]
        struct TestPatch {
            gain: f32,
            mode: f32,
        }

        impl ParameterPatchPath<TestPatch> for TestPath {
            fn plain_value(self, patch: &TestPatch) -> f32 {
                match self {
                    Self::Gain => patch.gain,
                    Self::Mode => patch.mode,
                }
            }

            fn apply_plain(self, patch: &mut TestPatch, value: f32) {
                match self {
                    Self::Gain => patch.gain = value,
                    Self::Mode => patch.mode = value,
                }
            }
        }

        #[derive(Debug, Default)]
        struct TestDispatcher {
            live_updates: Vec<(ParameterId, f32)>,
            rebuilds: usize,
        }

        impl ParameterApplyDispatcher<TestPatch, TestApply, TestRuntimeTarget> for TestDispatcher {
            fn handle_parameter_apply(
                &mut self,
                _patch: &mut TestPatch,
                outcome: ParameterApplyOutcome<TestApply, TestRuntimeTarget>,
            ) {
                match (outcome.apply_kind, outcome.runtime_target) {
                    (TestApply::Live, TestRuntimeTarget::Output) => {
                        self.live_updates.push((outcome.id, outcome.plain));
                    }
                    (TestApply::Rebuild, _) => {
                        self.rebuilds += 1;
                    }
                    (TestApply::Live, TestRuntimeTarget::None) => {}
                }
            }
        }

        type TestBinding =
            ParameterBinding<TestPath, TestApply, TestRuntimeTarget, (), ParameterFormatter, ()>;
        const BINDINGS: &[TestBinding] = &[
            TestBinding::new(
                ParameterInfo::continuous(7, "Gain", "", ParameterRange::linear(0.0, 10.0, 5.0)),
                TestPath::Gain,
                TestApply::Live,
                TestRuntimeTarget::Output,
                None,
                ParameterFormatter::Plain,
                None,
            ),
            TestBinding::new(
                ParameterInfo::stepped(8, "Mode", "", ParameterRange::linear(0.0, 2.0, 0.0), 2),
                TestPath::Mode,
                TestApply::Rebuild,
                TestRuntimeTarget::None,
                None,
                ParameterFormatter::Plain,
                None,
            ),
        ];
        const REGISTRY: ParameterRegistry<TestBinding> = ParameterRegistry::new(BINDINGS);

        let mut patch = TestPatch::default();
        let mut dispatcher = TestDispatcher::default();

        let gain = REGISTRY
            .dispatch_normalized(&mut patch, 7, 0.25, &mut dispatcher)
            .unwrap();
        let mode = REGISTRY
            .dispatch_normalized(&mut patch, 8, 1.0, &mut dispatcher)
            .unwrap();

        assert_eq!(patch.gain, 2.5);
        assert_eq!(patch.mode, 2.0);
        assert_eq!(gain.plain, 2.5);
        assert_eq!(gain.normalized, 0.25);
        assert_eq!(mode.apply_kind, TestApply::Rebuild);
        assert_eq!(dispatcher.live_updates, vec![(ParameterId(7), 2.5)]);
        assert_eq!(dispatcher.rebuilds, 1);
        assert!(
            REGISTRY
                .dispatch_normalized(&mut patch, 999, 0.5, &mut dispatcher)
                .is_none()
        );
    }

    #[test]
    fn define_parameter_bindings_macro_materializes_shared_registry_shape() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestApply {
            Live,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestRuntime {
            None,
            Output,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestSmoothing {
            Runtime,
        }

        #[derive(Debug, Clone, Copy)]
        enum TestPath {
            Gain,
        }

        type TestBinding = ParameterBinding<
            TestPath,
            TestApply,
            TestRuntime,
            TestSmoothing,
            ParameterFormatter,
            (),
        >;

        crate::define_parameter_bindings! {
            binding: TestBinding;
            parameters: const TEST_PARAMETERS;
            bindings: const TEST_BINDINGS;
            defaults {
                runtime: TestRuntime::None,
                smoothing: None::<TestSmoothing>,
            }

            ParameterInfo::continuous(9, "Gain", "dB", ParameterRange::linear(-12.0, 12.0, 0.0)) => {
                path: TestPath::Gain,
                apply: TestApply::Live,
                runtime: TestRuntime::Output,
                smoothing: TestSmoothing::Runtime,
                format: ParameterFormatter::Plain,
                editor: None,
            },
            ParameterInfo::continuous(10, "Trim", "dB", ParameterRange::linear(-6.0, 6.0, 0.0)) => {
                path: TestPath::Gain,
                apply: TestApply::Live,
                format: ParameterFormatter::Plain,
                editor: None,
            },
        }

        assert_eq!(TEST_PARAMETERS.len(), 2);
        assert_eq!(TEST_BINDINGS.len(), 2);
        assert_eq!(TEST_BINDINGS[0].runtime_target(), TestRuntime::Output);
        assert_eq!(TEST_BINDINGS[0].smoothing(), Some(TestSmoothing::Runtime));
        assert_eq!(TEST_BINDINGS[0].path() as u8, TestPath::Gain as u8);
        assert_eq!(TEST_BINDINGS[1].runtime_target(), TestRuntime::None);
        assert_eq!(TEST_BINDINGS[1].smoothing(), None);
    }

    #[test]
    fn parameter_codec_maps_plain_indices_and_labels() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestChoice {
            Off,
            Slow,
            Fast,
        }

        crate::define_parameter_codec! {
            impl ParameterCodec for TestChoice {
                max: 2;
                fallback: Self::Off;
                0 => Self::Off, "Off";
                1 => Self::Slow, "Slow";
                2 => Self::Fast, "Fast";
            }
        }

        assert_eq!(TestChoice::LABELS, &["Off", "Slow", "Fast"]);
        assert_eq!(stepped_parameter_index(f32::NAN, 2), 0);
        assert_eq!(stepped_parameter_index(1.6, 2), 2);
        assert_eq!(stepped_parameter_index(99.0, 2), 2);
        assert_eq!(TestChoice::from_plain(f32::NAN), TestChoice::Off);
        assert_eq!(TestChoice::from_plain(1.6), TestChoice::Fast);
        assert_eq!(TestChoice::from_index(99), TestChoice::Off);
        assert_eq!(TestChoice::Slow.plain(), 1.0);
        assert_eq!(TestChoice::label_from_plain(2.0), "Fast");
    }
}
