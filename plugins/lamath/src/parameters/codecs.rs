#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixSide {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorModel {
    Modal,
    Waveguide,
    Mesh,
}

impl ResonatorModel {
    fn from_config(config: ResonatorConfig) -> Self {
        match config {
            ResonatorConfig::Modal(_) => Self::Modal,
            ResonatorConfig::Waveguide(_) => Self::Waveguide,
            ResonatorConfig::Mesh(_) => Self::Mesh,
        }
    }

    fn config_from(self, current: ResonatorConfig) -> ResonatorConfig {
        match self {
            Self::Modal => ResonatorConfig::Modal(modal_config_from(current)),
            Self::Waveguide => ResonatorConfig::Waveguide(waveguide_config_from(current)),
            Self::Mesh => ResonatorConfig::Mesh(mesh_config_from(current)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingMode {
    Parallel,
    Series,
    BodyColor,
}

impl RoutingMode {
    fn from_routing(routing: ResonatorRouting) -> Self {
        match routing {
            ResonatorRouting::Parallel { .. } => Self::Parallel,
            ResonatorRouting::Series { .. } => Self::Series,
            ResonatorRouting::BodyColor { .. } => Self::BodyColor,
        }
    }

    fn apply_to(self, current: ResonatorRouting) -> ResonatorRouting {
        let mix_a = parallel_mix_a(current);
        let mix_b = parallel_mix_b(current);
        match self {
            Self::Parallel => ResonatorRouting::Parallel { mix_a, mix_b },
            Self::Series => ResonatorRouting::Series { mix_a, mix_b },
            Self::BodyColor => ResonatorRouting::BodyColor { mix_a, mix_b },
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for AudioInputMode {
        max: 2;
        fallback: Self::Off;
        0 => Self::Off, "Off";
        1 => Self::AudioCreatesNotes, "Audio Notes";
        2 => Self::MidiPlusAudioCreatesNotes, "MIDI + Audio";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for LiveExcitationMode {
        max: 3;
        fallback: Self::Off;
        0 => Self::Off, "Off";
        1 => Self::Continuous, "Continuous";
        2 => Self::NoteLatched, "Note Latched";
        3 => Self::ContinuousAndNoteLatched, "Cont + Latch";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for FilterMode {
        max: 2;
        fallback: Self::LowPass;
        0 => Self::LowPass, "LP";
        1 => Self::BandPass, "BP";
        2 => Self::HighPass, "HP";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for RoutingMode {
        max: 2;
        fallback: Self::Parallel;
        0 => Self::Parallel, "Parallel";
        1 => Self::Series, "Series";
        2 => Self::BodyColor, "Body Color";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshParameter {
    Material,
    Size,
    Damping,
    Tension,
    PickupSpread,
}

impl MeshParameter {
    fn plain_value(self, config: MeshConfig) -> f32 {
        match self {
            Self::Material => config.material,
            Self::Size => config.size,
            Self::Damping => config.damping,
            Self::Tension => config.tension,
            Self::PickupSpread => config.pickup_spread,
        }
    }

    fn apply_if_selected(self, config: &mut ResonatorConfig, value: f32) {
        if let ResonatorConfig::Mesh(mesh) = config {
            self.apply_plain(mesh, value);
        }
    }

    fn apply_plain(self, config: &mut MeshConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.5);
        match self {
            Self::Material => config.material = value,
            Self::Size => config.size = value,
            Self::Damping => config.damping = value,
            Self::Tension => config.tension = value,
            Self::PickupSpread => config.pickup_spread = value,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ResonatorModel {
        max: 2;
        fallback: Self::Modal;
        0 => Self::Modal, "Modal";
        1 => Self::Waveguide, "Waveguide";
        2 => Self::Mesh, "Mesh";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ModalPreset {
        max: 6;
        fallback: Self::GenericStrike;
        0 => Self::Kalimba, "Kalimba";
        1 => Self::Marimba, "Marimba";
        2 => Self::Bell, "Bell";
        3 => Self::GlassBowl, "Glass Bowl";
        4 => Self::MetalBar, "Metal Bar";
        5 => Self::Woodblock, "Woodblock";
        6 => Self::GenericStrike, "Generic";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for LfoShape {
        max: 4;
        fallback: Self::Sine;
        0 => Self::Sine, "Sine";
        1 => Self::Triangle, "Triangle";
        2 => Self::Saw, "Saw";
        3 => Self::Square, "Square";
        4 => Self::SampleAndHold, "S&H";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ModulationSource {
        max: 5;
        fallback: Self::Brightness;
        0 => Self::SecondaryEnvelope, "Secondary Env";
        1 => Self::Lfo, "LFO";
        2 => Self::Velocity, "Velocity";
        3 => Self::Aftertouch, "Pressure";
        4 => Self::ModWheel, "Mod Wheel";
        5 => Self::Brightness, "Brightness";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ModulationDestination {
        max: 6;
        fallback: Self::FilterCutoff;
        0 => Self::FilterCutoff, "Filter Cutoff";
        1 => Self::ResonatorADamping, "Res A Damping";
        2 => Self::ResonatorBDamping, "Res B Damping";
        3 => Self::ResonatorAPosition, "Res A Position";
        4 => Self::ResonatorBPosition, "Res B Position";
        5 => Self::ExcitationGain, "Excitation Gain";
        6 => Self::LfoRate, "LFO Rate";
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for WaveguideStyle {
        max: 1;
        fallback: Self::String;
        0 => Self::String, "String";
        1 => Self::Tube, "Tube";
    }
}
