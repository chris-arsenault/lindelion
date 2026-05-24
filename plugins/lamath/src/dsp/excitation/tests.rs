use super::*;
use crate::assert_no_allocations;

#[test]
fn one_shot_plays_then_finishes() {
    let samples = [1.0, 0.5, -0.5];
    let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 1.0, 0.0, false);

    assert_eq!(playback.next_sample(), 1.0);
    assert_eq!(playback.next_sample(), 0.5);
    assert_eq!(playback.next_sample(), -0.5);
    assert_eq!(playback.next_sample(), 0.0);
    assert!(playback.is_finished());
}

#[test]
fn pitch_ratio_advances_cursor() {
    let samples = [0.0, 1.0, 2.0, 3.0];
    let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 2.0, 0.0, false);

    assert_eq!(playback.next_sample(), 0.0);
    assert_eq!(playback.next_sample(), 2.0);
}

#[test]
fn looped_playback_wraps() {
    let samples = [1.0, 2.0];
    let mut playback = ExcitationPlayback::new(&samples, 48_000.0, 48_000.0, 1.0, 0.0, true);

    assert_eq!(playback.next_sample(), 1.0);
    assert_eq!(playback.next_sample(), 2.0);
    assert_eq!(playback.next_sample(), 1.0);
    assert!(!playback.is_finished());
}

#[test]
fn live_excitation_block_sanitizes_bounds_and_scales_input() {
    let samples = [0.5, f32::NAN, 2.0, f32::NEG_INFINITY, -2.0];
    let block = LiveExcitationBlock::from_mono_block(&samples, 6.0);
    let gain = db_to_gain(6.0);

    assert!(block.is_enabled());
    assert_eq!(block.sample_at(0), 0.5 * gain);
    assert_eq!(block.sample_at(1), 0.0);
    assert_eq!(block.sample_at(2), gain);
    assert_eq!(block.sample_at(3), 0.0);
    assert_eq!(block.sample_at(4), -gain);
    assert_eq!(block.sample_at(5), 0.0);
}

#[test]
fn live_excitation_block_does_not_allocate() {
    let samples = [0.25; 64];
    let block = LiveExcitationBlock::from_mono_block(&samples, 0.0);

    assert_no_allocations("live excitation block sample reads", || {
        let mut sum = 0.0;
        for index in 0..samples.len() {
            sum += block.sample_at(index);
        }
        assert!(sum > 0.0);
    });
}

#[test]
fn voice_latch_captures_pre_roll_current_block_and_future_blocks() {
    let mut pre_roll = LiveExcitationPreRoll::with_capacity(3);
    pre_roll.push_block(&[0.1, 0.2, 0.3]);
    let block = [0.4, 0.5, 0.6, 0.7];
    let future = [0.8, 0.9];
    let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 2, 3, 4, 0, 0.0);
    let mut latch = VoiceLiveExcitationLatch::with_capacity(7);

    latch.trigger(capture);
    latch.continue_capture(&future);

    let output = (0..8).map(|_| latch.next_sample()).collect::<Vec<_>>();
    assert_eq!(output, vec![0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.0]);
    assert!(latch.is_finished());
}

#[test]
fn voice_latch_applies_short_fades() {
    let pre_roll = LiveExcitationPreRoll::with_capacity(0);
    let block = [1.0, 1.0, 1.0, 1.0];
    let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 0, 0, 4, 2, 0.0);
    let mut latch = VoiceLiveExcitationLatch::with_capacity(4);

    latch.trigger(capture);

    assert_eq!(latch.next_sample(), 0.5);
    assert_eq!(latch.next_sample(), 1.0);
    assert_eq!(latch.next_sample(), 1.0);
    assert_eq!(latch.next_sample(), 0.5);
    assert_eq!(latch.next_sample(), 0.0);
}

#[test]
fn voice_latch_capture_does_not_allocate() {
    let mut pre_roll = LiveExcitationPreRoll::with_capacity(32);
    pre_roll.push_block(&[0.25; 32]);
    let block = [0.5; 64];
    let capture = LiveExcitationLatchCapture::new(&pre_roll, &block, 16, 16, 32, 4, 0.0);
    let mut latch = VoiceLiveExcitationLatch::with_capacity(48);

    assert_no_allocations("voice live latch trigger and playback", || {
        latch.trigger(capture);
        latch.continue_capture(&block);
        let mut sum = 0.0;
        for _ in 0..48 {
            sum += latch.next_sample();
        }
        assert!(sum > 0.0);
    });
}

#[test]
fn selector_layers_ungrouped_velocity_matches() {
    let a = [1.0];
    let b = [2.0];
    let slots = [
        Some(RuntimeExcitationSlot::new(&a, 48_000.0)),
        Some(RuntimeExcitationSlot {
            velocity_low: 64,
            ..RuntimeExcitationSlot::new(&b, 48_000.0)
        }),
        None,
        None,
    ];
    let mut selector = ExcitationSelector::default();

    let soft = selector.select(&slots, 0.25);
    let hard = selector.select(&slots, 0.75);

    assert_eq!(soft.layer_count(), 1);
    assert_eq!(hard.layer_count(), 2);
}

#[test]
fn selector_cycles_round_robin_group() {
    let a = [1.0];
    let b = [2.0];
    let slots = [
        Some(RuntimeExcitationSlot {
            round_robin_group: Some(2),
            ..RuntimeExcitationSlot::new(&a, 48_000.0)
        }),
        Some(RuntimeExcitationSlot {
            round_robin_group: Some(2),
            ..RuntimeExcitationSlot::new(&b, 48_000.0)
        }),
        None,
        None,
    ];
    let mut selector = ExcitationSelector::default();

    let first = selector.select(&slots, 1.0);
    let second = selector.select(&slots, 1.0);
    let third = selector.select(&slots, 1.0);

    assert_eq!(first.layers()[0].unwrap().samples[0], 1.0);
    assert_eq!(second.layers()[0].unwrap().samples[0], 2.0);
    assert_eq!(third.layers()[0].unwrap().samples[0], 1.0);
}

#[test]
fn voice_excitation_sums_layers() {
    let a = [1.0, 0.0];
    let b = [0.5, 0.0];
    let mut selected = SelectedExcitations::default();
    selected.push(ExcitationLayer {
        gain: 2.0,
        ..ExcitationLayer::new(&a, 48_000.0)
    });
    selected.push(ExcitationLayer {
        gain: 4.0,
        ..ExcitationLayer::new(&b, 48_000.0)
    });
    let mut excitation = VoiceExcitation::default();

    excitation.trigger(selected, 48_000.0, 1.0);

    assert_eq!(excitation.next_sample(), 4.0);
    assert_eq!(excitation.next_sample(), 0.0);
    assert!(excitation.is_finished());
}

#[test]
fn selection_and_layer_trigger_do_not_allocate() {
    let a = [1.0, 0.0];
    let b = [0.5, 0.0];
    let slots = [
        Some(RuntimeExcitationSlot::new(&a, 48_000.0)),
        Some(RuntimeExcitationSlot {
            round_robin_group: Some(1),
            ..RuntimeExcitationSlot::new(&b, 48_000.0)
        }),
        None,
        None,
    ];
    let mut selector = ExcitationSelector::default();
    let mut excitation = VoiceExcitation::default();

    assert_no_allocations("excitation selection and trigger", || {
        let selected = selector.select(&slots, 1.0);
        excitation.trigger(selected, 48_000.0, 1.0);
        let _ = excitation.next_sample();
    });
}
