# Phrase analysis

Pipeline orchestrator that combines pitch detection, onset detection, and note segmentation into a single `PhraseAnalysisResult`.

## 1. Purpose

Takes a recorded monophonic phrase and produces:

- The full `PitchContour` from pitch detection.
- A `Vec<SliceMarker>` of onset positions.
- A `Vec<SegmentedNote>` of note-bounded regions with median voiced pitch and RMS statistics.
- A `Vec<DetectedNote>` derived from the segmented notes for downstream MIDI emission.

Used by Glirdir's analysis worker after a sing-to-MIDI capture completes. The orchestrator owns the pipeline order; downstream consumers (MIDI clip builder, quantizer) act on the structured result.

## 2. Theory

**Pipeline order.**

```
audio  ─┬──► pitch_detector.detect_with_config ──► PitchContour
        │                                              │
        └──► OnsetDetectionInput::new(audio, sr)       │
                            │                          │
                            └─ with_pitch_contour ─────┤
                                                       ▼
                                onset_detector.detect ──► Vec<SliceMarker>
                                                              │
                                              NoteSegmenter ──┤
                                                              ▼
                                                     Vec<SegmentedNote>
                                                              │
                                                     detected_notes(...)
                                                              ▼
                                                     Vec<DetectedNote>
```

Pitch detection runs first. Its contour is fed to the onset detector (via `with_pitch_contour`) so the `HybridOnsetDetector` can combine spectral-flux onsets with pitch-stability onsets. The combined marker list bounds the note segmenter, which walks consecutive marker pairs and emits a `SegmentedNote` per window of at least `min_note_ms` containing a stable median voiced pitch.

**Note segmentation rules** (in order):

1. **Window length floor.** Windows shorter than `min_note_ms` are skipped.
2. **Pitch fill.** Median voiced pitch in the window. If no voiced frames, inherit the previous note's pitch (flagged `inherited_pitch = true`).
3. **Inherited-pitch RMS gate.** Inherited-pitch notes are rejected if either peak RMS in the window OR the minimum-chunk RMS falls below `min_inherited_pitch_rms`. Prevents the segmenter from creating phantom notes through silence.
4. **Same-pitch merge.** Adjacent notes whose pitches differ by less than `same_pitch_merge_cents` AND lack an articulation gap are merged. The articulation gap test scans `articulation_search_ms` of audio before the next note's onset and looks for an RMS dip below `articulation_gap_ratio` of the surrounding peaks.

**Stability.** Pure pipeline. The pitch detector and onset detector each handle their own validation. Note segmentation is deterministic given the same inputs.

**Valid parameter range.** Per `NoteSegmentationConfig::sanitized()`:

- `min_note_ms ∈ [0, 5000]`
- `min_inherited_pitch_rms ∈ [0, 1]`
- `same_pitch_merge_cents ∈ [0, 1200]`
- `articulation_search_ms ∈ [0, 1000]`
- `articulation_gap_ratio ∈ [0, 1]`
- `rms_chunk_samples ≥ 1`

## 3. Algorithm

Top-level entry:

```rust
pub fn analyze(
    &self,
    audio: &[f32],
    sample_rate: u32,
    config: PhraseAnalysisConfig,
) -> Result<PhraseAnalysisResult, PhraseAnalysisError> {
    if audio.is_empty() { return Err(PhraseAnalysisError::EmptyAudio); }
    let config = config.sanitized();
    let pitch_contour = self.pitch_detector
        .detect_with_config(audio, sample_rate, config.pitch_detection)?;
    Ok(self.analyze_with_pitch_contour(audio, sample_rate, config, pitch_contour))
}
```

Glirdir uses `analyze_with_pitch_contour` directly when it has already run pitch detection on a worker thread, avoiding a redundant SwiftF0 invocation.

Note segmentation per window:

```rust
let pitch = median_voiced_pitch(frames).or(previous_pitch);
let (peak_rms, mean_rms) = note_rms(audio, start, end, frames);
if inherited_pitch
    && (peak_rms < self.config.min_inherited_pitch_rms
        || minimum_chunk_rms(audio_region, self.config.rms_chunk_samples)
            < self.config.min_inherited_pitch_rms)
{ continue; }
notes.push(SegmentedNote {
    note: DetectedNote { start_sample, end_sample, pitch_hz, peak_rms, mean_rms },
    inherited_pitch,
});
```

## 4. Parameters

`PhraseAnalysisConfig`:

| Section | Type | Notes |
| ---- | ---- | ---- |
| `pitch_detection` | `PitchDetectionConfig` | Forwarded to the pitch detector |
| `onset_detection` | `DetectionConfig` | Forwarded to the onset detector |
| `note_segmentation` | `NoteSegmentationConfig` | Configures the segmenter directly |

`NoteSegmentationConfig`:

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| `min_note_ms` | `f32` | ms | `[0, 5000]` | 80 | Floor on per-note duration |
| `min_inherited_pitch_rms` | `f32` | linear | `[0, 1]` | 0.04 | RMS threshold for pitch-inherited notes |
| `same_pitch_merge_cents` | `f32` | cents | `[0, 1200]` | 35 | Merge threshold for adjacent same-pitch notes |
| `articulation_search_ms` | `f32` | ms | `[0, 1000]` | 45 | Window scanned for articulation gaps |
| `articulation_gap_ratio` | `f32` | linear | `[0, 1]` | 0.35 | RMS-dip threshold relative to surrounding peaks |
| `rms_chunk_samples` | `usize` | samples | `≥ 1` | 256 | Chunk size for minimum-chunk RMS |

Presets:

| Preset | min_note_ms | same_pitch_merge_cents | articulation_gap_ratio |
| ---- | ---- | ---- | ---- |
| `default()` | 80 | 35 | 0.35 |
| `relaxed()` | 60 | 50 | 0.25 |
| `aggressive()` | 45 | 20 | 0.5 |

## 5. Response plots

Visualizing a `PhraseAnalysisResult` requires a multi-panel plot (waveform with onset markers + pitch contour + segmented note rectangles) that does not currently fit any of the single-CSV plot scripts. Documented as a backlog item in `docs/backlog.md`.

In the meantime, the underlying [pitch detector](pitch-detect.md) and [onset detector](onset-detect.md) docs each provide a single-axis visualization of their own behavior on synthetic input. A phrase-analysis result is the composition of those two outputs plus note segmentation, which is best inspected via the structured `PhraseAnalysisResult` rather than a chart.

## 6. Realtime contract

- **Allocation.** `PhraseAnalyzer::analyze` allocates the pitch contour, marker vector, and segmentation result. NOT realtime-safe. Glirdir runs this on its analysis worker.
- **Denormals.** Inherited from the pitch and onset detectors; the segmenter operates on already-sanitized values.
- **Reset.** Stateless — each call analyzes independently. The pitch and onset detectors hold their own state.
- **Thread safety.** `PhraseAnalyzer` is `Send + Sync` when its detector parameters are. Glirdir holds it in the worker.
- **Bounded work.** O(audio length) for pitch detection (model + resampling) plus O(audio length) for onset detection (STFT) plus O(markers) for segmentation. The full pipeline is bounded by audio length and is suitable for offline analysis of phrase-sized captures (seconds).
- **Finite output.** All RMS and pitch values are validated before being placed in `SegmentedNote` / `DetectedNote`. Non-finite values are filtered or fall back to defaults.

## 7. Test coverage

- `lindelion_phrase_analysis::tests` in `src/tests.rs` — covers segmentation rules against synthetic pitch contours and marker fixtures.

## 8. Usage example

Full pipeline from raw audio:

```rust
use lindelion_phrase_analysis::{PhraseAnalysisConfig, PhraseAnalyzer};

let analyzer = PhraseAnalyzer::default();
let result = analyzer.analyze(&audio, 48_000, PhraseAnalysisConfig::default())?;

for note in &result.detected_notes {
    println!(
        "{:.3}-{:.3} s @ {:.1} Hz (peak rms {:.3})",
        note.start_sample as f32 / 48_000.0,
        note.end_sample as f32 / 48_000.0,
        note.pitch_hz,
        note.peak_rms,
    );
}
```

Skipping pitch re-analysis when the worker has already computed a contour:

```rust
let pitch_contour = swiftf0.detect(&audio, 48_000)?;
let result = analyzer.analyze_with_pitch_contour(
    &audio,
    48_000,
    PhraseAnalysisConfig::default(),
    pitch_contour,
);
```

## 9. References

- Source: [`crates/lindelion-phrase-analysis/`](../../crates/lindelion-phrase-analysis/).
- Consumer: Glirdir analysis worker (`plugins/glirdir/src/analysis.rs`).
- Composed of: [`PitchDetector`](pitch-detect.md), [`OnsetDetector`](onset-detect.md), `NoteSegmenter`.
- ADR-0003: [Shared-core extraction policy](../adr/0003-shared-core-extraction.md).
- ADR-0008: [Capture-first voice-to-MIDI analysis](../adr/0008-capture-first-analysis.md).
