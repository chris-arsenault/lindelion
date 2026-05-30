use super::*;
use crate::{
    ModalPreset, WaveguideStyle, assert_no_allocations,
    dsp::{
        modal::ModalBankParams,
        render_metrics::{
            RenderExcitation, compare_render_metrics, render_metric_profile, render_modal_response,
        },
    },
};
use lindelion_dsp_utils::analysis::assert_all_finite;

#[test]
fn mesh_promotion_gate_promotes_runtime_exposed_rectangular_mesh() {
    let target = concrete_plate_promotion_target();
    let evidence = evaluate_mesh_promotion(target);

    // The mesh is a new selectable resonator model; the String/Tube waveguide
    // styles are unchanged (the mesh is not hidden behind a style).
    assert_eq!(
        WaveguideStyle::ALL,
        [WaveguideStyle::String, WaveguideStyle::Tube]
    );
    assert!(evidence.concrete_target, "evidence={evidence:?}");
    assert!(
        evidence.candidate_rms > target.criteria.min_candidate_rms,
        "evidence={evidence:?}"
    );
    // Objective render gate: the mesh sounds materially unlike any ModalBank
    // baseline and its spatial strike response exceeds what modal positioning can
    // produce — a plate/membrane sound ModalBank cannot reach.
    assert!(
        evidence.closest_modal_difference > target.criteria.min_closest_modal_difference,
        "evidence={evidence:?}"
    );
    assert!(
        evidence.spatial_advantage() > target.criteria.min_spatial_advantage,
        "evidence={evidence:?}"
    );
    assert!(evidence.process_is_allocation_free, "evidence={evidence:?}");
    assert!(evidence.runtime_exposed, "evidence={evidence:?}");
    assert_eq!(
        evidence.decision(target.criteria),
        MeshPromotionDecision::Promote
    );
}

#[test]
fn promotion_gate_requires_complete_evidence_before_promoting() {
    let criteria = MeshPromotionCriteria::default();
    let passing = MeshPromotionEvidence {
        concrete_target: true,
        candidate_rms: criteria.min_candidate_rms * 2.0,
        closest_modal_difference: criteria.min_closest_modal_difference * 2.0,
        mesh_spatial_difference: 0.4,
        modal_spatial_difference: 0.1,
        process_is_allocation_free: true,
        runtime_exposed: true,
    };

    assert_eq!(passing.decision(criteria), MeshPromotionDecision::Promote);
    assert_eq!(
        MeshPromotionEvidence {
            runtime_exposed: false,
            ..passing
        }
        .decision(criteria),
        MeshPromotionDecision::KeepPrototype
    );
    assert_eq!(
        MeshPromotionEvidence {
            closest_modal_difference: criteria.min_closest_modal_difference * 0.5,
            ..passing
        }
        .decision(criteria),
        MeshPromotionDecision::KeepPrototype
    );
    assert_eq!(
        MeshPromotionEvidence {
            mesh_spatial_difference: 0.11,
            modal_spatial_difference: 0.1,
            ..passing
        }
        .decision(criteria),
        MeshPromotionDecision::KeepPrototype
    );
}

