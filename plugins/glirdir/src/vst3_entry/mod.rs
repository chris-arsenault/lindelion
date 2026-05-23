#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod controller;
mod messages;
mod processor;

#[cfg(test)]
mod processor_tests;
#[cfg(test)]
mod tests;

use crate::PARAMETERS;

const VST3_PARAMETER_COUNT: usize = PARAMETERS.len();

use controller::GlirdirVst3Controller;
use messages::{GlirdirPluginMessage, GlirdirStatusPayload};
