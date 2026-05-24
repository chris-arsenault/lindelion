use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LiveExcitationPolicy {
    config: LiveExcitationConfig,
}

impl LiveExcitationPolicy {
    pub(super) const fn new(config: LiveExcitationConfig) -> Self {
        Self { config }
    }

    fn uses_continuous(self) -> bool {
        matches!(
            self.config.mode,
            LiveExcitationMode::Continuous | LiveExcitationMode::ContinuousAndNoteLatched
        )
    }

    pub(super) fn continuous_block<'a>(self, sidechain: &'a [f32]) -> LiveExcitationBlock<'a> {
        if !self.uses_continuous() {
            return LiveExcitationBlock::disabled();
        }
        LiveExcitationBlock::from_mono_block(sidechain, self.config.gain_db)
    }

    fn uses_latch(self) -> bool {
        matches!(
            self.config.mode,
            LiveExcitationMode::NoteLatched | LiveExcitationMode::ContinuousAndNoteLatched
        )
    }

    pub(super) fn latch_capture<'a>(
        self,
        state: &'a LiveExcitationLatchRuntimeState,
        sidechain: &'a [f32],
        onset_offset: usize,
    ) -> Option<LiveExcitationLatchCapture<'a>> {
        if !self.uses_latch() || sidechain.is_empty() || state.capacity_samples() == 0 {
            return None;
        }

        Some(LiveExcitationLatchCapture::new(
            state.pre_roll(),
            sidechain,
            onset_offset,
            state.pre_roll_samples(),
            state.window_samples(),
            state.fade_samples(),
            self.config.gain_db,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LiveExcitationLatchRuntimeState {
    pre_roll: LiveExcitationPreRoll,
    capacity_samples: usize,
    pre_roll_samples: usize,
    window_samples: usize,
    fade_samples: usize,
}

impl LiveExcitationLatchRuntimeState {
    pub(super) fn new(sample_rate: f32, config: LiveExcitationConfig) -> Self {
        let sample_rate = sanitized_latch_sample_rate(sample_rate);
        let pre_roll_samples = ms_to_samples(
            finite_clamp(config.latch_pre_roll_ms, 0.0, 500.0, 20.0),
            sample_rate,
        );
        let window_samples = ms_to_samples(
            finite_clamp(config.latch_window_ms, 1.0, 2_000.0, 120.0),
            sample_rate,
        )
        .max(1);
        let fade_samples = ms_to_samples(
            finite_clamp(config.latch_fade_ms, 0.0, 250.0, 5.0),
            sample_rate,
        );
        let capacity_samples = pre_roll_samples.saturating_add(window_samples);

        Self {
            pre_roll: LiveExcitationPreRoll::with_capacity(pre_roll_samples),
            capacity_samples,
            pre_roll_samples,
            window_samples,
            fade_samples,
        }
    }

    pub(super) const fn capacity_samples(&self) -> usize {
        self.capacity_samples
    }

    pub(super) const fn pre_roll_samples(&self) -> usize {
        self.pre_roll_samples
    }

    pub(super) const fn window_samples(&self) -> usize {
        self.window_samples
    }

    pub(super) const fn fade_samples(&self) -> usize {
        self.fade_samples
    }

    pub(super) fn pre_roll(&self) -> &LiveExcitationPreRoll {
        &self.pre_roll
    }

    pub(super) fn push_sidechain_block(&mut self, sidechain: &[f32]) {
        if sidechain.is_empty() {
            return;
        }
        self.pre_roll.push_block(sidechain);
    }

    pub(super) fn reset_input(&mut self) {
        self.pre_roll.reset();
    }
}
