#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
// Lúmedir's live VST3 entry-point target is Windows (ADR-0023); on other hosts the COM scaffold
// is compiled (and `make ci`-tested) but unused, so allow dead code off-Windows.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

mod editor;
mod factory;
mod processor;

const SUBCATEGORY: &str = crate::VST3_BUNDLE_METADATA.vst3_sub_categories;

use processor::LumedirVst3Processor;
