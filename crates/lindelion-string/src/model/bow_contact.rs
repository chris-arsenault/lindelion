//! Bow-contact coupling onto the string: the contact excitation, the captured-cycle
//! intonation servo, and the per-sample wave correction.

#![allow(clippy::wildcard_imports)]

use super::*;

impl StringModel {
    pub(super) fn clear_bow_contact_state(&mut self) {
        self.bow_force = 0.0;
        self.bow_sticking = false;
        self.bow_slip_direction = 0.0;
        self.bow_incoming_left = [0.0; bow::BOW_READ_ADVANCE_SAMPLES];
        self.bow_incoming_right = [0.0; bow::BOW_READ_ADVANCE_SAMPLES];
        self.bow_incoming_index = 0;
        self.bow_noise_state = BOW_NOISE_SEED;
        self.bow_noise_lp = 0.0;
        self.bow_torsion_mean_force = 0.0;
        self.bow_samples_since_capture = 0.0;
        self.bow_last_capture_interval = 0.0;
    }

    /// Relax the bowed-intonation correction toward neutral (bow not engaged).
    pub(super) fn relax_bow_tuning(&mut self) {
        self.bow_tune_target = 1.0;
        self.bow_tune_correction +=
            self.bow_tune_release_coefficient * (1.0 - self.bow_tune_correction);
    }

    /// One servo step of the bowed-intonation correction (see
    /// `bow::BOW_TUNE_SERVO_SECONDS`). `expected_period` is the played
    /// target's fundamental period in samples; `captured` marks a slip→stick
    /// transition (one per steady Helmholtz cycle).
    pub(super) fn update_bow_tuning(&mut self, captured: bool, expected_period: f32) {
        self.bow_samples_since_capture += 1.0;
        if captured {
            let interval = self.bow_samples_since_capture;
            self.bow_samples_since_capture = 0.0;
            let (window_low, window_high) = bow::BOW_TUNE_INTERVAL_WINDOW;
            let plausible = expected_period > 1.0
                && interval > window_low * expected_period
                && interval < window_high * expected_period;
            let stable = self.bow_last_capture_interval > 0.0
                && (interval - self.bow_last_capture_interval).abs()
                    <= bow::BOW_TUNE_INTERVAL_TOLERANCE * self.bow_last_capture_interval;
            if plausible && stable {
                self.bow_tune_target = math::finite_clamp(
                    self.bow_tune_correction * expected_period / interval,
                    bow::BOW_TUNE_FACTOR_MIN,
                    bow::BOW_TUNE_FACTOR_MAX,
                    1.0,
                );
            }
            self.bow_last_capture_interval = if plausible { interval } else { 0.0 };
        }
        self.bow_tune_correction +=
            self.bow_tune_coefficient * (self.bow_tune_target - self.bow_tune_correction);
    }

