use lindelion_sample_library::SampleReference;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CymbalPatch {
    pub material: f32,
    pub size: f32,
    pub damping: f32,
    pub tension: f32,
    pub strike_position: f32,
    pub pickup_spread: f32,
    pub output_gain_db: f32,
    pub excitation_sample: Option<SampleReference>,
}

impl Default for CymbalPatch {
    fn default() -> Self {
        Self {
            material: 0.65,
            size: 0.74,
            damping: 0.28,
            tension: 0.55,
            strike_position: 0.42,
            pickup_spread: 0.42,
            output_gain_db: -3.0,
            excitation_sample: None,
        }
    }
}

impl CymbalPatch {
    pub fn sanitized(mut self) -> Self {
        self.material = unit(self.material, Self::default().material);
        self.size = unit(self.size, Self::default().size);
        self.damping = unit(self.damping, Self::default().damping);
        self.tension = unit(self.tension, Self::default().tension);
        self.strike_position = unit(self.strike_position, Self::default().strike_position);
        self.pickup_spread = unit(self.pickup_spread, Self::default().pickup_spread);
        self.output_gain_db = if self.output_gain_db.is_finite() {
            self.output_gain_db.clamp(-24.0, 12.0)
        } else {
            Self::default().output_gain_db
        };
        self
    }
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}
