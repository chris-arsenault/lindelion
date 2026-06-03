#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]

mod editor;
mod factory;
mod processor;

pub(super) use processor::LamathCymbalVst3Processor;

pub(super) const SUBCATEGORY: &str = crate::VST3_BUNDLE_METADATA.vst3_sub_categories;
pub(super) const MAX_BLOCK_EVENTS: usize = 256;
