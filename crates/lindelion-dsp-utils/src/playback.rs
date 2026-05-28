use crate::interpolation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackRegion {
    start_sample: f32,
    end_sample: f32,
}

impl PlaybackRegion {
    pub fn new(start_sample: f32, end_sample: f32) -> Self {
        let start_sample = finite_non_negative(start_sample);
        let end_sample = finite_non_negative(end_sample).max(start_sample);
        Self {
            start_sample,
            end_sample,
        }
    }

    pub fn full(sample_count: usize) -> Self {
        Self {
            start_sample: 0.0,
            end_sample: sample_count as f32,
        }
    }

    pub const fn start_sample(self) -> f32 {
        self.start_sample
    }

    pub const fn end_sample(self) -> f32 {
        self.end_sample
    }

    pub fn duration_samples(self) -> f32 {
        (self.end_sample - self.start_sample).max(0.0)
    }

    pub fn is_empty(self) -> bool {
        self.duration_samples() <= f32::EPSILON
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackCursor {
    region: PlaybackRegion,
    relative_position: f32,
    unwrapped_relative_position: f32,
    increment: f32,
    direction: PlaybackDirection,
    looped: bool,
    finished: bool,
}

impl PlaybackCursor {
    pub const fn finished() -> Self {
        Self {
            region: PlaybackRegion {
                start_sample: 0.0,
                end_sample: 0.0,
            },
            relative_position: 0.0,
            unwrapped_relative_position: 0.0,
            increment: 1.0,
            direction: PlaybackDirection::Forward,
            looped: false,
            finished: true,
        }
    }

    pub fn forward(
        sample_count: usize,
        start_offset_samples: f32,
        increment: f32,
        looped: bool,
    ) -> Self {
        Self::new(
            PlaybackRegion::full(sample_count),
            start_offset_samples,
            increment,
            PlaybackDirection::Forward,
            looped,
        )
    }

    pub fn new(
        region: PlaybackRegion,
        start_offset_samples: f32,
        increment: f32,
        direction: PlaybackDirection,
        looped: bool,
    ) -> Self {
        let relative_position = finite_non_negative(start_offset_samples);
        Self {
            region,
            relative_position,
            unwrapped_relative_position: relative_position,
            increment: finite_non_negative(increment),
            direction,
            looped,
            finished: region.is_empty(),
        }
    }

    pub const fn is_finished(self) -> bool {
        self.finished
    }

    pub const fn direction(self) -> PlaybackDirection {
        self.direction
    }

    pub fn next_position(&mut self) -> Option<f32> {
        self.next_position_with_unwrapped()
            .map(|(position, _)| position)
    }

    pub fn next_position_with_unwrapped(&mut self) -> Option<(f32, f32)> {
        if self.finished {
            return None;
        }

        let position = self.current_position();
        let unwrapped = self.unwrapped_relative_position;
        self.advance();
        Some((position, unwrapped))
    }

    fn current_position(self) -> f32 {
        match self.direction {
            PlaybackDirection::Forward => self.region.start_sample + self.relative_position,
            PlaybackDirection::Reverse => self.region.end_sample - 1.0 - self.relative_position,
        }
    }

    fn advance(&mut self) {
        self.relative_position += self.increment;
        self.unwrapped_relative_position += self.increment;
        let duration = self.region.duration_samples();
        if self.relative_position < duration {
            return;
        }

        if self.looped && duration > f32::EPSILON {
            self.relative_position = self.relative_position.rem_euclid(duration);
        } else {
            self.finished = true;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplePlayback<'a> {
    samples: &'a [f32],
    cursor: PlaybackCursor,
}

impl<'a> SamplePlayback<'a> {
    pub fn new(
        samples: &'a [f32],
        source_sample_rate: f32,
        output_sample_rate: f32,
        pitch_ratio: f32,
        start_offset_samples: f32,
        looped: bool,
    ) -> Self {
        let increment = playback_increment(source_sample_rate, output_sample_rate, pitch_ratio);
        Self {
            samples,
            cursor: PlaybackCursor::forward(samples.len(), start_offset_samples, increment, looped),
        }
    }

    pub fn region(
        samples: &'a [f32],
        region: PlaybackRegion,
        increment: f32,
        direction: PlaybackDirection,
        looped: bool,
    ) -> Self {
        Self {
            samples,
            cursor: PlaybackCursor::new(region, 0.0, increment, direction, looped),
        }
    }

    pub const fn is_finished(self) -> bool {
        self.cursor.is_finished()
    }

    pub fn next_sample(&mut self) -> f32 {
        self.cursor
            .next_position()
            .map(|position| interpolation::linear(self.samples, position))
            .unwrap_or(0.0)
    }
}

pub fn playback_increment(
    source_sample_rate: f32,
    output_sample_rate: f32,
    pitch_ratio: f32,
) -> f32 {
    if source_sample_rate > 0.0 && output_sample_rate > 0.0 {
        source_sample_rate / output_sample_rate * finite_non_negative(pitch_ratio)
    } else {
        1.0
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_cursor_yields_forward_positions() {
        let mut cursor = PlaybackCursor::forward(3, 0.0, 1.0, false);

        assert_eq!(cursor.next_position(), Some(0.0));
        assert_eq!(cursor.next_position(), Some(1.0));
        assert_eq!(cursor.next_position(), Some(2.0));
        assert_eq!(cursor.next_position(), None);
    }

    #[test]
    fn playback_cursor_loops_inside_region() {
        let mut cursor = PlaybackCursor::new(
            PlaybackRegion::new(10.0, 13.0),
            0.0,
            1.0,
            PlaybackDirection::Forward,
            true,
        );

        assert_eq!(cursor.next_position(), Some(10.0));
        assert_eq!(cursor.next_position(), Some(11.0));
        assert_eq!(cursor.next_position(), Some(12.0));
        assert_eq!(cursor.next_position(), Some(10.0));
    }

    #[test]
    fn sample_playback_supports_reverse_regions() {
        let samples = [1.0, 2.0, 3.0, 4.0];
        let mut playback = SamplePlayback::region(
            &samples,
            PlaybackRegion::new(1.0, 4.0),
            1.0,
            PlaybackDirection::Reverse,
            false,
        );

        assert_eq!(playback.next_sample(), 4.0);
        assert_eq!(playback.next_sample(), 3.0);
        assert_eq!(playback.next_sample(), 2.0);
        assert_eq!(playback.next_sample(), 0.0);
    }
}
