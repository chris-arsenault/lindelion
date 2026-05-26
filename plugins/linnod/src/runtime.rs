use lindelion_dsp_utils::{
    db_to_gain,
    envelope::{Adsr, AdsrState, EnvelopePhase},
    equal_power_pan,
    filters::OnePoleLowpass,
    interpolation,
    math::{ms_to_samples, semitones_to_ratio, snap_to_zero},
    playback::{PlaybackCursor, PlaybackDirection, PlaybackRegion, playback_increment},
    soft_saturate,
};
use lindelion_pitch_shift::{PitchShiftEngine, PitchShiftRatios, PitchShiftRegionSampleRequest};
use lindelion_plugin_shell::{
    ControlEvent, MidiEvent, MidiExpressionSource, MidiExpressionUpdate, MidiVoiceExpression,
    NoteEvent, VoiceLike, VoiceManager, VoiceRenderStatus,
};

use crate::{
    SourceAnalysis,
    patch::{
        ChokeGroupId, EnvelopeConfig, LinnodPatch, PlaybackMode, SLICE_COUNT, SliceParams,
        TriggerMode, pad_assignment_for_note,
    },
};

const MAX_VOICES: usize = 16;
const DEFAULT_PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
const IDLE_LEVEL_THRESHOLD: f32 = 1.0e-6;
const OUTPUT_BOUND: f32 = 4.0;
const IDENTITY_PITCH_RATIO_EPSILON: f32 = 1.0e-4;

#[derive(Debug)]
pub(crate) struct LinnodProcessor {
    engine: LinnodEngine,
    expression_source: MidiExpressionSource<MAX_VOICES>,
    sample_rate: f32,
}

impl LinnodProcessor {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        Self {
            engine: LinnodEngine::new(sample_rate),
            expression_source: MidiExpressionSource::default(),
            sample_rate,
        }
    }

    pub(crate) fn reset(&mut self, sample_rate: f32) {
        *self = Self::new(sample_rate);
    }

    pub(crate) fn clear_voices(&mut self) {
        self.engine.clear_all();
        self.expression_source = MidiExpressionSource::default();
    }

    pub(crate) fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub(crate) fn process(
        &mut self,
        patch: &LinnodPatch,
        analysis: Option<&SourceAnalysis>,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        left.fill(0.0);
        right.fill(0.0);
        let Some(analysis) = analysis else {
            self.clear_voices();
            return;
        };

        for event in events {
            self.handle_event(patch, analysis, *event);
        }

        self.engine.render_add(analysis, left, right);
        apply_master_output(patch, left, right);
    }

    fn handle_event(&mut self, patch: &LinnodPatch, analysis: &SourceAnalysis, event: MidiEvent) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => self.note_on(patch, analysis, channel, note, velocity),
            MidiEvent::Note(NoteEvent::On { channel, note, .. })
            | MidiEvent::Note(NoteEvent::Off { channel, note, .. }) => {
                self.engine.note_off(channel, note);
            }
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    fn note_on(
        &mut self,
        patch: &LinnodPatch,
        analysis: &SourceAnalysis,
        channel: u8,
        note: u8,
        velocity: f32,
    ) {
        let Some(trigger) =
            voice_trigger_from_note(patch, analysis, note, self.sample_rate, velocity)
        else {
            return;
        };
        if matches!(patch.trigger_mode, TriggerMode::Pad) {
            if let Some(choke_group) = trigger.choke_group {
                self.engine.choke_group(channel, choke_group);
            } else {
                self.engine.choke_note(channel, note);
            }
        }
        let expression = self.expression_source.note_expression(channel, velocity);
        let slot = self.engine.note_on(channel, note, expression, trigger);
        self.expression_source
            .begin_voice(slot as u32, channel, velocity);
    }

    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::PolyPressure {
                channel,
                note,
                value,
            } => self.engine.set_poly_pressure(channel, note, value),
            _ => {
                if let Some(update) = self
                    .expression_source
                    .apply_control(control, DEFAULT_PITCH_BEND_RANGE_SEMITONES)
                {
                    self.sync_expression_update_to_engine(update);
                }
            }
        }
    }

    fn sync_expression_update_to_engine(&mut self, update: MidiExpressionUpdate) {
        let expression = update.expression.sanitized();
        if update.channel == 0 {
            self.engine.set_expression_controls(expression);
        } else {
            self.engine
                .set_expression_controls_for_channel(update.channel, expression);
        }
    }
}

