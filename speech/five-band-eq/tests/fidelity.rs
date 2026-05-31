//! 5-Band EQ against the shared general-signal fidelity battery.

use lindelion_fidelity::{assert_allocation_free, run_general_battery};
use lindelion_speech_five_band_eq::FiveBandEq;

lindelion_test_allocator::install_test_allocator!();

#[test]
fn passes_general_battery() {
    let mut effect = FiveBandEq::new();
    run_general_battery(&mut effect, 48_000.0).expect("5-band EQ passes the general battery");
}

#[test]
fn process_is_allocation_free() {
    let mut effect = FiveBandEq::new();
    assert_allocation_free(&mut effect, 512);
}
