#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixSide {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorModel {
    Modal,
    Waveguide,
}

impl ResonatorModel {
    fn from_config(config: ResonatorConfig) -> Self {
        match config {
            ResonatorConfig::Modal(_) => Self::Modal,
            ResonatorConfig::Waveguide(_) => Self::Waveguide,
        }
    }

    fn config_from(self, current: ResonatorConfig) -> ResonatorConfig {
        match self {
            Self::Modal => ResonatorConfig::Modal(modal_config_from(current)),
            Self::Waveguide => ResonatorConfig::Waveguide(waveguide_config_from(current)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingMode {
    Parallel,
    Series,
}

impl RoutingMode {
    fn from_routing(routing: ResonatorRouting) -> Self {
        match routing {
            ResonatorRouting::Parallel { .. } => Self::Parallel,
            ResonatorRouting::Series { .. } => Self::Series,
        }
    }

    fn apply_to(self, current: ResonatorRouting) -> ResonatorRouting {
        let mix_a = parallel_mix_a(current);
        let mix_b = parallel_mix_b(current);
        match self {
            Self::Parallel => ResonatorRouting::Parallel { mix_a, mix_b },
            Self::Series => ResonatorRouting::Series { mix_a, mix_b },
        }
    }
}

pub(crate) trait ParameterCodec: Copy + Sized {
    const MAX_INDEX: u32;

    fn from_index(index: u32) -> Self;
    fn to_index(self) -> u32;
    fn label(self) -> &'static str;

    fn from_plain(value: f32) -> Self {
        Self::from_index(stepped_index(value, Self::MAX_INDEX))
    }

    fn plain(self) -> f32 {
        self.to_index() as f32
    }

    fn label_from_plain(value: f32) -> &'static str {
        Self::from_plain(value).label()
    }
}

impl ParameterCodec for FilterMode {
    const MAX_INDEX: u32 = 2;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::BandPass,
            2 => Self::HighPass,
            _ => Self::LowPass,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::LowPass => 0,
            Self::BandPass => 1,
            Self::HighPass => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::BandPass => "BP",
            Self::HighPass => "HP",
        }
    }
}

impl ParameterCodec for RoutingMode {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::Series
        } else {
            Self::Parallel
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Parallel => 0,
            Self::Series => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Parallel => "Parallel",
            Self::Series => "Series",
        }
    }
}

impl ParameterCodec for ResonatorModel {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::Waveguide
        } else {
            Self::Modal
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Modal => 0,
            Self::Waveguide => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Modal => "Modal",
            Self::Waveguide => "Waveguide",
        }
    }
}

impl ParameterCodec for ModalPreset {
    const MAX_INDEX: u32 = 6;

    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Kalimba,
            1 => Self::Marimba,
            2 => Self::Bell,
            3 => Self::GlassBowl,
            4 => Self::MetalBar,
            5 => Self::Woodblock,
            _ => Self::GenericStrike,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Kalimba => 0,
            Self::Marimba => 1,
            Self::Bell => 2,
            Self::GlassBowl => 3,
            Self::MetalBar => 4,
            Self::Woodblock => 5,
            Self::GenericStrike => 6,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Kalimba => "Kalimba",
            Self::Marimba => "Marimba",
            Self::Bell => "Bell",
            Self::GlassBowl => "Glass Bowl",
            Self::MetalBar => "Metal Bar",
            Self::Woodblock => "Woodblock",
            Self::GenericStrike => "Generic",
        }
    }
}

impl ParameterCodec for LfoShape {
    const MAX_INDEX: u32 = 4;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Triangle,
            2 => Self::Saw,
            3 => Self::Square,
            4 => Self::SampleAndHold,
            _ => Self::Sine,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::Sine => 0,
            Self::Triangle => 1,
            Self::Saw => 2,
            Self::Square => 3,
            Self::SampleAndHold => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::Saw => "Saw",
            Self::Square => "Square",
            Self::SampleAndHold => "S&H",
        }
    }
}

impl ParameterCodec for ModulationSource {
    const MAX_INDEX: u32 = 5;

    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::SecondaryEnvelope,
            1 => Self::Lfo,
            2 => Self::Velocity,
            3 => Self::Aftertouch,
            4 => Self::ModWheel,
            _ => Self::Brightness,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::SecondaryEnvelope => 0,
            Self::Lfo => 1,
            Self::Velocity => 2,
            Self::Aftertouch => 3,
            Self::ModWheel => 4,
            Self::Brightness => 5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SecondaryEnvelope => "Secondary Env",
            Self::Lfo => "LFO",
            Self::Velocity => "Velocity",
            Self::Aftertouch => "Pressure",
            Self::ModWheel => "Mod Wheel",
            Self::Brightness => "Brightness",
        }
    }
}

impl ParameterCodec for ModulationDestination {
    const MAX_INDEX: u32 = 6;

    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::ResonatorADamping,
            2 => Self::ResonatorBDamping,
            3 => Self::ResonatorAPosition,
            4 => Self::ResonatorBPosition,
            5 => Self::ExcitationGain,
            6 => Self::LfoRate,
            _ => Self::FilterCutoff,
        }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::FilterCutoff => 0,
            Self::ResonatorADamping => 1,
            Self::ResonatorBDamping => 2,
            Self::ResonatorAPosition => 3,
            Self::ResonatorBPosition => 4,
            Self::ExcitationGain => 5,
            Self::LfoRate => 6,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FilterCutoff => "Filter Cutoff",
            Self::ResonatorADamping => "Res A Damping",
            Self::ResonatorBDamping => "Res B Damping",
            Self::ResonatorAPosition => "Res A Position",
            Self::ResonatorBPosition => "Res B Position",
            Self::ExcitationGain => "Excitation Gain",
            Self::LfoRate => "LFO Rate",
        }
    }
}

impl ParameterCodec for WaveguideStyle {
    const MAX_INDEX: u32 = 1;

    fn from_index(index: u32) -> Self {
        if index == 1 { Self::Tube } else { Self::String }
    }

    fn to_index(self) -> u32 {
        match self {
            Self::String => 0,
            Self::Tube => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Tube => "Tube",
        }
    }
}