    pub(super) fn bow_contact_excitation(
        &mut self,
        one_way_delay: f32,
        expected_period: f32,
        drive: BowContactDrive,
    ) -> Option<BowContactCorrection> {
        let position = math::finite_clamp(drive.params.position, 0.001, 0.999, 0.12);
        let pressure = math::finite_clamp(drive.params.pressure, 0.0, 1.0, 0.48);
        let speed = math::finite_clamp(drive.params.speed, 0.0, 1.0, 0.45);
        let friction = math::finite_clamp(drive.params.friction, 0.0, 1.0, 0.45);
        let effort = math::finite_clamp(drive.effort, 0.0, 1.0, 0.0);
        let gate = math::finite_clamp(drive.drive_gate, 0.0, 1.0, 0.0);
        // Loudness rides on bow speed (Helmholtz amplitude ∝ v_bow): played
        // effort scales speed and normal force together, holding the contact at
        // the same point of the Schelleng cone across the dynamic range. The
        // gate ramps both from zero, so the attack is the bow accelerating onto
        // the string — there is no separate excitation injection.
        let effort_scale = bow::BOW_EFFORT_FLOOR + (1.0 - bow::BOW_EFFORT_FLOOR) * effort;
        // The arm's weight engages with the stroke: force follows the stroke
        // magnitude so a starting or reversing bow does not press full weight
        // into a stationary string — at zero speed the Schelleng maximum
        // collapses and full weight is a hard scrape (see
        // `bow::BOW_STROKE_WEIGHT_FLOOR`).
        let stroke = math::finite_clamp(drive.stroke, -1.0, 1.0, 1.0);
        let stroke_engagement =
            bow::BOW_STROKE_WEIGHT_FLOOR + (1.0 - bow::BOW_STROKE_WEIGHT_FLOOR) * stroke.abs();
        let normal_force =
            bow::BOW_NORMAL_FORCE_MAX * pressure * effort_scale * gate * stroke_engagement;
        if normal_force <= f32::EPSILON {
            self.clear_bow_contact_state();
            self.relax_bow_tuning();
            return None;
        }
        // Signed by the stroke: down-/up-bows alternate on rearticulation and
        // the speed passes through zero at a bow change (the kernel is
        // direction-symmetric).
        let bow_velocity = (bow::BOW_SPEED_MIN + (bow::BOW_SPEED_MAX - bow::BOW_SPEED_MIN) * speed)
            * effort_scale
            * gate
            * stroke;
        let slip_velocity = bow::BOW_SMOOTH_SLIP_VELOCITY
            + (bow::BOW_SHARP_SLIP_VELOCITY - bow::BOW_SMOOTH_SLIP_VELOCITY) * friction;
        // Keep the contact (and its advanced reads) clear of the string ends.
        // A fixed clearance in samples mirrors a real bow's fixed distance from
        // the bridge: on short (high) strings the relative bowing point grows.
        let clearance = bow::BOW_BOUNDARY_CLEARANCE_SAMPLES / one_way_delay.max(1.0);
        let position = if clearance < 0.5 {
            position.clamp(clearance, 1.0 - clearance)
        } else {
            0.5
        };

        let advanced = self.waves.contact_average_samples(
            one_way_delay,
            position,
            BOW_CONTACT_HALF_WIDTH,
            bow::BOW_READ_ADVANCE_SAMPLES as f32,
        );
        // Incoming-wave (history) velocity at the contact: the advanced upstream
        // reads, delayed back to the contact through the FIFO, summed across the
        // rails. Exact in the discrete waveguide — no numerical differentiation,
        // and never contaminated by the contact's own outgoing corrections.
        let fifo_index = self.bow_incoming_index;
        let history_velocity =
            self.bow_incoming_left[fifo_index] + self.bow_incoming_right[fifo_index];
        self.bow_incoming_left[fifo_index] = advanced.left;
        self.bow_incoming_right[fifo_index] = advanced.right;
        self.bow_incoming_index = (fifo_index + 1) % bow::BOW_READ_ADVANCE_SAMPLES;
        // Rosin noise: the sliding friction fluctuates with the bow-hair/rosin
        // surface, re-excited each slip phase — the pitch-synchronous broadband
        // bed a real bowed tone carries between its harmonics.
        let slip_friction_scale = 1.0 + bow::BOW_SLIP_NOISE_DEPTH * self.next_bow_noise();
        let solved = bow::solve_bow_contact(bow::BowContactInputs {
            history_velocity,
            bow_velocity,
            normal_force,
            slip_velocity,
            slip_friction_scale,
            torsion_relief_velocity: bow::BOW_TORSIONAL_ADMITTANCE * self.bow_torsion_mean_force,
            was_sticking: self.bow_sticking,
            previous_slip_direction: self.bow_slip_direction,
        });
        self.bow_torsion_mean_force +=
            self.bow_torsion_coefficient * (solved.force - self.bow_torsion_mean_force);
        // A slip→stick capture marks one Helmholtz cycle for the intonation servo.
        let captured = solved.sticking && !self.bow_sticking;
        self.update_bow_tuning(captured, expected_period);
        self.bow_force = solved.force;
        self.bow_sticking = solved.sticking;
        self.bow_slip_direction = solved.slip_direction;

        let correction = math::finite_clamp(
            solved.velocity_correction,
            -bow::BOW_CORRECTION_LIMIT,
            bow::BOW_CORRECTION_LIMIT,
            0.0,
        );
        (correction != 0.0).then_some(BowContactCorrection {
            position,
            half_width: BOW_CONTACT_HALF_WIDTH,
            wave_correction: correction,
        })
    }

    pub(super) fn apply_bow_correction(
        &mut self,
        one_way_delay: f32,
        correction: Option<BowContactCorrection>,
    ) {
        if let Some(contact) = correction {
            self.waves.apply_bow_contact_correction(
                one_way_delay,
                contact.position,
                contact.half_width,
                contact.wave_correction,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BowContactCorrection {
    pub(super) position: f32,
    pub(super) half_width: f32,
    pub(super) wave_correction: f32,
}
