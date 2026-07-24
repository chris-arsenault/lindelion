use lindelion_dsp_utils::math::{finite_clamp, ms_to_samples};
use lindelion_pitch_detect::{PitchContour, PitchFrame};

use crate::patch::DEFAULT_TUNING_REFERENCE_HZ;

pub const DEFAULT_PITCH_MAP_TOLERANCE_CENTS: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchMappedRegion {
    pub midi_note: u8,
    pub start_sample: usize,
    pub end_sample: usize,
    pub detected_f0_hz: f32,
    pub cents_deviation: f32,
    pub mean_confidence: f32,
}

impl PitchMappedRegion {
    pub fn duration_samples(self) -> usize {
        self.end_sample.saturating_sub(self.start_sample)
    }
}

pub(crate) fn detect_pitch_mapped_regions(
    contour: &PitchContour,
    source_len: usize,
    source_sample_rate: u32,
    tuning_reference_hz: f32,
    min_region_ms: f32,
) -> Vec<PitchMappedRegion> {
    if source_len == 0 || source_sample_rate == 0 {
        return Vec::new();
    }

    let reference_hz = finite_clamp(
        tuning_reference_hz,
        1.0,
        24_000.0,
        DEFAULT_TUNING_REFERENCE_HZ,
    );
    let min_region_samples = ms_to_samples(min_region_ms, source_sample_rate).max(1);
    let frame_span_samples = source_frame_span_samples(contour, source_sample_rate);
    let mut best_by_note = [None; 128];
    let mut current: Option<PitchRun> = None;

    for frame in &contour.frames {
        let matched = matched_pitch_frame(frame, reference_hz);
        match (current.as_mut(), matched) {
            (Some(run), Some(pitch)) if run.midi_note == pitch.midi_note => {
                run.push(frame, pitch);
            }
            (Some(_), matched) => {
                finish_run(
                    current.take().unwrap(),
                    &mut best_by_note,
                    source_len,
                    frame_span_samples,
                    min_region_samples,
                );
                current = matched.map(|pitch| PitchRun::new(frame, pitch));
            }
            (None, Some(pitch)) => current = Some(PitchRun::new(frame, pitch)),
            (None, None) => {}
        }
    }
    if let Some(run) = current {
        finish_run(
            run,
            &mut best_by_note,
            source_len,
            frame_span_samples,
            min_region_samples,
        );
    }

    best_by_note.into_iter().flatten().collect()
}

fn matched_pitch_frame(frame: &PitchFrame, reference_hz: f32) -> Option<MatchedPitch> {
    if !frame.voiced {
        return None;
    }
    let f0_hz = frame.f0_hz.filter(|f0| f0.is_finite() && *f0 > 0.0)?;
    let detected_note = 69.0 + 12.0 * (f0_hz / reference_hz).log2();
    if !detected_note.is_finite() {
        return None;
    }
    let nearest_note = detected_note.round();
    if !(0.0..=127.0).contains(&nearest_note) {
        return None;
    }
    let cents_deviation = (detected_note - nearest_note) * 100.0;
    (cents_deviation.abs() <= DEFAULT_PITCH_MAP_TOLERANCE_CENTS).then_some(MatchedPitch {
        midi_note: nearest_note as u8,
        detected_note,
        f0_hz,
    })
}

fn finish_run(
    run: PitchRun,
    best_by_note: &mut [Option<PitchMappedRegion>; 128],
    source_len: usize,
    frame_span_samples: usize,
    min_region_samples: usize,
) {
    let region = run.into_region(source_len, frame_span_samples);
    if region.duration_samples() < min_region_samples {
        return;
    }
    let slot = &mut best_by_note[region.midi_note as usize];
    if slot.is_none_or(|current| region_is_better(region, current)) {
        *slot = Some(region);
    }
}

fn region_is_better(candidate: PitchMappedRegion, current: PitchMappedRegion) -> bool {
    candidate.duration_samples() > current.duration_samples()
        || (candidate.duration_samples() == current.duration_samples()
            && (candidate.mean_confidence > current.mean_confidence
                || (candidate.mean_confidence == current.mean_confidence
                    && candidate.cents_deviation.abs() < current.cents_deviation.abs())))
}

fn source_frame_span_samples(contour: &PitchContour, source_sample_rate: u32) -> usize {
    if contour.analysis_sample_rate > 0 && contour.hop_size > 0 {
        return ((contour.hop_size as f64 * source_sample_rate as f64
            / contour.analysis_sample_rate as f64)
            .round() as usize)
            .max(1);
    }
    contour
        .frames
        .windows(2)
        .find_map(|frames| {
            frames[1]
                .source_sample_position
                .checked_sub(frames[0].source_sample_position)
                .filter(|span| *span > 0)
        })
        .unwrap_or(1)
}

