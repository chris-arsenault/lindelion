# Resample Pro Pitch Shifting

Resample Pro is Lindelion's clean-room stretch-plus-resample pitch-shift family for fixed-duration sample rendering. Linnod exposes it through the legacy `ResampleStretch` enum value for patch and UI compatibility.

The engine is designed for source/patch preparation. It renders complete shifted slice buffers, then runtime playback reads those buffers with the normal sample cursor, declick, envelope, filter, gain, pan, and output stages.

## Algorithm

For a pitch ratio `p`, Resample Pro renders a duration-preserving pitch shift in two explicit stages:

1. Time-scale the source by `p` without changing pitch.
2. Read the time-scaled signal with a bandlimited resampler at read ratio `p`.

For `p = 2.0`, the source is stretched to twice its duration, then read twice as fast. The result returns to the original duration and sounds one octave higher. For `p = 0.5`, the source is compressed to half duration, then read half as fast, producing an octave-down fixed-duration render.

The time-scale stage is a phase-aware STFT renderer:

- `f64` FFT analysis, phase, instantaneous-frequency, and time-position values;
- fixed analysis and synthesis hops;
- fractional source-analysis position for non-unity stretch;
- per-bin instantaneous-frequency estimation from unwrapped phase deltas;
- output phase accumulation across frames;
- peak phase-locking for vertical coherence;
- transient-frame hints from slice/onset analysis;
- overlap-add reconstruction with window normalization.

The resampling stage is shared through `lindelion-dsp-utils::resampling`:

- `f64` read cursor;
- windowed-sinc interpolation;
- a pitch-shift quality profile with stronger tap count and transition guard;
- pitch-up cutoff based on legal shifted bandwidth;
- alias tests covering high-frequency tones, sine-sweep segments, and bright breath/noise fixtures.

## Source Cache

`PitchShiftSourceCache` contains `ResampleProCache`, a source-level STFT cache built during analysis. It stores:

- sample rate, FFT size, analysis hop, and synthesis hop;
- analysis window and overlap-add normalization;
- per-frame magnitudes, phases, instantaneous frequencies, and peak ownership;
- transient frame and sample hints.

FFT planning and spectral allocation happen during analysis or setup-time rendering, not on Linnod's note playback path.

## Formants

Resample Pro keeps Linnod's existing `PitchShiftRatios` semantics:

- `formant_ratio = None` preserves the source spectral envelope independently of pitch;
- `formant_ratio = Some(pitch_ratio)` moves the spectral envelope with the pitch shift.

Because final pitch is created by resampling, spectral-envelope correction is applied in the stretched spectrum before the resampling stage.

## Linnod Boundary

Linnod owns product policy for prepared variants. During source or patch preparation it resolves the Resample Stretch slice variants required by the current patch, renders non-identity variants with guard context, trims them to exact slice duration, and stores them in a plugin-local prepared-buffer cache.

The prepared-buffer key includes:

- source cache key;
- slice index;
- source start and end samples;
- pitch and formant ratios;
- playback direction policy;
- Resample Pro render-config version.

MIDI note playback never invokes `ResampleProRenderState`. It either reads the prepared shifted slice buffer or returns silence for a requested shifted Resample Stretch variant that was not prepared. It does not fall back to direct unshifted playback or to another pitch algorithm while claiming the requested shift.

Pad mode prepares the exact pitch variants implied by the pad map and slice settings. Chromatic mode currently prepares the selected root-note variant; broader chromatic prepared-variant coverage is tracked in the Linnod backlog.

## Quality Contract

The Resample Pro path is covered by objective tests for:

- unity STFT reconstruction;
- 1-cent sine and harmonic-stack residuals;
- source-pitch leakage suppression;
- formant preserve and formant track behavior;
- pitch-up alias rejection against legal shifted bandwidth;
- bright breath/noise alias rejection;
- downshift low-frequency phase continuity;
- synthetic transient pre-echo;
- guarded slice-edge rendering;
- real wind and sax fixture cleanliness before Linnod output processing;
- Linnod prepared-buffer playback with no note-path rendering.

Identity playback may read the source directly in Linnod, but every non-identity Resample Stretch request uses the same real stretch-plus-resample renderer during preparation. The implementation does not use soft saturation, direct-path crossfade, or an unshifted residual layer to hide pitch-shift artifacts.
