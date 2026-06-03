#![allow(unsafe_op_in_unsafe_fn)]

mod editor;
mod factory;
mod processor;

pub(super) use processor::LamathStringedVst3Processor;

pub const SUBCATEGORY: &str = "Instrument|Synth";
const MAX_BLOCK_EVENTS: usize = 256;
