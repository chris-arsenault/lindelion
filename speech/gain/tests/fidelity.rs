//! Gain end-to-end against the shared general-signal fidelity battery (M0 exit).

use lindelion_fidelity::{assert_allocation_free, run_general_battery};
use lindelion_speech_gain::Gain;

lindelion_test_allocator::install_test_allocator!();

#[test]
fn gain_passes_general_battery() {
    let mut gain = Gain::new();
    run_general_battery(&mut gain, 48_000.0).expect("gain passes the general battery");
}

#[test]
fn gain_process_is_allocation_free() {
    let mut gain = Gain::new();
    assert_allocation_free(&mut gain, 512);
}