#[derive(Debug, Clone, Copy)]
struct MatchedPitch {
    midi_note: u8,
    detected_note: f32,
    f0_hz: f32,
}

#[derive(Debug, Clone, Copy)]
struct PitchRun {
    midi_note: u8,
    first_center_sample: usize,
    last_center_sample: usize,
    frame_count: usize,
    detected_note_sum: f32,
    f0_sum: f32,
    confidence_sum: f32,
}

impl PitchRun {
    fn new(frame: &PitchFrame, pitch: MatchedPitch) -> Self {
        Self {
            midi_note: pitch.midi_note,
            first_center_sample: frame.source_sample_position,
            last_center_sample: frame.source_sample_position,
            frame_count: 1,
            detected_note_sum: pitch.detected_note,
            f0_sum: pitch.f0_hz,
            confidence_sum: frame.confidence.clamp(0.0, 1.0),
        }
    }

    fn push(&mut self, frame: &PitchFrame, pitch: MatchedPitch) {
        self.last_center_sample = frame.source_sample_position;
        self.frame_count += 1;
        self.detected_note_sum += pitch.detected_note;
        self.f0_sum += pitch.f0_hz;
        self.confidence_sum += frame.confidence.clamp(0.0, 1.0);
    }

    fn into_region(self, source_len: usize, frame_span_samples: usize) -> PitchMappedRegion {
        let edge = frame_span_samples / 2;
        let mean_note = self.detected_note_sum / self.frame_count as f32;
        PitchMappedRegion {
            midi_note: self.midi_note,
            start_sample: self
                .first_center_sample
                .saturating_sub(edge)
                .min(source_len),
            end_sample: self
                .last_center_sample
                .saturating_add(frame_span_samples.saturating_sub(edge))
                .min(source_len),
            detected_f0_hz: self.f0_sum / self.frame_count as f32,
            cents_deviation: (mean_note - self.midi_note as f32) * 100.0,
            mean_confidence: self.confidence_sum / self.frame_count as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_the_whole_contour_and_maps_each_absolute_pitch() {
        let contour = contour(vec![
            frame(0, 0, midi_hz(61.0), true),
            frame(1, 256, midi_hz(61.0), true),
            frame(2, 512, 0.0, false),
            frame(3, 768, midi_hz(60.0), true),
            frame(4, 1_024, midi_hz(60.0), true),
        ]);

        let regions = detect_pitch_mapped_regions(&contour, 1_280, 48_000, 440.0, 5.0);

        assert_eq!(
            regions
                .iter()
                .map(|region| region.midi_note)
                .collect::<Vec<_>>(),
            vec![60, 61]
        );
        assert_eq!(regions[0].start_sample, 640);
        assert_eq!(regions[0].end_sample, 1_152);
        assert_eq!(regions[1].start_sample, 0);
        assert_eq!(regions[1].end_sample, 384);
    }

    #[test]
    fn rejects_out_of_tolerance_and_missing_pitches() {
        let contour = contour(vec![
            frame(0, 0, midi_hz(60.26), true),
            frame(1, 256, midi_hz(60.26), true),
            frame(2, 512, midi_hz(62.0), false),
            frame(3, 768, midi_hz(62.0), false),
        ]);

        let regions = detect_pitch_mapped_regions(&contour, 1_024, 48_000, 440.0, 1.0);

        assert!(regions.is_empty());
    }

    #[test]
    fn keeps_the_longest_stable_run_for_a_repeated_pitch() {
        let contour = contour(vec![
            frame(0, 0, midi_hz(60.0), true),
            frame(1, 256, 0.0, false),
            frame(2, 512, midi_hz(60.1), true),
            frame(3, 768, midi_hz(60.1), true),
            frame(4, 1_024, midi_hz(60.1), true),
        ]);

        let regions = detect_pitch_mapped_regions(&contour, 1_280, 48_000, 440.0, 1.0);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_sample, 384);
        assert_eq!(regions[0].end_sample, 1_152);
        assert!((regions[0].cents_deviation - 10.0).abs() < 0.01);
    }

    fn contour(frames: Vec<PitchFrame>) -> PitchContour {
        PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 48_000,
            hop_size: 256,
            frames,
        }
    }

    fn frame(
        frame_index: usize,
        source_sample_position: usize,
        f0_hz: f32,
        voiced: bool,
    ) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz: voiced.then_some(f0_hz),
            raw_f0_hz: f0_hz,
            confidence: 0.95,
            voiced,
            rms: 0.1,
        }
    }

    fn midi_hz(note: f32) -> f32 {
        440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
    }
}
