fn test_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        name: "Runtime Test".to_string(),
        polyphony: 4,
        resonator_a: ResonatorConfig::Modal(ModalConfig {
            mode_count: 12,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.35,
            ..ModalConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        output: OutputConfig {
            filter_mode: FilterMode::BandPass,
            filter_cutoff: 4_000.0,
            master_gain_db: -6.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}

fn pitch_tracking_waveguide_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        name: "Pitch Tracking".to_string(),
        polyphony: 1,
        resonator_a: ResonatorConfig::Waveguide(crate::WaveguideConfig {
            loop_gain: 0.99,
            loop_filter_cutoff: 12_000.0,
            ..crate::WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: 0.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}

fn pitch_tracking_polyphonic_waveguide_patch() -> ResonatorSynthPatch {
    let mut patch = pitch_tracking_waveguide_patch();
    patch.polyphony = 2;
    patch
}

fn two_member_channel_notes() -> [MidiEvent; 2] {
    [
        MidiEvent::Note(NoteEvent::On {
            channel: 1,
            note: 48,
            velocity: 1.0,
        }),
        MidiEvent::Note(NoteEvent::On {
            channel: 2,
            note: 60,
            velocity: 1.0,
        }),
    ]
}

fn expression_for_slot(
    processor: &ResonatorProcessor<'_>,
    channel: u8,
    note: u8,
) -> VoiceExpression {
    (0..processor.engine.polyphony())
        .find(|index| {
            processor.engine.slot_channel(*index) == Some(channel)
                && processor.engine.slot_note(*index) == Some(note)
        })
        .and_then(|index| processor.engine.slot_expression(index))
        .unwrap()
}

fn assert_slot_expression_gate(
    processor: &ResonatorProcessor<'_>,
    channel: u8,
    note: u8,
    gate: bool,
) {
    assert_eq!(
        expression_for_slot(processor, channel, note).stream.gate,
        gate
    );
}

fn assert_frequency_dominates(
    samples: &[f32],
    sample_rate: f32,
    high_note: f32,
    low_note: f32,
) {
    let high = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(high_note));
    let low = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(low_note));
    assert!(
        high > low,
        "note {high_note} magnitude {high} should exceed note {low_note} magnitude {low}"
    );
}

fn expression_filter_patch() -> ResonatorSynthPatch {
    let mut patch = test_patch();
    patch.output.filter_mode = FilterMode::LowPass;
    patch.output.filter_cutoff = 300.0;
    patch.output.filter_resonance = 0.0;
    patch.output.master_gain_db = 0.0;
    patch.modulation.slots[0] = ModulationSlot {
        enabled: true,
        source: ModulationSource::Aftertouch,
        destination: ModulationDestination::FilterCutoff,
        amount: 1.0,
    };
    patch
}

fn external_expression_filter_patch() -> ResonatorSynthPatch {
    let mut patch = expression_filter_patch();
    patch.polyphony = 1;
    patch.modulation.slots[0].amount = 0.5;
    patch.modulation.slots[1] = ModulationSlot {
        enabled: true,
        source: ModulationSource::Brightness,
        destination: ModulationDestination::FilterCutoff,
        amount: 0.5,
    };
    patch
}

fn aftertouch_resonator_damping_patch() -> ResonatorSynthPatch {
    resonator_damping_patch(ModulationSource::Aftertouch)
}

fn poly_pressure_resonator_damping_patch() -> ResonatorSynthPatch {
    let mut patch = resonator_damping_patch(ModulationSource::Aftertouch);
    patch.polyphony = 2;
    patch
}

fn mod_wheel_resonator_damping_patch() -> ResonatorSynthPatch {
    resonator_damping_patch(ModulationSource::ModWheel)
}

fn brightness_resonator_damping_patch() -> ResonatorSynthPatch {
    resonator_damping_patch(ModulationSource::Brightness)
}

fn resonator_damping_patch(source: ModulationSource) -> ResonatorSynthPatch {
    let mut patch = test_patch();
    patch.polyphony = 1;
    patch.resonator_a = ResonatorConfig::Waveguide(crate::WaveguideConfig {
        loop_gain: 0.62,
        loop_filter_cutoff: 12_000.0,
        ..crate::WaveguideConfig::default()
    });
    patch.routing = ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    };
    patch.output.filter_mode = FilterMode::LowPass;
    patch.output.filter_cutoff = 20_000.0;
    patch.output.master_gain_db = 0.0;
    patch.modulation.slots[0] = ModulationSlot {
        enabled: true,
        source,
        destination: ModulationDestination::ResonatorADamping,
        amount: 1.0,
    };
    patch
}

fn mean_abs_difference(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len()).max(1);
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| (left - right).abs())
        .sum::<f32>()
        / len as f32
}
