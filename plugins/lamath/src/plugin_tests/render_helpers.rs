fn render_default_note(master_gain_db: f32, saturation_drive: f32) -> Vec<f32> {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 16_384,
        mode: ProcessMode::Realtime,
    };
    let mut patch = ResonatorSynthPatch::default();
    patch.output.master_gain_db = master_gain_db;
    patch.output.saturation_drive = saturation_drive;
    let mut left = vec![0.0; 16_384];
    let mut right = vec![0.0; 16_384];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 100.0 / 127.0,
    })];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut left,
            right: &mut right,
        },
        &events,
    ));

    left
}

#[derive(Debug)]
struct RenderedClip {
    left: Vec<f32>,
    right: Vec<f32>,
    rms: f32,
    peak: f32,
}

#[derive(Debug, Clone, Copy)]
enum ScheduledActionKind {
    Event(MidiEvent),
    Parameter { id: u32, plain: f32 },
}

#[derive(Debug, Clone, Copy)]
struct ScheduledAction {
    block: usize,
    order: usize,
    kind: ScheduledActionKind,
}

fn render_qa_clip(sample_rate: f32, block_size: usize, mode: ProcessMode) -> RenderedClip {
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode,
    };
    let total_blocks = ((sample_rate * 8.0).ceil() as usize).div_ceil(block_size);
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(total_blocks * block_size);
    let mut right = Vec::with_capacity(total_blocks * block_size);
    let mut schedule = qa_clip_schedule(sample_rate, block_size, total_blocks);
    let mut cursor = 0;
    let mut events = Vec::with_capacity(16);

    schedule.sort_by_key(|action| (action.block, action.order));
    synth.reset(setup);

    for block in 0..total_blocks {
        events.clear();
        while cursor < schedule.len() && schedule[cursor].block == block {
            match schedule[cursor].kind {
                ScheduledActionKind::Event(event) => events.push(event),
                ScheduledActionKind::Parameter { id, plain } => {
                    set_parameter_plain(&mut synth, id, plain);
                }
            }
            cursor += 1;
        }

        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &events,
        );
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

fn render_automation_stress_clip() -> RenderedClip {
    let sample_rate = 48_000.0;
    let block_size = 128;
    let total_blocks = 160;
    let setup = ProcessSetup {
        sample_rate,
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(total_blocks * block_size);
    let mut right = Vec::with_capacity(total_blocks * block_size);

    synth.reset(setup);
    for block in 0..total_blocks {
        if (16..96).contains(&block) && block % 2 == 0 {
            let high = (block / 2) % 2 == 0;
            set_parameter_plain(&mut synth, 1, if high { 6.0 } else { -42.0 });
            set_parameter_plain(&mut synth, 52, if high { 0.99 } else { 0.1 });
            set_parameter_plain(&mut synth, 3, if high { 20_000.0 } else { 250.0 });
            set_parameter_plain(&mut synth, 6, if high { 0.9 } else { 0.0 });
            set_parameter_plain(&mut synth, 4, if high { 1.0 } else { 0.0 });
        }

        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        })];
        let events = if block == 0 { &note_on[..] } else { &[] };
        process_block(&mut synth, setup, &mut block_left, &mut block_right, events);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

fn qa_clip_schedule(
    sample_rate: f32,
    block_size: usize,
    total_blocks: usize,
) -> Vec<ScheduledAction> {
    let mut builder = ScheduleBuilder::new(sample_rate, block_size, total_blocks);

    builder.parameter(0.0, 4, 0.0);

    builder.note(0.00, 0.25, 36, 32.0 / 127.0);
    builder.note(0.50, 0.75, 48, 80.0 / 127.0);
    builder.note(1.00, 1.25, 60, 1.0);

    for note in [48, 52, 55] {
        builder.note(2.00, 2.20, note, 100.0 / 127.0);
    }
    for note in [36, 40, 43, 47, 48, 52, 55, 59] {
        builder.note(2.75, 3.00, note, 95.0 / 127.0);
    }

    builder.note(4.00, 5.85, 48, 100.0 / 127.0);
    builder.pitch_bend(4.50, -2.0);
    builder.pitch_bend(5.00, 0.0);
    builder.pitch_bend(5.50, 2.0);
    builder.pitch_bend(5.85, 0.0);

    builder.parameter(6.00, 1, -60.0);
    builder.note(6.00, 7.75, 60, 100.0 / 127.0);
    builder.parameter(6.25, 1, 0.0);
    builder.parameter(6.50, 1, 12.0);
    builder.parameter(6.75, 52, 0.1);
    builder.parameter(7.00, 52, 0.98);
    builder.parameter(7.25, 3, 20.0);
    builder.parameter(7.50, 3, 20_000.0);

    builder.into_schedule()
}

