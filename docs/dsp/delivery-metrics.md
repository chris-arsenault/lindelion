# Lúmedir Delivery Metrics

Lúmedir's four delivery estimators and the aggregator that assembles them. These run **off the audio
thread** on the `DeliveryWorker` (ADR-0001, ADR-0048); the audio thread only feeds a mono mix into a
lock-free ring. They are analysis/measurement modules (not filters), so this doc uses the
algorithm/parameters/realtime-contract sections plus a fixture-validation table in place of
magnitude/phase plots.

Source: `plugins/lumedir/src/{speaking_rate,pitch_dynamism,pause_structure,clarity,delivery}.rs`.
Each estimator consumes the per-block audio and/or the shared `SignalSnapshot`
(`lindelion-speech-signals`) and exposes pure read accessors.

## 1. Speaking rate (`speaking_rate.rs`)

**Role.** Acoustic speaking rate as the **syllable-nuclei** rate over a sliding window, plus a WPM
derivation. No ASR.

**Algorithm.** One intensity sample (dB) is stored every `HOP = 128` audio samples (≈2.7 ms at
48 kHz) into a window-bounded `contour_db` deque. Syllable nuclei are picked by **prominence**: a
local maximum counts only if it is within `silence_threshold_db` of the window's intensity maximum
(`floor = global_max − silence_threshold_db`) and is separated from the previous nucleus by an
intensity dip of at least `min_dip_db` and by at least `min_nucleus_interval_s`. The rate is
`nuclei / window_s`; `words_per_minute(factor) = nuclei_per_minute ÷ factor`.

| Parameter | Units | Default | Source |
| ---- | ---- | ---- | ---- |
| `silence_threshold_db` | dB below window max | 25.0 | `SpeakingRateConfig` |
| `min_dip_db` | dB | 2.5 | `SpeakingRateConfig` |
| `min_nucleus_interval_s` | seconds | 0.09 | `SpeakingRateConfig` |
| `window_s` | seconds | 8.0 | `SpeakingRateConfig` |
| `DEFAULT_SYLLABLES_PER_WORD` | syllables/word | 1.5 | const |

## 2. Pitch dynamism (`pitch_dynamism.rs`)

**Role.** Spread of voiced pitch over a window, in semitones (flat ↔ animated).

**Algorithm.** Each voiced, pitched frame's f0 is converted to semitones (`12·log2(f0)`) and pushed
into a `semitones` deque bounded to `window_voiced_frames`; unvoiced/`None` frames are ignored.
`semitone_std()` is the standard deviation of the windowed semitone values (0 with fewer than two
samples).

| Parameter | Units | Default | Source |
| ---- | ---- | ---- | ---- |
| `window_voiced_frames` | voiced frames | 2000 | `PitchDynamismConfig` |

## 3. Pause structure (`pause_structure.rs`)

**Role.** Silence runs → pause fraction and pause count.

**Algorithm.** Per frame, the frame's energy in dB updates a `running_max_db`; a frame is `silent`
when `frame_db < running_max_db − silence_threshold_db`. A run of silent frames is a **pause** when
it lasts at least `min_pause_s`. `pause_fraction()` is the silent fraction of the window;
`pause_count()` counts qualifying runs (including a trailing run).

| Parameter | Units | Default | Source |
| ---- | ---- | ---- | ---- |
| `silence_threshold_db` | dB below running max | 35.0 | `PauseStructureConfig` |
| `min_pause_s` | seconds | 0.2 | `PauseStructureConfig` |

## 4. Clarity (`clarity.rs`)

**Role.** How clearly articulated the speech is, as a `0..1` composite.

**Algorithm.** Two cues from `SignalAnalyzer`'s per-frame outputs: the **voicing ratio** (voiced
frames ÷ phonated frames) and **onset sharpness** (`onset_flux_high` soft-saturated as
`x / (x + onset_reference)`). Silence contributes to neither. `clarity()` blends them as
`voicing_weight·voicing_ratio + onset_weight·onset_norm`, clamped to `0..1`. Voicing ratio is the
noise-robust cue and carries the larger weight (onset flux is partly inflated by broadband noise).

