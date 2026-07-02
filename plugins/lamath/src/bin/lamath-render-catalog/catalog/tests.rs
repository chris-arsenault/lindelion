use super::*;
use crate::cli::RenderSelection;

#[test]
fn catalog_selection_handles_baseline_dynamics_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "baseline_dynamics");
    assert_eq!(selected.len(), 12);
    assert_all_paths_safe(&selected);
    assert_eq!(selected.first().unwrap().id, "baseline_modal_c4_v020");
    assert_eq!(selected.last().unwrap().id, "baseline_mesh_c4_v127");
    assert_eq!(
        select_case(&cases, "baseline_string_c4_v100").single().id,
        "baseline_string_c4_v100"
    );
}

#[test]
fn catalog_selection_handles_register_range_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "register_range");
    assert_eq!(selected.len(), 12);
    assert_all_paths_safe(&selected);
    assert_eq!(selected.first().unwrap().id, "register_modal_c2_v100");
    assert_eq!(selected.last().unwrap().id, "register_mesh_c6_v100");
    assert_eq!(
        select_case(&cases, "register_tube_c6_v100").single().id,
        "register_tube_c6_v100"
    );
}

#[test]
fn catalog_selection_handles_drivers_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "drivers");
    assert_eq!(selected.len(), 17);
    assert_all_paths_safe(&selected);
    assert_eq!(selected.first().unwrap().id, "driver_string_sample_c4_v100");
    assert_eq!(
        selected.last().unwrap().id,
        "driver_tube_reed_c4_phrasing_full"
    );
    assert_eq!(
        select_case(&cases, "driver_string_bow_scratch_c4_v100")
            .single()
            .id,
        "driver_string_bow_scratch_c4_v100"
    );
}

#[test]
fn catalog_selection_handles_contact_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "contact");
    assert_eq!(selected.len(), 4);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "contact_string_tight_short_c4_v100"
    );
    assert_eq!(
        selected.last().unwrap().id,
        "contact_string_wide_long_c4_v100"
    );
    assert_eq!(
        select_case(&cases, "contact_string_wide_long_c4_v100")
            .single()
            .id,
        "contact_string_wide_long_c4_v100"
    );
}

#[test]
fn catalog_selection_handles_source_body_balance_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "source_body_balance");
    assert_eq!(selected.len(), 9);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "source_body_string_depth000_c4_v020"
    );
    assert_eq!(
        selected.last().unwrap().id,
        "source_body_string_depth100_c4_repeated_v127"
    );
    assert_eq!(
        select_case(&cases, "source_body_string_depth050_c4_v127")
            .single()
            .id,
        "source_body_string_depth050_c4_v127"
    );
}

#[test]
fn catalog_selection_handles_surrounding_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "surrounding");
    assert_eq!(selected.len(), 8);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "surrounding_modal_off_c4_v127"
    );
    assert_eq!(selected.last().unwrap().id, "surrounding_modal_all_c4_v127");
    assert_eq!(
        select_case(&cases, "surrounding_modal_sympathetic_c4_v127")
            .single()
            .id,
        "surrounding_modal_sympathetic_c4_v127"
    );
    assert_eq!(
        select_case(&cases, "surrounding_modal_all_c4_v127")
            .single()
            .id,
        "surrounding_modal_all_c4_v127"
    );
}

#[test]
fn catalog_selection_handles_chords_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "chords");
    assert_eq!(selected.len(), 6);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "chord_string_major_triad_v100"
    );
    assert_eq!(
        selected.last().unwrap().id,
        "chord_modal_major_triad_sympathetic_on_v100"
    );
    assert_eq!(
        select_case(&cases, "chord_string_dense_cluster_v100")
            .single()
            .id,
        "chord_string_dense_cluster_v100"
    );
    assert_eq!(
        select_case(&cases, "chord_modal_repeated_strikes_v100")
            .single()
            .id,
        "chord_modal_repeated_strikes_v100"
    );
    assert_eq!(
        select_case(&cases, "chord_modal_major_triad_sympathetic_on_v100")
            .single()
            .id,
        "chord_modal_major_triad_sympathetic_on_v100"
    );
}

#[test]
fn catalog_selection_handles_articulation_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "articulation");
    assert_eq!(selected.len(), 19);
    assert_all_paths_safe(&selected);
    assert_eq!(selected.first().unwrap().id, "tube_scale_tongued_c4_c5");
    assert_eq!(selected.last().unwrap().id, "tube_articulation_slot_slur");
}

#[test]
fn catalog_selection_handles_mesh_strikers_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "mesh_strikers");
    assert_eq!(selected.len(), 12);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "mesh_striker_kit_ride_hard_stick_single"
    );
    assert_eq!(
        selected.last().unwrap().id,
        "mesh_striker_gong_crash_jazz_brush_overlap"
    );
    assert_eq!(
        select_case(&cases, "mesh_striker_kit_crash_hard_stick_build")
            .single()
            .id,
        "mesh_striker_kit_crash_hard_stick_build"
    );
}

