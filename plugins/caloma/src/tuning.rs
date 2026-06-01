//! Calóma's offline **default-tuning** harness (M5): pick each signal order's built-in default
//! parameter patch by optimizing measured full-chain output on the spoken-word fixture battery,
//! subject to hard no-artifact constraints. Per ADR-0012 this speech-specific tuning lives in
//! `caloma`, not the shared `crates/` foundations.
//!
//! The pure pieces — metric primitives ([`metrics`]), the scored/constrained objective, the search,
//! and the tuning configuration — run in the fast `make ci` suite. The full-chain battery runner
//! (which loads the NN models) is `#[ignore]`d and runs via `make tune-defaults` / `make
//! test-models` (ADR-0018, ADR-0001).

pub mod config;
pub mod harness;
pub mod metrics;
pub mod score;
pub mod search;