#[derive(Debug)]
struct LinnodEngine {
    voices: VoiceManager<MAX_VOICES, LinnodVoice>,
}

impl LinnodEngine {
    fn new(sample_rate: f32) -> Self {
        Self {
            voices: VoiceManager::new(MAX_VOICES, || LinnodVoice::new(sample_rate)),
        }
    }

    fn active_voice_count(&self) -> usize {
        self.voices.active_voice_count()
    }

    fn note_on(
        &mut self,
        channel: u8,
        note: u8,
        expression: MidiVoiceExpression,
        trigger: LinnodVoiceTrigger,
    ) -> usize {
        self.voices
            .start_voice(channel, note, expression, true, |voice| {
                voice.trigger(trigger)
            })
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        self.voices.release_note_for_channel(channel, note);
    }

    fn choke_note(&mut self, channel: u8, note: u8) {
        self.voices.clear_note_for_channel(channel, note);
    }

    fn choke_group(&mut self, channel: u8, group: ChokeGroupId) {
        let channel = channel.min(15);
        self.voices.clear_voices_where(|slot| {
            slot.channel == Some(channel) && slot.voice.choke_group() == Some(group)
        });
    }

    fn clear_all(&mut self) {
        self.voices.clear_all();
    }

    fn set_expression_controls(&mut self, expression: MidiVoiceExpression) {
        let expression = expression.sanitized();
        self.voices.set_expression_controls(
            expression.stream.pitch_bend,
            expression.stream.pressure,
            expression.stream.brightness,
            expression.mod_wheel,
        );
    }

    fn set_expression_controls_for_channel(
        &mut self,
        channel: u8,
        expression: MidiVoiceExpression,
    ) {
        let expression = expression.sanitized();
        self.voices.set_expression_controls_for_channel(
            channel,
            expression.stream.pitch_bend,
            expression.stream.pressure,
            expression.stream.brightness,
            expression.mod_wheel,
        );
    }

    fn set_poly_pressure(&mut self, channel: u8, note: u8, value: f32) {
        self.voices.set_poly_pressure(channel, note, value);
    }

