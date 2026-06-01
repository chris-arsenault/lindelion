#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
// Calóma's live VST3 entry-point target is Windows (ADR-0023); on other hosts the COM scaffold is
// compiled (and `make ci`-tested) but unused, so allow dead code off-Windows.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

//! Calóma's VST3 entry point: a **single-component** object (`plugin_object`) registered as one
//! class by `factory`. The host loads the audio class and `queryInterface`s the controller on the
//! same object, so the Vizia editor and the DSP share one `Caloma` (and its `SharedControls`) with
//! no host-relayed parameter channel (ADR-0023). The editor view (`createView`/`editor.rs` +
//! `lindelion-ui::caloma_vizia`) lands in M4 Step 6.

mod editor;
mod factory;
mod plugin_object;

const SUBCATEGORY: &str = crate::VST3_BUNDLE_METADATA.vst3_sub_categories;

use plugin_object::CalomaVst3Plugin;
