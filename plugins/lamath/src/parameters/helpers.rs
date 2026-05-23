pub(crate) fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .find(|binding| binding.id() == ParameterId(id))
}

pub(crate) fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS.get(index)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PARAMETER_BINDING_COUNT: usize = PARAMETER_BINDINGS.len();

pub(crate) fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_BINDINGS
        .iter()
        .position(|binding| binding.id() == ParameterId(id))
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) fn editor_parameter_bindings() -> impl Iterator<Item = &'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .filter(|binding| binding.editor().is_some())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn editor_parameter_binding(
    slot: EditorSurfaceSlot,
) -> Option<&'static ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .find(|binding| binding.editor().is_some_and(|editor| editor.slot() == slot))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn editor_parameter_bindings_for_group(
    group: EditorSurfaceGroup,
) -> impl Iterator<Item = &'static ParameterBinding> {
    editor_parameter_bindings().filter(move |binding| {
        binding
            .editor()
            .is_some_and(|editor| editor.group() == group)
    })
}

pub(crate) fn patch_parameter_plain_value(patch: &ResonatorSynthPatch, id: u32) -> Option<f32> {
    parameter_binding(id).map(|binding| binding.plain_value(patch))
}

pub(crate) fn smoothed_runtime_parameter(
    id: u32,
    sample_rate: f32,
    initial_plain: f32,
) -> Option<SmoothedAtomicParam> {
    let binding = parameter_binding(id)?;
    let spec = binding.smoothed_atomic_spec()?;
    Some(SmoothedAtomicParam::with_initial_plain(
        spec,
        sample_rate,
        initial_plain,
    ))
}

pub(crate) fn apply_parameter_plain(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    value: f32,
) -> ParameterApplyKind {
    let Some(binding) = parameter_binding(id) else {
        return ParameterApplyKind::Ignored;
    };
    binding.apply_plain(patch, value)
}

pub(crate) fn apply_parameter_plain_for_controller(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    value: f32,
) -> bool {
    !matches!(
        apply_parameter_plain(patch, id, value),
        ParameterApplyKind::Ignored
    )
}

pub(crate) fn finite_value(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

fn output_gain_from_plain(gain_db: f32) -> f32 {
    db_to_gain(MASTER_GAIN_DB.clamp(gain_db))
}

fn modal_config_from(config: ResonatorConfig) -> ModalConfig {
    match config {
        ResonatorConfig::Modal(config) => config,
        ResonatorConfig::Waveguide(config) => ModalConfig {
            semitone_offset: config.semitone_offset,
            cent_offset: config.cent_offset,
            position_of_strike: config.position_of_strike,
            ..ModalConfig::default()
        },
    }
}

fn waveguide_config_from(config: ResonatorConfig) -> WaveguideConfig {
    match config {
        ResonatorConfig::Waveguide(config) => config,
        ResonatorConfig::Modal(config) => WaveguideConfig {
            semitone_offset: config.semitone_offset,
            cent_offset: config.cent_offset,
            position_of_strike: config.position_of_strike,
            ..WaveguideConfig::default()
        },
    }
}

fn parallel_mix_a(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_a, .. } => mix_a,
        ResonatorRouting::Series { mix_a, .. } => mix_a,
    }
}

fn parallel_mix_b(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_b, .. } => mix_b,
        ResonatorRouting::Series { mix_b, .. } => mix_b,
    }
}

fn set_parallel_mix(routing: ResonatorRouting, side: MixSide, value: f32) -> ResonatorRouting {
    let mut mix_a = parallel_mix_a(routing);
    let mut mix_b = parallel_mix_b(routing);
    match side {
        MixSide::A => mix_a = finite_value(value, 0.0, 1.0, 0.5),
        MixSide::B => mix_b = finite_value(value, 0.0, 1.0, 0.5),
    }
    match routing {
        ResonatorRouting::Parallel { .. } => ResonatorRouting::Parallel { mix_a, mix_b },
        ResonatorRouting::Series { .. } => ResonatorRouting::Series { mix_a, mix_b },
    }
}

fn bool_from_plain(value: f32) -> bool {
    finite_value(value, 0.0, 1.0, 0.0) >= 0.5
}

fn bool_plain(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn stepped_index(value: f32, max: u32) -> u32 {
    finite_value(value, 0.0, max as f32, 0.0).round() as u32
}

fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn filter_mode_label_from_plain(value: f32) -> &'static str {
    FilterMode::label_from_plain(value)
}

fn routing_label_from_plain(value: f32) -> &'static str {
    RoutingMode::label_from_plain(value)
}

fn resonator_model_label_from_plain(value: f32) -> &'static str {
    ResonatorModel::label_from_plain(value)
}

fn modal_preset_label_from_plain(value: f32) -> &'static str {
    ModalPreset::label_from_plain(value)
}

fn lfo_shape_label_from_plain(value: f32) -> &'static str {
    LfoShape::label_from_plain(value)
}

fn waveguide_style_label_from_plain(value: f32) -> &'static str {
    <WaveguideStyle as ParameterCodec>::label_from_plain(value)
}

fn format_modulation_source_label(value: f32) -> &'static str {
    ModulationSource::label_from_plain(value)
}

fn format_modulation_destination_label(value: f32) -> &'static str {
    ModulationDestination::label_from_plain(value)
}

fn retrigger_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) {
        "Retrigger"
    } else {
        "Carry"
    }
}

fn tempo_sync_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) {
        "Sync"
    } else {
        "Free"
    }
}

fn enabled_label_from_plain(value: f32) -> &'static str {
    if bool_from_plain(value) { "On" } else { "Off" }
}