#[test]
fn catalog_selection_handles_edges_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();
    assert_eq!(cases.len(), 199);

    let selected = select_group(&cases, "edges");
    assert_eq!(selected.len(), 8);
    assert_all_paths_safe(&selected);
    assert_eq!(
        selected.first().unwrap().id,
        "edge_string_high_loop_gain_c4_v127"
    );
    assert_eq!(
        selected.last().unwrap().id,
        "edge_string_dense_hard_chord_v127"
    );
    assert_eq!(
        select_case(&cases, "edge_modal_bright_long_decay_c4_v127")
            .single()
            .id,
        "edge_modal_bright_long_decay_c4_v127"
    );
    assert_eq!(
        select_case(&cases, "edge_string_dense_hard_chord_v127")
            .single()
            .id,
        "edge_string_dense_hard_chord_v127"
    );
}

#[test]
fn catalog_selection_handles_tube_dynamics_group() {
    let cases = catalog_cases();
    validate_catalog(&cases).unwrap();

    let selected = select_group(&cases, "tube_dynamics");
    assert_eq!(selected.len(), 6);
    assert_all_paths_safe(&selected);
    assert_eq!(selected.first().unwrap().id, "tube_dyn_scale_v020_bell_on");
    assert_eq!(selected.last().unwrap().id, "tube_dyn_scale_v127_bell_off");
    assert_eq!(
        select_case(&cases, "tube_dyn_scale_v100_bell_off")
            .single()
            .id,
        "tube_dyn_scale_v100_bell_off"
    );
}

#[test]
fn catalog_selection_rejects_unknown_group_or_case() {
    let cases = catalog_cases();
    assert_eq!(
        selected_cases(&cases, &RenderSelection::Group("missing".to_string())).unwrap_err(),
        CatalogError::UnknownGroup("missing".to_string())
    );
    assert_eq!(
        selected_cases(&cases, &RenderSelection::Case("missing".to_string())).unwrap_err(),
        CatalogError::UnknownCase("missing".to_string())
    );
}

#[test]
fn catalog_selection_by_tag_collects_all_tagged_cases() {
    let cases = catalog_cases();
    // Every Mesh case across all groups carries the "mesh" tag, so one `--tag mesh` selects
    // them all (baseline + register + articulation + timbre + striker + the edge case).
    let mesh = selected_cases(&cases, &RenderSelection::Tag("mesh".to_string())).unwrap();
    assert!(
        mesh.len() >= 31,
        "expected all mesh cases, got {}",
        mesh.len()
    );
    assert!(mesh.iter().all(|case| case.tags.contains(&"mesh")));
    assert_all_paths_safe(&mesh);
    assert_eq!(
        selected_cases(&cases, &RenderSelection::Tag("nonexistent".to_string())).unwrap_err(),
        CatalogError::UnknownTag("nonexistent".to_string())
    );
}

#[test]
fn catalog_validation_rejects_duplicate_ids_and_paths() {
    let mut duplicate_id = catalog_cases();
    duplicate_id[1].id = duplicate_id[0].id;
    assert_eq!(
        validate_catalog(&duplicate_id).unwrap_err(),
        CatalogError::DuplicateCaseId(duplicate_id[0].id.to_string())
    );

    let mut duplicate_path = catalog_cases();
    duplicate_path[1].relative_wav = duplicate_path[0].relative_wav;
    assert_eq!(
        validate_catalog(&duplicate_path).unwrap_err(),
        CatalogError::DuplicateOutputPath(duplicate_path[0].relative_wav.to_string())
    );
}

#[test]
fn catalog_validation_rejects_unsafe_paths() {
    for unsafe_path in [
        "../escape.wav",
        "/absolute.wav",
        "01_baseline_dynamics/Unsafe.wav",
        "01_baseline_dynamics/missing_extension",
        "01_baseline_dynamics/not-wav.mp3",
    ] {
        let mut cases = catalog_cases();
        cases[0].relative_wav = unsafe_path;
        assert_eq!(
            validate_catalog(&cases).unwrap_err(),
            CatalogError::UnsafeOutputPath(unsafe_path.to_string())
        );
    }
}

fn select_group(cases: &[CatalogCase], group: &str) -> Vec<CatalogCase> {
    selected_cases(cases, &RenderSelection::Group(group.to_string())).unwrap()
}

fn select_case(cases: &[CatalogCase], case: &str) -> Vec<CatalogCase> {
    selected_cases(cases, &RenderSelection::Case(case.to_string())).unwrap()
}

fn assert_all_paths_safe(cases: &[CatalogCase]) {
    assert!(
        cases
            .iter()
            .all(|case| safe_relative_wav(case.relative_wav))
    );
}

trait SingleCase {
    fn single(&self) -> &CatalogCase;
}

impl SingleCase for Vec<CatalogCase> {
    fn single(&self) -> &CatalogCase {
        assert_eq!(self.len(), 1);
        &self[0]
    }
}
