use lindelion_dsp_utils::{
    db_to_gain,
    filters::{Biquad, BiquadCoefficients},
    math::{finite_clamp, snap_to_zero},
    params::{StructuralChangePolicy, StructuralParam},
    soft_saturate,
};
use lindelion_plugin_shell::SmoothedAtomicParam;

use crate::{
    FILTER_CUTOFF_PARAMETER_ID, FILTER_RESONANCE_PARAMETER_ID, FilterMode,
    MASTER_GAIN_PARAMETER_ID, MASTER_PAN_PARAMETER_ID, OutputConfig, SATURATION_PARAMETER_ID,
    smoothed_runtime_parameter,
};

use super::structural_ramp_samples;
use crate::dsp::constants::{
    FILTER_CUTOFF_MOD_OCTAVES, MASTER_GAIN_DB, OUTPUT_FILTER_CUTOFF_HZ, OUTPUT_FILTER_Q,
};

const INTERNAL_HEADROOM_DB: f32 = -12.0;

#[derive(Debug)]
pub(super) struct OutputStage {
    pub(super) config: OutputConfig,
    pub(super) filter: Biquad,
    pub(super) filter_mode: StructuralParam<FilterMode>,
    pub(super) filter_cutoff: SmoothedAtomicParam,
    pub(super) filter_resonance: SmoothedAtomicParam,
    pub(super) master_gain: SmoothedAtomicParam,
    pub(super) saturation_drive: SmoothedAtomicParam,
    pub(super) master_pan: SmoothedAtomicParam,
}

impl OutputStage {
    pub(super) fn new(sample_rate: f32) -> Self {
        let config = OutputConfig::default();
        Self {
            config,
            filter: Biquad::new(output_filter_coefficients(
                sample_rate,
                config.filter_cutoff,
                config.filter_resonance,
                config.filter_mode,
            )),
            filter_mode: StructuralParam::with_ramp_samples(
                config.filter_mode,
                StructuralChangePolicy::LiveMuteRamp,
                structural_ramp_samples(sample_rate),
            ),
            filter_cutoff: output_filter_cutoff_param(sample_rate, config.filter_cutoff),
            filter_resonance: output_filter_resonance_param(sample_rate, config.filter_resonance),
            master_gain: master_gain_param(sample_rate, config.master_gain_db),
            saturation_drive: saturation_drive_param(sample_rate, config.saturation_drive),
            master_pan: master_pan_param(sample_rate, config.master_pan),
        }
    }

    pub(super) fn reset(&mut self, config: OutputConfig) {
        self.config = config;
        self.filter.reset();
        self.filter_mode.reset(config.filter_mode);
        self.filter_cutoff.reset_plain(config.filter_cutoff);
        self.filter_resonance.reset_plain(config.filter_resonance);
        self.master_gain.reset_plain(config.master_gain_db);
        self.saturation_drive.reset_plain(config.saturation_drive);
        self.master_pan.reset_plain(config.master_pan);
    }

    pub(super) fn clear(&mut self) {
        self.reset(self.config);
    }

    pub(super) fn set_config(&mut self, config: OutputConfig) {
        self.filter_mode.set_target(config.filter_mode);
        self.filter_cutoff.set_plain_target(config.filter_cutoff);
        self.filter_resonance
            .set_plain_target(config.filter_resonance);
        self.master_gain.set_plain_target(config.master_gain_db);
        self.saturation_drive
            .set_plain_target(config.saturation_drive);
        self.master_pan.set_plain_target(config.master_pan);
        self.config = config;
    }

    pub(super) fn apply_structural_transitions(&mut self) -> f32 {
        let filter_sample = self.filter_mode.next_sample();
        if filter_sample.change.is_some() {
            self.filter.reset();
        }
        filter_sample.gain
    }

    pub(super) fn process_sample(
        &mut self,
        input: f32,
        sample_rate: f32,
        cutoff_mod: f32,
        amp: f32,
        structural_gain: f32,
    ) -> f32 {
        let base_cutoff = self.filter_cutoff.next_sample();
        let filter_resonance = self.filter_resonance.next_sample();
        let filter_cutoff = finite_clamp(
            base_cutoff * 2.0_f32.powf(cutoff_mod * FILTER_CUTOFF_MOD_OCTAVES),
            OUTPUT_FILTER_CUTOFF_HZ.min,
            OUTPUT_FILTER_CUTOFF_HZ.max,
            base_cutoff,
        );
        self.filter.set_coefficients(output_filter_coefficients(
            sample_rate,
            filter_cutoff,
            filter_resonance,
            self.filter_mode.current(),
        ));
        let filtered = self.filter.process(input);
        let staged = filtered * db_to_gain(INTERNAL_HEADROOM_DB);
        let saturated = soft_saturate(staged, self.saturation_drive.next_sample());

        snap_to_zero(saturated * amp * self.master_gain.next_sample() * structural_gain)
    }

    pub(super) fn next_pan(&mut self) -> f32 {
        self.master_pan.next_sample()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn output_gain(gain_db: f32) -> f32 {
    db_to_gain(MASTER_GAIN_DB.clamp(gain_db))
}

fn sanitize_output_filter_cutoff(cutoff_hz: f32) -> f32 {
    OUTPUT_FILTER_CUTOFF_HZ.clamp(cutoff_hz)
}

fn output_filter_cutoff_param(sample_rate: f32, cutoff_hz: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(FILTER_CUTOFF_PARAMETER_ID, sample_rate, cutoff_hz)
}

fn output_filter_resonance_param(sample_rate: f32, resonance: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(FILTER_RESONANCE_PARAMETER_ID, sample_rate, resonance)
}

fn master_gain_param(sample_rate: f32, gain_db: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(MASTER_GAIN_PARAMETER_ID, sample_rate, gain_db)
}

fn saturation_drive_param(sample_rate: f32, drive: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(SATURATION_PARAMETER_ID, sample_rate, drive)
}

fn master_pan_param(sample_rate: f32, pan: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(MASTER_PAN_PARAMETER_ID, sample_rate, pan)
}

fn runtime_smoothed_param(id: u32, sample_rate: f32, initial_plain: f32) -> SmoothedAtomicParam {
    smoothed_runtime_parameter(id, sample_rate, initial_plain)
        .expect("live output parameter should have smoothing metadata")
}

fn output_filter_coefficients(
    sample_rate: f32,
    cutoff_hz: f32,
    resonance: f32,
    mode: FilterMode,
) -> BiquadCoefficients {
    let cutoff_hz = sanitize_output_filter_cutoff(cutoff_hz);
    let q = OUTPUT_FILTER_Q.from_resonance(resonance);
    match mode {
        FilterMode::LowPass => BiquadCoefficients::lowpass(sample_rate, cutoff_hz, q),
        FilterMode::BandPass => BiquadCoefficients::bandpass(sample_rate, cutoff_hz, q),
        FilterMode::HighPass => BiquadCoefficients::highpass(sample_rate, cutoff_hz, q),
    }
}
