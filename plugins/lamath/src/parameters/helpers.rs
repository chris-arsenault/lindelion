pub(crate) fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding(id)
}

pub(crate) fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding_by_index(index)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PARAMETER_BINDING_COUNT: usize = PARAMETER_REGISTRY.len();

pub(crate) fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_REGISTRY.binding_index(id)
}

pub(crate) fn parameter_info(id: u32) -> Option<ParameterInfo> {
    PARAMETER_REGISTRY.info(id)
}

pub(crate) fn parameter_default_normalized_value_by_index(index: usize) -> Option<f32> {
    PARAMETER_REGISTRY.default_normalized_value_by_index(index)
}

pub(crate) fn normalized_parameter_value(id: u32, plain: f32) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_value(id, plain)
}

pub(crate) fn dispatch_parameter_normalized<Dispatcher>(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    normalized: f32,
    dispatcher: &mut Dispatcher,
) -> ParameterApplyKind
where
    Dispatcher:
        ParameterApplyDispatcher<ResonatorSynthPatch, ParameterApplyKind, RuntimeParameterTarget>,
{
    PARAMETER_REGISTRY
        .dispatch_normalized(patch, id, normalized, dispatcher)
        .map(|outcome| outcome.apply_kind)
        .unwrap_or(ParameterApplyKind::Ignored)
}

pub(crate) fn apply_parameter_normalized_for_controller(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    normalized: f32,
) -> bool {
    PARAMETER_REGISTRY
        .apply_normalized(patch, id, normalized)
        .is_some_and(|outcome| !matches!(outcome.apply_kind, ParameterApplyKind::Ignored))
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) fn editor_parameter_bindings() -> impl Iterator<Item = &'static ParameterBinding> {
    PARAMETER_REGISTRY.editor_bindings()
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn resonator_editor_parameter_bindings(
) -> impl Iterator<Item = lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding> {
    PARAMETER_REGISTRY.projected_editor_bindings()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn editor_parameter_binding(
    slot: EditorSurfaceSlot,
) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY
        .editor_bindings()
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

#[cfg(test)]
pub(crate) fn patch_parameter_plain_value(patch: &ResonatorSynthPatch, id: u32) -> Option<f32> {
    PARAMETER_REGISTRY.patch_plain_value(patch, id)
}

pub(crate) fn patch_parameter_normalized_value(
    patch: &ResonatorSynthPatch,
    id: u32,
) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_patch_value(patch, id)
}

pub(crate) fn format_parameter_plain_value(id: u32, value: f32) -> String {
    PARAMETER_REGISTRY.formatted_plain_value(id, value)
}

pub(crate) fn smoothed_runtime_parameter(
    id: u32,
    sample_rate: f32,
    initial_plain: f32,
) -> Option<SmoothedAtomicParam> {
    PARAMETER_REGISTRY.smoothed_atomic_param(id, sample_rate, initial_plain)
}

#[cfg(test)]
pub(crate) fn apply_parameter_plain(
    patch: &mut ResonatorSynthPatch,
    id: u32,
    value: f32,
) -> ParameterApplyKind {
    PARAMETER_REGISTRY
        .apply_plain(patch, id, value)
        .unwrap_or(ParameterApplyKind::Ignored)
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

fn audio_input_mode_label_from_plain(value: f32) -> &'static str {
    AudioInputMode::label_from_plain(value)
}

fn live_excitation_mode_label_from_plain(value: f32) -> &'static str {
    LiveExcitationMode::label_from_plain(value)
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
