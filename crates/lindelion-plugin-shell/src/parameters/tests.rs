use super::*;
use lindelion_dsp_utils::smoothing::SmoothedParamSpec;

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
    let info = ParameterInfo::continuous(1, "Target", "", ParameterRange::linear(0.0, 1.0, 0.0));
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

    let info = ParameterInfo::continuous(2, "Mapped", "", ParameterRange::linear(0.0, 2.0, 1.0));
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
    assert_eq!(REGISTRY.default_normalized_values::<2>(), [0.5, 0.0]);
    assert_eq!(REGISTRY.normalized_value(7, 6.0), Some(0.75));
    assert_eq!(REGISTRY.denormalized_value(7, 0.25), Some(-6.0));
    assert_eq!(REGISTRY.patch_plain_value(&patch, 7), Some(0.0));
    assert_eq!(binding.apply_plain(&mut patch, 24.0), TestApply::Live);
    assert_eq!(patch.gain, 12.0);
    assert_eq!(binding.plain_value(&patch), 12.0);
    assert_eq!(REGISTRY.normalized_patch_value(&patch, 7), Some(1.0));
    assert_eq!(
        REGISTRY.normalized_patch_values(&patch, [0.0; 2]),
        [1.0, 0.0]
    );
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
            ParameterInfo::continuous(7, "Gain", "dB", ParameterRange::linear(-12.0, 12.0, 0.0)),
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

    type TestBinding =
        ParameterBinding<TestPath, TestApply, TestRuntime, TestSmoothing, ParameterFormatter, ()>;

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
