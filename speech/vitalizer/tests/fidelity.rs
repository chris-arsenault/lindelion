//! Vitalizer Mk2-T against the shared general-signal fidelity battery.

use lindelion_fidelity::{assert_allocation_free, run_general_battery};
use lindelion_speech_vitalizer::Vitalizer;

lindelion_test_allocator::install_test_allocator!();

#[test]
fn passes_general_battery() {
    let mut effect = Vitalizer::new();
    run_general_battery(&mut effect, 48_000.0).expect("vitalizer passes the general battery");
}

#[test]
fn process_is_allocation_free() {
    let mut effect = Vitalizer::new();
    assert_allocation_free(&mut effect, 512);
}
