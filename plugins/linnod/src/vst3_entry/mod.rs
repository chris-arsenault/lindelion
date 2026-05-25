#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod controller;
mod controller_helpers;
mod editor;
mod factory;
mod messages;
mod patch_edits;
mod processor;
mod processor_helpers;
mod processor_notifications;

#[cfg(test)]
mod tests;

use crate::parameters::PARAMETER_BINDING_COUNT;

const MAX_BLOCK_EVENTS: usize = 128;
const SUBCATEGORY: &str = crate::VST3_BUNDLE_METADATA.vst3_sub_categories;
const VST3_PARAMETER_COUNT: usize = PARAMETER_BINDING_COUNT;

use controller::LinnodVst3Controller;
#[cfg(test)]
use messages::LinnodMessageKind;
use messages::{LinnodPluginMessage, LinnodStatusPayload};
use processor::LinnodVst3Processor;