| Parameter | Units | Default | Source |
| ---- | ---- | ---- | ---- |
| `voicing_weight` / `onset_weight` | 0..1 blend | voicing-dominant | `ClarityConfig` |
| `onset_reference` | flux units | tuned (see source) | `ClarityConfig` |

## 5. Aggregation (`delivery.rs`)

`DeliveryAggregator::update(audio, &SignalSnapshot)` routes each step: rate + pauses get the audio,
dynamism gets the voiced f0 (`voicing_state ≥ 1.5 ? Some(pitch_hz) : None`), clarity gets
`voicing_state` + `onset_flux_high`. `snapshot_with_factor(factor)` assembles the `DeliverySnapshot`;
the worker passes the live syllables-per-word factor each publish so WPM tracks editor edits.

## 6. Realtime contract

- **No estimator method is RT-callable.** They run on the `DeliveryWorker` thread. The audio thread
  calls only `DeliveryWorker::push`, which writes the mono mix into a lock-free `SampleRing`
  (`lindelion_dsp_utils::handoff`) — allocation-free and non-blocking (ADR-0001). Pinned by
  `lumedir::plugin::tests::process_is_bit_exact_stereo_passthrough_without_allocating` and
  `lumedir::vst3_entry::processor::tests::loads_and_runs_bit_exact_passthrough_through_the_vst3_boundary`.
- **The worker thread allocates, but boundedly.** All estimators hold capacity-bounded deques
  (`pop_front` at the window cap); `semitone_std` builds one window-sized scratch per snapshot. No
  unbounded growth over a soak — pinned by
  `lumedir::delivery::tests::aggregator_soak_does_not_grow_allocations` (warmed past the 2000-frame
  dynamism window, asserts no batch-over-batch allocation growth, via the counting allocator) and
  `worker_soak_is_stable_off_thread` (`plugins/lumedir/tests/integration.rs`).
- All snapshot fields are finite and in range (`pause_fraction`, `clarity` ∈ `0..1`).

## 7. Validation against the speech fixtures

Estimators are validated against the public-domain LibriVox fixtures in `testdata/audio/FIXTURES.md`
(targets: `syl/s`, `pstd` in semitones, `pause` fraction). They must rank the fixtures correctly and
land near the targets within tolerance.

| Dimension | Fixture targets | Test |
| ---- | ---- | ---- |
| Speaking rate | slow 2.8 vs fast 3.8 syl/s | `plugins/lumedir/tests/rate_fixtures.rs::estimator_ranks_and_lands_near_fixture_targets` |
| Pitch dynamism | flat 1.1 vs animated 7.4 st | `plugins/lumedir/tests/dynamism_fixtures.rs::dynamism_ranks_flat_below_animated_and_lands_near_targets` |
| Pause structure | pauses fixture (fraction/count/length) | `plugins/lumedir/tests/pause_fixtures.rs::pause_metrics_match_the_pauses_fixture` |
| Clarity | clean ranks above noisy | `plugins/lumedir/tests/delivery_fixtures.rs::clarity_ranks_clean_above_noisy_and_snapshot_is_complete` |

In-memory unit tests cover the same behaviors deterministically: `lumedir::speaking_rate::tests`
(`tracks_a_known_syllable_rate`, `wpm_derives_from_the_rate_and_factor`), `lumedir::pitch_dynamism::tests`
(`alternating_pitches_read_the_expected_std`, `animated_reads_above_flat`),
`lumedir::pause_structure::tests` (`measures_a_known_pause_fraction_and_run`), and
`lumedir::clarity::tests` (`clear_speech_reads_high`, `silence_only_reads_zero`). The fixture renders
are gated to `make test-integration`; the unit tests run in `make ci`.

Target bands a delivery is *scored* against (a product concern, not an estimator property) live in
[the plugin spec](../plugins/lumedir.md#5-configuration-scoring-and-state).