struct ScheduleBuilder {
    sample_rate: f32,
    block_size: usize,
    total_blocks: usize,
    order: usize,
    schedule: Vec<ScheduledAction>,
}

impl ScheduleBuilder {
    fn new(sample_rate: f32, block_size: usize, total_blocks: usize) -> Self {
        Self {
            sample_rate,
            block_size,
            total_blocks,
            order: 0,
            schedule: Vec::new(),
        }
    }

    fn note(&mut self, start_seconds: f32, end_seconds: f32, note: u8, velocity: f32) {
        self.event(
            start_seconds,
            MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note,
                velocity,
            }),
        );
        self.event(
            end_seconds,
            MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note,
                velocity: 0.0,
            }),
        );
    }

    fn pitch_bend(&mut self, seconds: f32, semitones: f32) {
        self.event(
            seconds,
            MidiEvent::Control(ControlEvent::PitchBend {
                channel: 0,
                semitones,
            }),
        );
    }

    fn parameter(&mut self, seconds: f32, id: u32, plain: f32) {
        let block = self.block_at(seconds);
        let order = self.next_order();
        self.schedule.push(ScheduledAction {
            block,
            order,
            kind: ScheduledActionKind::Parameter { id, plain },
        });
    }

    fn event(&mut self, seconds: f32, event: MidiEvent) {
        let block = self.block_at(seconds);
        let order = self.next_order();
        self.schedule.push(ScheduledAction {
            block,
            order,
            kind: ScheduledActionKind::Event(event),
        });
    }

    fn block_at(&self, seconds: f32) -> usize {
        ((seconds * self.sample_rate) as usize / self.block_size)
            .min(self.total_blocks.saturating_sub(1))
    }

    fn next_order(&mut self) -> usize {
        let current = self.order;
        self.order += 1;
        current
    }

    fn into_schedule(self) -> Vec<ScheduledAction> {
        self.schedule
    }
}

fn process_block(
    synth: &mut ResonatorSynth,
    setup: ProcessSetup,
    left: &mut [f32],
    right: &mut [f32],
    events: &[MidiEvent],
) {
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer { left, right },
        events,
    ));
}

fn render_single_note_rms(sample_rate: f32, block_size: usize, note: u8, velocity: f32) -> f32 {
    let rendered =
        render_single_note_with_params_and_note(&[], sample_rate, block_size, note, velocity);
    rendered.rms
}

fn render_single_note_left(
    sample_rate: f32,
    block_size: usize,
    note: u8,
    velocity: f32,
) -> Vec<f32> {
    render_single_note_with_params_and_note(&[], sample_rate, block_size, note, velocity).left
}

fn render_single_note_with_params(
    params: &[(u32, f32)],
    sample_rate: f32,
    block_size: usize,
) -> RenderedClip {
    render_single_note_with_params_and_note(params, sample_rate, block_size, 60, 100.0 / 127.0)
}

fn render_filter_mode(mode: f32) -> RenderedClip {
    render_single_note_with_params(&[(3, 1_200.0), (6, 0.35), (7, mode)], 48_000.0, 128)
}

fn assert_rendered_clip_is_finite_and_bounded(rendered: &RenderedClip) {
    assert_all_finite(&rendered.left);
    assert_all_finite(&rendered.right);
    assert!(rendered.rms > 0.000_001);
    assert!(rendered.peak < 8.0);
}

fn render_single_note_with_params_and_note(
    params: &[(u32, f32)],
    sample_rate: f32,
    block_size: usize,
    note: u8,
    velocity: f32,
) -> RenderedClip {
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(block_size * 96);
    let mut right = Vec::with_capacity(block_size * 96);

    synth.reset(setup);
    for (id, plain) in params {
        set_parameter_plain(&mut synth, *id, *plain);
    }

    for block in 0..96 {
        if block == 0 {
            process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &[MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note,
                    velocity,
                })],
            );
        } else {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        }
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

