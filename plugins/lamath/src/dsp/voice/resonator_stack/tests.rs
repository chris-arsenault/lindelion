use super::*;
use crate::{ModalConfig, ModalPreset, ResonatorRouting};

#[test]
fn modal_engine_renders_after_configuration() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(&ModalConfig::default(), 440.0, true);

    let mut peak = 0.0_f32;
    for index in 0..2_048 {
        let input = if index == 0 { 1.0 } else { 0.0 };
        peak = peak.max(engine.process_sample(input).abs());
    }

    assert!(peak > 0.0);
}

#[test]
fn modal_engine_preserves_state_when_mode_count_is_unchanged() {
    let mut engine = ResonatorEngine::new(48_000.0);
    let config = ModalConfig {
        mode_count: 32,
        preset: ModalPreset::Bell,
        decay_global: 2.0,
        ..ModalConfig::default()
    };
    engine.configure(&config, 440.0, true);

    for index in 0..256 {
        engine.process_sample(if index == 0 { 1.0 } else { 0.0 });
    }
    let ringing = engine.process_sample(0.0).abs();
    engine.configure(&config, 466.16, false);
    let preserved = engine.process_sample(0.0).abs();

    assert!(ringing > 0.0);
    assert!(preserved > 0.0);
}

#[test]
fn stack_parallel_mix_sums_modal_lanes() {
    let mut stack = ResonatorStack::new(48_000.0);
    stack.set_base_configs(
        ModalConfig {
            mode_count: 16,
            preset: ModalPreset::Marimba,
            ..ModalConfig::default()
        },
        ModalConfig {
            mode_count: 16,
            preset: ModalPreset::Bell,
            ..ModalConfig::default()
        },
    );
    stack.configure(440.0, true, true);
    stack.reset_routing(ResonatorRouting::Parallel {
        mix_a: 0.5,
        mix_b: 0.5,
    });

    let output = stack.process_sample(1.0);

    assert!(output.is_finite());
    assert!(output.abs() > 0.0);
    assert!(stack.staged_output().is_finite());
}

#[test]
fn stack_keeps_series_and_body_color_modes_distinct() {
    let mut stack = ResonatorStack::new(48_000.0);
    stack.configure(440.0, true, true);

    stack.reset_routing(ResonatorRouting::Series {
        mix_a: 1.0,
        mix_b: 1.0,
    });
    assert_eq!(routing_plain(stack.routing.current()), 1);

    stack.reset_routing(ResonatorRouting::BodyColor {
        mix_a: 1.0,
        mix_b: 1.0,
    });
    assert_eq!(routing_plain(stack.routing.current()), 2);
}
