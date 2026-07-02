//! Monophonic note lifecycle: MIDI event routing, the held-note stack (finger-lift return to
//! a still-held key), and the register-break dip transition that parks legato note changes
//! crossing the break until the bore is quiet.

#![allow(clippy::wildcard_imports)]

use super::*;

/// Most simultaneously-held keys the monophonic note stack remembers; releasing the sounding
/// note returns to the most recent key still down. 16 covers two hands with margin.
pub(super) const HELD_NOTE_CAPACITY: usize = 16;
// Register-break transition: a legato note change that crosses the break cannot keep the bore
// continuous — the vent topology and the bore mode ratio (1.0 <-> 2.994, a ~3x delay-line
// retune) both step, and stepping them under a ringing bore is an audible waveform snap. A
// player crossing the break makes a brief swell through near-silence instead. The transition
// holds the *old* fingering while the drive gate falls (6 ms time constant) and the bore is
// damped hard (loop gain scaled, reaching the bore through the tube's 8 ms parameter smoother),
// then switches fingering and note at the quiet bottom and swells back through the articulation's
// re-lock attack.
pub(super) const BREAK_DIP_SECONDS: f32 = 0.016;
pub(super) const BREAK_DAMP_LOOP_SCALE: f32 = 0.5;

impl TubeProcessor<'_> {
    pub(super) fn handle_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            let note = match *event {
                MidiEvent::Note(note) => note,
                MidiEvent::Control(control) => {
                    self.handle_control(control);
                    continue;
                }
            };
            match note {
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 => {
                    if let Some(slot) = keyswitch_slot(note) {
                        self.select_slot(slot);
                    } else {
                        self.note_on(note, velocity);
                    }
                }
                NoteEvent::Off { note, .. } => self.note_off(note),
                NoteEvent::On { note, .. } => self.note_off(note),
            }
        }
    }

    /// Host performance layer: the CC dynamics line (CC1/CC11) multiplies the
    /// breath intensity, channel pressure swells above it, pitch bend retunes
    /// the bore. Inert until the host sends something.
    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::ContinuousController {
                controller: 1 | 11,
                value,
                ..
            } => self.host_expression.set_expression(value),
            ControlEvent::ChannelPressure { value, .. } => {
                self.host_expression.set_aftertouch(value)
            }
            ControlEvent::PitchBend { semitones, .. } => {
                self.host_expression.set_bend_semitones(semitones)
            }
            _ => {}
        }
    }

    fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot.min(ARTICULATION_SLOT_COUNT - 1);
        self.patch.selected_articulation = self.selected_slot;
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.held_notes_push(note, velocity.clamp(0.0, 1.0));
        self.request_note(note, velocity, articulation_style(self.selected_slot));
    }

    fn note_off(&mut self, note: u8) {
        self.held_notes_remove(note);
        if let Some(pending) = self.pending_break_note {
            // The sounding note is already being left through the dip; only the pending
            // note's own release retargets the transition (to the next held key, or into
            // a plain release when none remain).
            if pending.note == note {
                if let Some((held, velocity)) = self.held_notes_top() {
                    self.pending_break_note = Some(PendingBreakNote {
                        note: held,
                        velocity,
                        style: HELD_RETURN,
                    });
                } else {
                    self.pending_break_note = None;
                    self.current_note = None;
                    self.phrase_engine.note_off();
                    self.drive_target = 0.0;
                }
            }
            return;
        }
        if self.current_note == Some(note) {
            if let Some((held, velocity)) = self.held_notes_top() {
                self.request_note(held, velocity, HELD_RETURN);
            } else {
                self.current_note = None;
                self.phrase_engine.note_off();
                self.drive_target = 0.0;
            }
        }
    }

    /// Route a note change: a legato change that crosses the register break parks the note
    /// behind the break dip (latest request wins while the dip runs); everything else starts
    /// immediately.
    fn request_note(&mut self, note: u8, velocity: f32, style: ArticulationStyle) {
        if self.pending_break_note.is_some() {
            self.pending_break_note = Some(PendingBreakNote {
                note,
                velocity,
                style,
            });
            return;
        }
        let crossing = self.current_note.is_some_and(|current| {
            self.register_active_for(current) != self.register_active_for(note)
        });
        if crossing {
            self.pending_break_note = Some(PendingBreakNote {
                note,
                velocity,
                style,
            });
            self.break_dip_remaining = (BREAK_DIP_SECONDS * self.model_sample_rate) as u32;
            self.drive_target = 0.0;
        } else {
            self.start_note(note, velocity, style);
        }
    }

    fn register_active_for(&self, note: u8) -> bool {
        register_key_state(&self.patch, Some(note)).vent_admittance > 0.0
    }

    fn start_note(&mut self, note: u8, velocity: f32, style: ArticulationStyle) {
        // Note overlap is the legato/rearticulation seam: an overlapping note
        // keeps the developed breath and blooming vibrato (slurred); a note
        // from silence is a fresh tongued attack — unless the articulation
        // itself is a slurred entry.
        let legato = self.current_note.is_some() || style.slurred;
        self.phrase_engine
            .note_on(velocity.clamp(0.0, 1.0), legato, PHRASE_DEVELOPMENT_START);
        self.current_note = Some(note);
        self.sounding_note = Some(note);
        self.frequency_hz = midi_note_to_hz(note as f32);
        self.effort = velocity.clamp(0.0, 1.0);
        self.drive_target = 1.0;
        // Tongue-release overpressure: every vented-register attack (including
        // legato note changes, which must re-lock the new mode) arms the boost
        // at the articulation's weight. Below the break the bore speaks
        // immediately, so the arm is the articulation's accent — zero for the
        // neutral tongue, which keeps the auditioned low-register start.
        let vented = register_key_state(&self.patch, self.sounding_note).vent_admittance > 0.0;
        let arm = if vented {
            style.vented_attack
        } else {
            style.low_attack
        };
        if arm > 0.0 {
            self.attack_envelope = self.attack_envelope.max(arm);
        }
        self.attack_coeff = attack_coeff(self.model_sample_rate, style.attack_tau_scale);
        self.gate_coeff = gate_coeff(self.model_sample_rate, style.gate_ramp_scale);
        self.effort_accent = style.effort_accent;
        self.onset_noise_envelope = self.onset_noise_envelope.max(style.onset_noise);
        let gain = self.effort.sqrt() * style.seed_gain;
        if gain > 0.0 {
            let source = self.sources[self.selected_slot];
            self.injector.trigger(source, gain, self.model_sample_rate);
        }
    }

    /// Per-model-sample break-transition step: while the dip runs, the drive gate is held shut
    /// and the bore is damped (returned loop-gain scale); at the bottom the parked note starts
    /// with the new fingering on a quiet bore.
    pub(super) fn process_break_transition(&mut self) -> f32 {
        let Some(pending) = self.pending_break_note else {
            return 1.0;
        };
        if self.break_dip_remaining > 0 {
            self.break_dip_remaining -= 1;
            return BREAK_DAMP_LOOP_SCALE;
        }
        self.pending_break_note = None;
        self.start_note(pending.note, pending.velocity, pending.style);
        1.0
    }

    fn held_notes_push(&mut self, note: u8, velocity: f32) {
        self.held_notes_remove(note);
        if self.held_note_count == HELD_NOTE_CAPACITY {
            self.held_notes.copy_within(1.., 0);
            self.held_note_count -= 1;
        }
        self.held_notes[self.held_note_count] = (note, velocity);
        self.held_note_count += 1;
    }

    fn held_notes_remove(&mut self, note: u8) {
        let Some(index) = self.held_notes[..self.held_note_count]
            .iter()
            .position(|&(held, _)| held == note)
        else {
            return;
        };
        self.held_notes.copy_within(index + 1.., index);
        self.held_note_count -= 1;
    }

    fn held_notes_top(&self) -> Option<(u8, f32)> {
        self.held_note_count
            .checked_sub(1)
            .map(|index| self.held_notes[index])
    }
}