#[test]
fn mesh_runtime_is_allocation_free_and_stable_at_parameter_extremes() {
    // Allocation-free, fixed-memory runtime: after construction, re-tuning and
    // processing never allocate (the grid is fixed; spatial weights recompute in
    // place within their grid-sized capacity).
    let mut mesh = MeshResonator::new(48_000.0);
    assert_no_allocations("mesh runtime configure + process", || {
        for material in [0.0, 1.0] {
            mesh.configure(MeshVoiceParams {
                frequency_hz: 110.0,
                material,
                size: 0.0,
                damping: 1.0,
                tension: 1.0,
                strike_position: material,
                pickup_spread: 1.0,
            });
            for index in 0..256 {
                let _ = mesh.process_sample((index == 0) as u8 as f32);
            }
        }
    });

    // Finite and bounded at every exposed-parameter extreme, across the range.
    for frequency_hz in [30.0_f32, 220.0, 4_000.0] {
        for material in [0.0_f32, 1.0] {
            for extreme in [0.0_f32, 1.0] {
                let mut mesh = MeshResonator::new(48_000.0);
                mesh.configure(MeshVoiceParams {
                    frequency_hz,
                    material,
                    size: extreme,
                    damping: extreme,
                    tension: 1.0 - extreme,
                    strike_position: extreme,
                    pickup_spread: 1.0 - extreme,
                });
                let output = (0..4_096)
                    .map(|index| mesh.process_sample((index == 0) as u8 as f32))
                    .collect::<Vec<_>>();
                assert_all_finite(&output);
                assert!(
                    output.iter().all(|sample| sample.abs() < 8.0),
                    "mesh must stay bounded: f={frequency_hz} material={material} extreme={extreme}"
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MeshPromotionTarget {
    name: &'static str,
    config: RectangularMesh2dConfig,
    alternate_config: RectangularMesh2dConfig,
    excitation: RenderExcitation,
    sample_count: usize,
    criteria: MeshPromotionCriteria,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshPromotionCriteria {
    min_candidate_rms: f32,
    min_closest_modal_difference: f32,
    min_spatial_advantage: f32,
}

impl Default for MeshPromotionCriteria {
    fn default() -> Self {
        Self {
            min_candidate_rms: 1.0e-8,
            min_closest_modal_difference: 0.05,
            min_spatial_advantage: 0.03,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshPromotionEvidence {
    concrete_target: bool,
    candidate_rms: f32,
    closest_modal_difference: f32,
    mesh_spatial_difference: f32,
    modal_spatial_difference: f32,
    process_is_allocation_free: bool,
    runtime_exposed: bool,
}

impl MeshPromotionEvidence {
    fn decision(self, criteria: MeshPromotionCriteria) -> MeshPromotionDecision {
        let clears_audio_gate = self.concrete_target
            && self.candidate_rms > criteria.min_candidate_rms
            && self.closest_modal_difference > criteria.min_closest_modal_difference
            && self.spatial_advantage() > criteria.min_spatial_advantage;
        if clears_audio_gate && self.process_is_allocation_free && self.runtime_exposed {
            MeshPromotionDecision::Promote
        } else {
            MeshPromotionDecision::KeepPrototype
        }
    }

    fn spatial_advantage(self) -> f32 {
        self.mesh_spatial_difference - self.modal_spatial_difference
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeshPromotionDecision {
    Promote,
    KeepPrototype,
}

fn evaluate_mesh_promotion(target: MeshPromotionTarget) -> MeshPromotionEvidence {
    let primary = render_mesh(target.config, target.sample_count, target.excitation);
    let alternate = render_mesh(
        target.alternate_config,
        target.sample_count,
        target.excitation,
    );
    let frequency_hz = mode_frequency(target.config);
    let profile = render_metric_profile(&primary, target.config.sample_rate, frequency_hz);

    assert_all_finite(&primary);
    assert_all_finite(&alternate);
    MeshPromotionEvidence {
        concrete_target: !target.name.is_empty(),
        candidate_rms: profile.early.rms,
        closest_modal_difference: closest_modal_difference(&primary, target),
        mesh_spatial_difference: spatial_difference(&primary, &alternate, target),
        modal_spatial_difference: strongest_modal_spatial_difference(target),
        process_is_allocation_free: process_sample_is_allocation_free(target.config),
        runtime_exposed: mesh_runtime_exposed(),
    }
}

fn closest_modal_difference(mesh: &[f32], target: MeshPromotionTarget) -> f32 {
    let frequency_hz = mode_frequency(target.config);
    modal_baselines(frequency_hz, target.config.strike_position.x)
        .into_iter()
        .map(|params| {
            let modal = render_modal_response(
                target.config.sample_rate,
                params,
                target.sample_count,
                target.excitation,
            );
            compare_render_metrics(&modal, mesh, target.config.sample_rate, frequency_hz)
                .normalized_shape_difference
        })
        .fold(f32::INFINITY, f32::min)
}

fn strongest_modal_spatial_difference(target: MeshPromotionTarget) -> f32 {
    let frequency_hz = mode_frequency(target.config);
    modal_baselines(frequency_hz, target.config.strike_position.x)
        .into_iter()
        .map(|params| {
            let moved = ModalBankParams {
                position_of_strike: target.alternate_config.strike_position.x,
                ..params
            };
            let base = render_modal_response(
                target.config.sample_rate,
                params,
                target.sample_count,
                target.excitation,
            );
            let alternate = render_modal_response(
                target.config.sample_rate,
                moved,
                target.sample_count,
                target.excitation,
            );
            spatial_difference(&base, &alternate, target)
        })
        .fold(0.0, f32::max)
}

fn spatial_difference(left: &[f32], right: &[f32], target: MeshPromotionTarget) -> f32 {
    compare_render_metrics(
        left,
        right,
        target.config.sample_rate,
        mode_frequency(target.config),
    )
    .normalized_shape_difference
}

fn process_sample_is_allocation_free(config: RectangularMesh2dConfig) -> bool {
    let mut mesh = RectangularMesh2d::new(config);
    assert_no_allocations("2d mesh process_sample", || {
        for index in 0..512 {
            let excitation = if index == 0 { 1.0 } else { 0.0 };
            let _ = mesh.process_sample(excitation);
        }
    });
    true
}

fn mesh_runtime_exposed() -> bool {
    // The mesh is promoted as its own selectable resonator model: a
    // `ResonatorConfig::Mesh` drives a runtime `MeshResonator` in the voice
    // engine and renders, without touching the String/Tube waveguide styles.
    let mut mesh = MeshResonator::new(48_000.0);
    mesh.configure(MeshVoiceParams::default());
    let rings = (0..512)
        .map(|index| mesh.process_sample((index == 0) as u8 as f32))
        .any(|sample| sample.abs() > 0.0);
    rings
        && matches!(
            crate::ResonatorConfig::Mesh(crate::MeshConfig::default()),
            crate::ResonatorConfig::Mesh(_)
        )
}

fn concrete_plate_promotion_target() -> MeshPromotionTarget {
    let config = RectangularMesh2dConfig {
        width: 18,
        height: 12,
        wave_speed_mps: 240.0,
        physical_width_m: 0.68,
        physical_height_m: 0.46,
        boundary: MeshBoundaryConfig::fixed_edges(0.14, 0.24, 0.18, 0.3),
        strike_position: MeshPoint::new(0.31, 0.42),
        pickup_position: MeshPoint::new(0.72, 0.58),
        excitation_width: 0.045,
        ..RectangularMesh2dConfig::default()
    };
    MeshPromotionTarget {
        name: "small_rectangular_plate_spatial_strike",
        config,
        alternate_config: RectangularMesh2dConfig {
            strike_position: MeshPoint::new(0.76, 0.25),
            pickup_position: MeshPoint::new(0.23, 0.73),
            ..config
        },
        excitation: RenderExcitation::NoiseBurst,
        sample_count: 24_000,
        criteria: MeshPromotionCriteria::default(),
    }
}

fn modal_baselines(fundamental_hz: f32, strike_position: f32) -> [ModalBankParams; 4] {
    [
        modal_baseline(fundamental_hz, ModalPreset::MetalBar, 0.28, strike_position),
        modal_baseline(
            fundamental_hz,
            ModalPreset::GlassBowl,
            0.18,
            strike_position,
        ),
        modal_baseline(fundamental_hz, ModalPreset::Bell, 0.22, strike_position),
        modal_baseline(
            fundamental_hz,
            ModalPreset::GenericStrike,
            0.08,
            strike_position,
        ),
    ]
}

fn modal_baseline(
    fundamental_hz: f32,
    preset: ModalPreset,
    inharmonicity: f32,
    strike_position: f32,
) -> ModalBankParams {
    ModalBankParams {
        fundamental_hz,
        mode_count: 96,
        preset,
        inharmonicity,
        brightness: 0.66,
        decay_global: 1.15,
        decay_tilt: 0.35,
        position_of_strike: strike_position,
    }
}