    fn render_add(&mut self, analysis: &SourceAnalysis, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        self.voices.process_live_voices(|voice| {
            let mut block_peak = 0.0_f32;
            for index in 0..len {
                let (sample_left, sample_right) = voice.process_stereo_sample(analysis);
                block_peak = block_peak.max(sample_left.abs()).max(sample_right.abs());
                left[index] = snap_to_zero(left[index] + sample_left);
                right[index] = snap_to_zero(right[index] + sample_right);
            }

            VoiceRenderStatus {
                last_level: block_peak,
                idle: voice.is_idle() && block_peak < IDLE_LEVEL_THRESHOLD,
            }
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct LinnodVoiceTrigger {
    slice_index: usize,
    source_start_sample: usize,
    source_end_sample: usize,
    cursor: PlaybackCursor,
    ratios: PitchShiftRatios,
    playback_mode: PlaybackMode,
    choke_group: Option<ChokeGroupId>,
    envelope: EnvelopeConfig,
    gain: f32,
    pan: f32,
    filter_cutoff: f32,
}

#[derive(Debug, Clone)]
struct LinnodVoice {
    sample_rate: f32,
    slice_index: usize,
    source_start_sample: usize,
    source_end_sample: usize,
    cursor: PlaybackCursor,
    ratios: PitchShiftRatios,
    playback_mode: PlaybackMode,
    choke_group: Option<ChokeGroupId>,
    envelope: EnvelopeConfig,
    envelope_state: AdsrState,
    filter: OnePoleLowpass,
    gain: f32,
    pan: f32,
    expression: MidiVoiceExpression,
    playback_finished: bool,
}

impl LinnodVoice {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            slice_index: 0,
            source_start_sample: 0,
            source_end_sample: 0,
            cursor: PlaybackCursor::finished(),
            ratios: PitchShiftRatios::identity(),
            playback_mode: PlaybackMode::OneShot,
            choke_group: None,
            envelope: EnvelopeConfig::default(),
            envelope_state: AdsrState::default(),
            filter: OnePoleLowpass::new(20_000.0, sample_rate),
            gain: 0.0,
            pan: 0.0,
            expression: MidiVoiceExpression::default(),
            playback_finished: true,
        }
    }

    fn trigger(&mut self, trigger: LinnodVoiceTrigger) {
        self.slice_index = trigger.slice_index;
        self.source_start_sample = trigger.source_start_sample;
        self.source_end_sample = trigger.source_end_sample;
        self.cursor = trigger.cursor;
        self.ratios = trigger.ratios.sanitized();
        self.playback_mode = trigger.playback_mode;
        self.choke_group = trigger.choke_group;
        self.envelope = trigger.envelope.sanitized();
        self.filter.reset();
        self.filter
            .set_cutoff(trigger.filter_cutoff, self.sample_rate);
        self.gain = trigger.gain;
        self.pan = trigger.pan;
        self.playback_finished = false;
        self.envelope_state.reset();
        self.envelope_state.note_on();
    }

    fn process_stereo_sample(&mut self, analysis: &SourceAnalysis) -> (f32, f32) {
        let envelope = self.next_envelope();
        if envelope <= 0.0 && self.is_idle() {
            return (0.0, 0.0);
        }

        let sample = self.next_source_sample(analysis);
        let sample = self.filter.process(sample) * envelope * self.gain * self.velocity_gain();
        equal_power_pan(sample, self.pan)
    }

    fn next_source_sample(&mut self, analysis: &SourceAnalysis) -> f32 {
        let Some(offset) = self.cursor.next_position() else {
            self.finish_playback();
            return 0.0;
        };
        let pitch_bend_ratio = semitones_to_ratio(self.expression.stream.pitch_bend);
        let ratios = PitchShiftRatios {
            pitch_ratio: self.ratios.pitch_ratio * pitch_bend_ratio,
            formant_ratio: self
                .ratios
                .formant_ratio
                .map(|formant_ratio| formant_ratio * pitch_bend_ratio),
        };
        if is_identity_pitch_request(ratios) {
            return direct_region_sample(
                analysis,
                self.source_start_sample,
                self.source_end_sample,
                offset,
            );
        }
        let request = PitchShiftRegionSampleRequest::new(
            self.source_start_sample,
            self.source_end_sample,
            offset,
            ratios,
        );
        PitchShiftEngine
            .render_region_sample(
                analysis.audio.samples(),
                &analysis.pitch_shift_cache,
                request,
            )
            .unwrap_or(0.0)
    }

    fn next_envelope(&mut self) -> f32 {
        self.envelope_state.next_sample(
            Adsr {
                attack_ms: self.envelope.attack_ms,
                decay_ms: self.envelope.decay_ms,
                sustain: self.envelope.sustain,
                release_ms: self.envelope.release_ms,
            },
            self.sample_rate,
        )
    }

    fn finish_playback(&mut self) {
        if self.playback_finished {
            return;
        }
        self.playback_finished = true;
        self.envelope_state.note_off();
    }

    fn velocity_gain(&self) -> f32 {
        self.expression.stream.velocity.clamp(0.0, 1.0)
    }

    fn is_idle(&self) -> bool {
        self.envelope_state.phase() == EnvelopePhase::Idle
    }

    fn choke_group(&self) -> Option<ChokeGroupId> {
        self.choke_group
    }
}

impl VoiceLike for LinnodVoice {
    type Expression = MidiVoiceExpression;

    fn set_expression(&mut self, expression: Self::Expression) {
        let previous_gate = self.expression.stream.gate;
        self.expression = expression.sanitized();
        if previous_gate
            && !self.expression.stream.gate
            && !matches!(self.playback_mode, PlaybackMode::OneShot)
        {
            self.envelope_state.note_off();
        }
    }

    fn clear(&mut self) {
        *self = Self::new(self.sample_rate);
    }
}

fn direct_region_sample(
    analysis: &SourceAnalysis,
    source_start_sample: usize,
    source_end_sample: usize,
    offset_samples: f32,
) -> f32 {
    let source = analysis.audio.samples();
    let source_start_sample = source_start_sample.min(source.len());
    let source_end_sample = source_end_sample.min(source.len()).max(source_start_sample);
    let duration = source_end_sample.saturating_sub(source_start_sample) as f32;
    if offset_samples < 0.0 || offset_samples >= duration {
        return 0.0;
    }
    interpolation::linear(source, source_start_sample as f32 + offset_samples)
}

fn is_identity_pitch_request(ratios: PitchShiftRatios) -> bool {
    (ratios.pitch_ratio - 1.0).abs() <= IDENTITY_PITCH_RATIO_EPSILON
        && ratios
            .formant_ratio
            .is_none_or(|ratio| (ratio - 1.0).abs() <= IDENTITY_PITCH_RATIO_EPSILON)
}

fn voice_trigger_from_note(
    patch: &LinnodPatch,
    analysis: &SourceAnalysis,
    note: u8,
    output_sample_rate: f32,
    _velocity: f32,
) -> Option<LinnodVoiceTrigger> {
    let resolved = resolve_note_trigger(patch, note)?;
    let slice = patch.slice(resolved.slice_index)?;
    let summary = analysis
        .pitch_shift_cache
        .slice_summary(resolved.slice_index)
        .copied()?;
    let source_sample_rate = analysis.audio.sample_rate();
    let playback = patch.effective_playback_config(resolved.slice_index);
    let source_start_sample = summary.start_sample;
    let source_end_sample = slice_playback_end_sample(
        playback.mode,
        summary.end_sample,
        analysis.audio.samples().len(),
    );
    let region = slice_playback_region(
        slice,
        playback.mode,
        source_start_sample,
        source_end_sample,
        analysis.pitch_shift_cache.sample_rate,
    );
    if region.is_empty() {
        return None;
    }

    let pitch_ratio = slice.pitch.ratio() * semitones_to_ratio(resolved.chromatic_semitones);
    Some(LinnodVoiceTrigger {
        slice_index: resolved.slice_index,
        source_start_sample,
        source_end_sample,
        cursor: PlaybackCursor::new(
            region,
            0.0,
            playback_increment(source_sample_rate, output_sample_rate, 1.0),
            if slice.reverse {
                PlaybackDirection::Reverse
            } else {
                PlaybackDirection::Forward
            },
            matches!(playback.mode, PlaybackMode::Looped),
        ),
        ratios: PitchShiftRatios {
            pitch_ratio,
            formant_ratio: Some(pitch_ratio),
        },
        playback_mode: playback.mode,
        choke_group: resolved.choke_group,
        envelope: playback.envelope,
        gain: db_to_gain(slice.gain_db),
        pan: slice.pan,
        filter_cutoff: slice.filter_cutoff,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NoteTriggerResolution {
    slice_index: usize,
    chromatic_semitones: f32,
    choke_group: Option<ChokeGroupId>,
}

fn resolve_note_trigger(patch: &LinnodPatch, note: u8) -> Option<NoteTriggerResolution> {
    match patch.trigger_mode {
        TriggerMode::Pad => {
            pad_assignment_for_note(&patch.pad_map, note).map(|assignment| NoteTriggerResolution {
                slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
                chromatic_semitones: 0.0,
                choke_group: assignment.choke_group,
            })
        }
        TriggerMode::Chromatic => {
            let slice_index = patch.selected_slice_index()?;
            let root_note = patch
                .pad_map
                .iter()
                .find(|assignment| {
                    assignment.pad.sanitized() == patch.active_chromatic_pad.sanitized()
                })
                .map(|assignment| assignment.midi_note)
                .unwrap_or(60);
            Some(NoteTriggerResolution {
                slice_index,
                chromatic_semitones: note as f32 - root_note as f32,
                choke_group: None,
            })
        }
    }
}

fn slice_playback_region(
    slice: &SliceParams,
    playback_mode: PlaybackMode,
    start_sample: usize,
    end_sample: usize,
    source_sample_rate: u32,
) -> PlaybackRegion {
    let duration = end_sample.saturating_sub(start_sample);
    let start_offset = ms_to_samples(slice.start_offset_ms, source_sample_rate).min(duration);
    let end_offset = if matches!(playback_mode, PlaybackMode::Continue) {
        0
    } else {
        ms_to_samples(slice.end_offset_ms, source_sample_rate).min(duration - start_offset)
    };
    PlaybackRegion::new(
        start_offset as f32,
        duration.saturating_sub(end_offset) as f32,
    )
}

fn slice_playback_end_sample(
    playback_mode: PlaybackMode,
    slice_end_sample: usize,
    source_len: usize,
) -> usize {
    if matches!(playback_mode, PlaybackMode::Continue) {
        source_len
    } else {
        slice_end_sample
    }
}

fn apply_master_output(patch: &LinnodPatch, left: &mut [f32], right: &mut [f32]) {
    let gain = db_to_gain(patch.output.master_gain_db);
    let len = left.len().min(right.len());
    for index in 0..len {
        left[index] = bounded_output(left[index] * gain);
        right[index] = bounded_output(right[index] * gain);
    }
}

fn bounded_output(sample: f32) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }
    soft_saturate(sample.clamp(-OUTPUT_BOUND, OUTPUT_BOUND), 0.1)
}

fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
