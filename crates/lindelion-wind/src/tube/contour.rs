//! Legacy clarinet-contour comparator (internal switch, off in the shipped model): tracked
//! per-harmonic peaking EQ over the summed output.

use lindelion_dsp_utils::{filters::BiquadCoefficients, math};

use super::ReedTube;

const CLARINET_CONTOUR_Q: f32 = 10.0;
const CLARINET_CONTOUR_H2_DB: f32 = -18.0;
const CLARINET_CONTOUR_H3_DB: f32 = -6.0;
const CLARINET_CONTOUR_H4_DB: f32 = -16.0;
const CLARINET_CONTOUR_H5_DB: f32 = -18.0;
const CLARINET_CONTOUR_H6_DB: f32 = -10.0;
const CLARINET_CONTOUR_H7_DB: f32 = -12.0;
const CLARINET_CONTOUR_UPPER_HZ: f32 = 3_100.0;
const CLARINET_CONTOUR_UPPER_Q: f32 = 1.15;
const CLARINET_CONTOUR_UPPER_DB: f32 = 8.0;

impl ReedTube {
    pub(super) fn set_clarinet_contour(&mut self, frequency_hz: f32) {
        let frequency_hz = math::finite_clamp(frequency_hz, 20.0, self.sample_rate * 0.20, 220.0);
        self.contour_h2.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            2.0,
            CLARINET_CONTOUR_H2_DB,
        ));
        self.contour_h3.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            3.0,
            CLARINET_CONTOUR_H3_DB,
        ));
        self.contour_h4.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            4.0,
            CLARINET_CONTOUR_H4_DB,
        ));
        self.contour_h5.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            5.0,
            CLARINET_CONTOUR_H5_DB,
        ));
        self.contour_h6.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            6.0,
            CLARINET_CONTOUR_H6_DB,
        ));
        self.contour_h7.set_coefficients(harmonic_cut(
            self.sample_rate,
            frequency_hz,
            7.0,
            CLARINET_CONTOUR_H7_DB,
        ));
        self.contour_upper
            .set_coefficients(BiquadCoefficients::peaking(
                self.sample_rate,
                CLARINET_CONTOUR_UPPER_HZ,
                CLARINET_CONTOUR_UPPER_Q,
                CLARINET_CONTOUR_UPPER_DB,
            ));
    }

    pub(super) fn clarinet_contour_sample(&mut self, sample: f32) -> f32 {
        let sample = self.contour_h2.process(sample);
        let sample = self.contour_h3.process(sample);
        let sample = self.contour_h4.process(sample);
        let sample = self.contour_h5.process(sample);
        let sample = self.contour_h6.process(sample);
        let sample = self.contour_h7.process(sample);
        math::snap_to_zero(self.contour_upper.process(sample))
    }
}

fn harmonic_cut(
    sample_rate: f32,
    frequency_hz: f32,
    harmonic: f32,
    gain_db: f32,
) -> BiquadCoefficients {
    let cutoff_hz = math::finite_clamp(
        frequency_hz * harmonic,
        20.0,
        sample_rate * 0.45,
        frequency_hz,
    );
    BiquadCoefficients::peaking(sample_rate, cutoff_hz, CLARINET_CONTOUR_Q, gain_db)
}
