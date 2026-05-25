use crate::{MarkerKind, SliceMarker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerReconcilePolicy {
    MergeUserMarkers,
    ReplaceUserMarkers,
    CancelIfUserMarkers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerReconcileOutcome {
    Applied(Vec<SliceMarker>),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceRegion {
    pub index: usize,
    pub start_sample: usize,
    pub end_sample: usize,
}

impl SliceRegion {
    pub const fn duration_samples(self) -> usize {
        self.end_sample.saturating_sub(self.start_sample)
    }
}

pub fn normalize_markers(
    markers: impl IntoIterator<Item = SliceMarker>,
    min_gap_samples: usize,
    source_len: usize,
) -> Vec<SliceMarker> {
    if source_len == 0 {
        return Vec::new();
    }

    let mut markers = markers
        .into_iter()
        .map(|mut marker| {
            marker.position_samples = marker.position_samples.min(source_len - 1);
            marker
        })
        .collect::<Vec<_>>();
    markers.sort_by(|left, right| {
        left.position_samples
            .cmp(&right.position_samples)
            .then_with(|| marker_priority(left.kind).cmp(&marker_priority(right.kind)))
    });

    let min_gap_samples = min_gap_samples.max(1);
    let mut normalized: Vec<SliceMarker> = Vec::new();
    for marker in markers {
        let Some(last) = normalized.last_mut() else {
            normalized.push(marker);
            continue;
        };
        if marker
            .position_samples
            .saturating_sub(last.position_samples)
            < min_gap_samples
        {
            if marker.kind == MarkerKind::User && last.kind == MarkerKind::Auto {
                *last = marker;
            }
        } else {
            normalized.push(marker);
        }
    }

    if normalized.first().map(|marker| marker.position_samples) != Some(0) {
        normalized.insert(
            0,
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
        );
    }

    normalized
}

pub fn reconcile_markers(
    detected_auto_markers: impl IntoIterator<Item = SliceMarker>,
    existing_markers: &[SliceMarker],
    policy: MarkerReconcilePolicy,
    min_gap_samples: usize,
    source_len: usize,
) -> MarkerReconcileOutcome {
    let has_user_markers = existing_markers
        .iter()
        .any(|marker| marker.kind == MarkerKind::User);
    if policy == MarkerReconcilePolicy::CancelIfUserMarkers && has_user_markers {
        return MarkerReconcileOutcome::Cancelled;
    }

    let mut markers = detected_auto_markers
        .into_iter()
        .map(|mut marker| {
            marker.kind = MarkerKind::Auto;
            marker
        })
        .collect::<Vec<_>>();
    if policy == MarkerReconcilePolicy::MergeUserMarkers {
        markers.extend(
            existing_markers
                .iter()
                .copied()
                .filter(|marker| marker.kind == MarkerKind::User),
        );
    }

    MarkerReconcileOutcome::Applied(normalize_markers(markers, min_gap_samples, source_len))
}

pub fn select_strongest_markers(
    markers: impl IntoIterator<Item = SliceMarker>,
    audio: &[f32],
    max_markers: usize,
    min_gap_samples: usize,
) -> Vec<SliceMarker> {
    if audio.is_empty() || max_markers == 0 {
        return Vec::new();
    }

    let source_len = audio.len();
    let mut candidates = markers
        .into_iter()
        .map(|mut marker| {
            marker.position_samples = marker.position_samples.min(source_len - 1);
            marker
        })
        .collect::<Vec<_>>();
    candidates.push(SliceMarker {
        position_samples: 0,
        kind: MarkerKind::Auto,
    });
    candidates.sort_by(|left, right| {
        left.position_samples
            .cmp(&right.position_samples)
            .then_with(|| marker_priority(left.kind).cmp(&marker_priority(right.kind)))
    });
    let mut deduped: Vec<SliceMarker> = Vec::new();
    for candidate in candidates {
        if let Some(last) = deduped.last_mut()
            && last.position_samples == candidate.position_samples
        {
            if candidate.kind == MarkerKind::User {
                last.kind = MarkerKind::User;
            }
            continue;
        }
        deduped.push(candidate);
    }

    let mut ranked = deduped
        .into_iter()
        .map(|marker| RankedMarker {
            marker,
            score: marker_salience(audio, marker.position_samples, min_gap_samples),
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        required_marker_priority(right.marker)
            .cmp(&required_marker_priority(left.marker))
            .then_with(|| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.marker
                    .position_samples
                    .cmp(&right.marker.position_samples)
            })
    });

    let min_gap_samples = min_gap_samples.max(1);
    let mut selected = Vec::new();
    for ranked_marker in ranked {
        if selected.len() >= max_markers {
            break;
        }
        if selected.iter().all(|marker: &SliceMarker| {
            marker
                .position_samples
                .abs_diff(ranked_marker.marker.position_samples)
                >= min_gap_samples
                || ranked_marker.marker.position_samples == 0
        }) {
            selected.push(ranked_marker.marker);
        }
    }

    normalize_markers(selected, min_gap_samples, source_len)
}

pub fn slice_regions_from_markers(markers: &[SliceMarker], source_len: usize) -> Vec<SliceRegion> {
    if source_len == 0 {
        return Vec::new();
    }

    let mut positions = normalize_markers(markers.iter().copied(), 1, source_len)
        .into_iter()
        .map(|marker| marker.position_samples.min(source_len))
        .collect::<Vec<_>>();
    positions.push(source_len);
    positions.sort_unstable();
    positions.dedup();

    positions
        .windows(2)
        .enumerate()
        .filter_map(|(index, window)| {
            let start_sample = window[0].min(source_len);
            let end_sample = window[1].clamp(start_sample, source_len);
            (end_sample > start_sample).then_some(SliceRegion {
                index,
                start_sample,
                end_sample,
            })
        })
        .collect()
}

pub fn slice_region_at_sample(
    markers: &[SliceMarker],
    source_len: usize,
    position_samples: usize,
) -> Option<SliceRegion> {
    let position_samples = position_samples.min(source_len.saturating_sub(1));
    slice_regions_from_markers(markers, source_len)
        .into_iter()
        .find(|region| {
            position_samples >= region.start_sample && position_samples < region.end_sample
        })
}

pub fn snap_position_to_nearest_zero_crossing(
    audio: &[f32],
    position_samples: usize,
    search_radius_samples: usize,
) -> usize {
    if audio.is_empty() {
        return 0;
    }

    let center = position_samples.min(audio.len() - 1);
    if is_zero_crossing(audio, center) {
        return center;
    }

    for offset in 1..=search_radius_samples {
        if let Some(left) = center.checked_sub(offset)
            && is_zero_crossing(audio, left)
        {
            return left;
        }
        let right = center.saturating_add(offset);
        if right < audio.len() && is_zero_crossing(audio, right) {
            return right;
        }
    }

    center
}

pub fn snap_markers_to_zero_crossings(
    markers: impl IntoIterator<Item = SliceMarker>,
    audio: &[f32],
    search_radius_samples: usize,
) -> Vec<SliceMarker> {
    markers
        .into_iter()
        .map(|mut marker| {
            marker.position_samples = snap_position_to_nearest_zero_crossing(
                audio,
                marker.position_samples,
                search_radius_samples,
            );
            marker
        })
        .collect()
}

fn is_zero_crossing(audio: &[f32], index: usize) -> bool {
    let sample = finite_sample(audio[index]);
    if sample.abs() <= f32::EPSILON {
        return true;
    }
    let Some(previous) = index
        .checked_sub(1)
        .and_then(|previous| audio.get(previous))
    else {
        return false;
    };
    let previous = finite_sample(*previous);
    (previous <= 0.0 && sample > 0.0) || (previous >= 0.0 && sample < 0.0)
}

fn finite_sample(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

#[derive(Debug, Clone, Copy)]
struct RankedMarker {
    marker: SliceMarker,
    score: f32,
}

fn marker_salience(audio: &[f32], position_samples: usize, min_gap_samples: usize) -> f32 {
    let window = min_gap_samples.clamp(32, 4_096);
    let position = position_samples.min(audio.len().saturating_sub(1));
    let before = &audio[position.saturating_sub(window)..position];
    let after = &audio[position..(position + window).min(audio.len())];
    let before_rms = rms(before);
    let after_rms = rms(after);
    let after_peak = after
        .iter()
        .copied()
        .map(finite_sample)
        .map(f32::abs)
        .fold(0.0, f32::max);
    (after_rms - before_rms).max(0.0) * 2.0 + after_rms + after_peak
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares = samples
        .iter()
        .copied()
        .map(finite_sample)
        .map(|sample| sample * sample)
        .sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn required_marker_priority(marker: SliceMarker) -> u8 {
    if marker.position_samples == 0 {
        2
    } else if marker.kind == MarkerKind::User {
        1
    } else {
        0
    }
}

fn marker_priority(kind: MarkerKind) -> u8 {
    match kind {
        MarkerKind::User => 0,
        MarkerKind::Auto => 1,
    }
}
