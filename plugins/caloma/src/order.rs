//! The signal-order parameter: one of three curated chain topologies. Each order is a *starting
//! point* — selecting it loads that order's default tuning (mechanism in `patch.rs`). The order
//! value lives inside the patch (see ADR-0020). The three topology *compositions* themselves are a
//! later milestone (M3 / D2); this module only models the parameter and its three values.

use lindelion_plugin_shell::{ParameterCodec, ParameterInfo, ParameterRange};
use serde::{Deserialize, Serialize};

/// The three curated signal orders. Names are fixed; their slot compositions are defined at M3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignalOrder {
    /// Repair → tonal → dynamics → enhance → limit. The default order.
    #[default]
    Clarity,
    /// Enhancement before the dynamics so the compressor controls the enhanced signal.
    Broadcast,
    /// Minimal, transparency-first; enhancement slots present but disabled by the default patch.
    Light,
}

impl SignalOrder {
    /// Every order, in parameter-index order.
    pub const ALL: [SignalOrder; 3] = [Self::Clarity, Self::Broadcast, Self::Light];
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for SignalOrder {
        max: 2;
        fallback: Self::Clarity;
        0 => Self::Clarity, "Clarity";
        1 => Self::Broadcast, "Broadcast";
        2 => Self::Light, "Light";
    }
}

/// Stable parameter id for the signal-order parameter.
pub const ORDER_PARAM_ID: u32 = 0;

/// The signal-order parameter: a three-valued stepped parameter (0 = Clarity, 1 = Broadcast,
/// 2 = Light).
pub const ORDER_PARAM: ParameterInfo = ParameterInfo::stepped(
    ORDER_PARAM_ID,
    "Signal Order",
    "",
    ParameterRange::linear(0.0, 2.0, 0.0),
    2,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_the_three_orders() {
        assert_eq!(SignalOrder::ALL.len(), 3);
    }

    #[test]
    fn index_round_trips_for_every_order() {
        for order in SignalOrder::ALL {
            assert_eq!(SignalOrder::from_index(order.to_index()), order);
        }
    }

    #[test]
    fn plain_round_trips_for_every_order() {
        for order in SignalOrder::ALL {
            assert_eq!(SignalOrder::from_plain(order.plain()), order);
        }
    }

    #[test]
    fn from_plain_clamps_out_of_range_values() {
        assert_eq!(SignalOrder::from_plain(5.0), SignalOrder::Light);
        assert_eq!(SignalOrder::from_plain(-1.0), SignalOrder::Clarity);
    }

    #[test]
    fn labels_match_each_order() {
        assert_eq!(SignalOrder::Clarity.label(), "Clarity");
        assert_eq!(SignalOrder::Broadcast.label(), "Broadcast");
        assert_eq!(SignalOrder::Light.label(), "Light");
    }

    #[test]
    fn order_param_is_a_three_value_stepped_parameter() {
        assert_eq!(ORDER_PARAM.step_count, Some(2));
        assert_eq!(ORDER_PARAM.range, ParameterRange::linear(0.0, 2.0, 0.0));
        assert_eq!(ORDER_PARAM.id.0, ORDER_PARAM_ID);
    }
}
