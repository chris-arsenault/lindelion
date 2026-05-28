# Ableton Complex Pro Reference

This note preserves a public-source analysis of Ableton Complex Pro and compares it with Lindelion's current pitch-shift engines. It is a clean-room reference for product and DSP work; it is not a bit-exact specification of Ableton or zplane internals.

## Executive finding

The best-supported public conclusion is:

**Ableton Complex Pro is Ableton’s UI/integration around zplane’s ELASTIQUE Pro-family time-stretch/pitch-shift technology, with Ableton-specific warp-marker, clip, automation, and rendering behavior around it.** The exact DSP implementation is proprietary, but the public zplane SDK/manual material reveals a lot about the likely control mapping: stretch factor, pitch factor, spectral-envelope/formant factor, envelope-order control, sync points, transient handling, and linked-channel analysis.

Ableton’s manual says **Complex Pro** is a higher-quality variation of Complex for polyphonic textures/full songs, with **Formants** and **Envelope** controls; Formants affect resonance-frequency preservation during pitch transposition, and Envelope defaults to **128**, lower for high-pitched material and higher for low-pitched material. Ableton also warns that Complex/Complex Pro are CPU-heavy. ([Ableton][1])

## What is confirmed by official sources

| Topic                             | Public evidence                                                                                                                                                                                                                                                                                                                                                   | Reverse-engineering implication                                                                                                                       |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Vendor / lineage**              | zplane says it has worked with Ableton since 2000 and was “most notably responsible” for the **Complex** warping algorithm. In the interview, Gerhard Behles says Ableton licensed **AUFTAKT** and **ELASTIQUE** for Live 5, and that Live’s previous algorithm was time-based/granular while the zplane Complex algorithm was **frequency-based**. ([zplane][2]) | Complex/Complex Pro should be modeled as an ELASTIQUE-family phase/frequency-domain TSM/pitch system, not just Live’s simpler grain modes.            |
| **Current-ish zplane dependency** | Ableton Live 11 release notes say a zplane library update could cause subtle sound changes in **Complex** and **Complex Pro** warp modes. ([Ableton][3]) zplane’s own blog explicitly describes Ableton Live’s Complex Pro formant shifting as powered by **ELASTIQUE**. ([zplane][4])                                                                            | Output can change with bundled zplane library versions; bit-exact cloning requires version matching, not just matching UI settings.                   |
| **Formants control**              | Ableton says Formants controls how much resonance frequencies are compensated during pitch transposition; at **100%**, original formants are preserved. It has no effect if transposition is unchanged. ([Ableton][1])                                                                                                                                            | This likely maps to zplane’s spectral-envelope shift factor. At 100%, envelope shift ≈ pitch factor; at 0%, envelope shift ≈ neutral/no compensation. |
| **Envelope control**              | Ableton says Envelope changes tonal quality; default is **128**; lower values suit high-pitched input, higher values suit low-pitched input. ([Ableton][1]) zplane’s SDK has `SetEnvelopeOrder`, default **128**, lower for high-pitched audio, higher for low-pitched audio, with values from roughly **8–512**.                                                 | Ableton’s Envelope knob is very likely a direct or scaled wrapper around zplane’s spectral-envelope estimation order.                                 |
| **Warp marker mapping**           | zplane’s SDK exposes sync-point APIs: input/output time anchors, output-position calculation, and a warp API with add/update/remove sync points.                                                                                                                                                                                                                  | Ableton warp markers plausibly become zplane sync points or a similar internal timing map.                                                            |
| **Transient handling**            | Ableton detects transients and uses transient markers as candidates for warp markers. ([Ableton][1]) zplane’s API includes sync-point handling with a transient-at-sync-point hint.                                                                                                                                                                               | A clean-room approximation should preserve transients around warp anchors, not only maintain average stretch ratios.                                  |
| **Stereo / phase coherence**      | zplane recommends processing stereo as a linked stereo instance rather than two mono instances, because both channels are linked during analysis.                                                                                                                                                                                                                 | Complex Pro should be treated as linked-channel processing to avoid stereo image drift and phase artifacts.                                           |

## Likely internal control model

A plausible clean-room model for Ableton Complex Pro is:

```text
source audio + warp markers + clip tempo map
        ↓
Ableton transient / tempo / warp analysis
        ↓
piecewise source-time → destination-time map
        ↓
zplane ELASTIQUE-style frequency-domain time/pitch engine
        ↓
optional spectral-envelope/formant compensation
        ↓
rendered warped clip output
```

The zplane SDK exposes separate **stretch** and **pitch** factors through `SetStretchQPitchFactor`; stretch factors are documented over a wide range, and pitch shifting is described as a combination of time stretching plus resampling.  That lines up well with Ableton’s distinction between clip warping, clip transposition, and Complex Pro’s formant/envelope controls.

A reasonable pseudo-implementation would look like this:

```pseudo
warp_map = build_piecewise_map_from_warp_markers(source_positions, output_positions)

pitch_factor = 2 ^ ((transpose_semitones + detune_cents / 100) / 12)

for each segment between warp anchors:
    stretch_factor = output_duration(segment) / input_duration(segment)

    if mode == ComplexPro:
        envelope_order = map_ableton_envelope_to_zplane_order(ui_envelope)
        envelope_factor = interpolate_formant_factor(
            neutral = 1.0,
            preserving = pitch_factor,
            amount = ui_formants_percent
        )

        process_with_elastique_pro_like_engine(
            stretch_factor,
            pitch_factor,
            envelope_factor,
            envelope_order,
            linked_channels = true,
            transient_sync_hints = warp/transient markers
        )
```

The uncertain part is the exact interpolation curve for Ableton’s **Formants** value. The zplane SDK says formant-preserving pitch shift is obtained when the spectral-envelope shift factor equals the pitch factor, and that envelope shifting happens before pitch shifting.  Ableton’s UI exposes this as a percentage, but the exact scaling curve, smoothing, automation behavior, and clamping are not publicly documented.

## Important distinction: Complex vs other Warp modes

Ableton’s manual broadly describes Warp Modes as using granular synthesis techniques, with “grains” selected, overlapped, and crossfaded differently depending on mode. ([Ableton][1]) But the zplane/Ableton interview specifically says the original Live algorithm was time-based/splicing/granular-like, while zplane’s Complex algorithm introduced a different, **frequency-based** technology. ([zplane][2])

So for reverse-engineering purposes:

* **Beats / Tones / Texture / Re-Pitch**: think Live-native/simple modes, largely time-domain or grain-oriented.
* **Complex / Complex Pro**: think zplane ELASTIQUE-style frequency-domain, phase-aware, polyphonic TSM/pitch shifting.
* **Complex Pro**: Complex plus exposed spectral-envelope/formant controls.

## Forum/community findings

Forum users mostly reinforce the official picture rather than reveal hidden internals.

On Ableton’s forum, users describe Complex Pro as useful for avoiding “munchkinized” pitch-shifted vocals, with Formants controlling how much vocal/body character is preserved and Envelope balancing warbling/graininess depending on source material. ([Ableton Forum][5])

KVR discussions repeatedly identify Ableton Complex/Complex Pro with zplane ELASTIQUE variants, but they also note an important practical point: even when two DAWs license the same zplane core, the output can differ because of **integration details** such as fixed vs continuously changing stretch ratios, exposed parameters, defaults, render mode, and analysis behavior. ([KVR Audio][6]) Another KVR thread notes that “Pro” is not automatically best for every extreme-stretch case; users sometimes prefer Efficient or other modes depending on material. ([KVR Audio][7])

That matters: **a clone of the zplane API settings alone would not necessarily clone Ableton Complex Pro output** unless the warp-map generation, transient anchoring, parameter smoothing, and buffer scheduling also match.

## What remains unknown / not publicly documented

The public docs do **not** reveal:

* exact ELASTIQUE version/build bundled in each Ableton version;
* Ableton’s exact Formants percentage → zplane envelope-factor curve;
* exact Envelope UI scaling/clamping, though the `128` default strongly matches zplane’s envelope order;
* internal FFT/window sizes, phase-locking rules, transient classification, lookahead, latency, and smoothing;
* how Ableton chunks audio into zplane buffers during real-time playback vs offline export;
* whether Ableton uses zplane’s higher-level warp API directly or a private integration layer.

Live 12 added modulation/MIDI mapping support for warp parameters including Complex Pro’s **Formants** and **Envelope**, but that tells us these are exposed controls, not the hidden DSP internals. ([Ableton][8])

## Clean-room reverse-engineering strategy

For a legal/clean-room approximation, the strongest path is black-box testing rather than binary reverse engineering:

1. Render controlled signals in Ableton:

   * impulse trains;
   * single sine sweeps;
   * harmonic stacks;
   * pink/white noise;
   * stereo phase-offset signals;
   * vocal-like formant test signals.

2. Sweep:

   * warp ratio;
   * pitch transposition;
   * Formants 0–100%;
   * Envelope values;
   * warp-marker density;
   * transient-aligned vs non-transient-aligned markers.

3. Compare against:

   * zplane ELASTIQUE Pro SDK or ELASTIQUE PITCH outputs;
   * phase-vocoder/WSOLA/granular baselines;
   * spectral-envelope-preserving pitch shifters.

4. Estimate:

   * Formants curve;
   * Envelope scaling;
   * latency/lookahead;
   * transient preservation behavior;
   * stereo phase-locking behavior;
   * behavior under time-varying stretch factors.

## Bottom line

The most defensible implementation reconstruction is:

**Complex Pro = Ableton warp/timeline analysis + zplane ELASTIQUE Pro-family polyphonic frequency-domain time-stretch/pitch-shift + spectral-envelope/formant preservation controls + Ableton-specific warp-marker/sync-point scheduling.**

You can approximate it well with a phase-aware, transient-preserving, stereo-linked ELASTIQUE-style engine and the public zplane controls. You cannot infer bit-exact behavior from public docs alone; the missing pieces are Ableton’s integration details and the exact bundled zplane library behavior.

## Lindelion Comparison

Linnod currently exposes four pitch-shift modes through `PitchShiftAlgorithm`: `SpectralPeak`, `Varispeed`, `TimeStretch`, and `ResampleStretch`. Those modes are source-sample playback engines for a mono melodic slicer. Complex Pro is a clip-warp engine for broad audio material with a dedicated stretch/pitch/formant control model.

| Complex Pro characteristic | Lindelion current behavior | Practical implication |
| ---- | ---- | ---- |
| Frequency-domain, phase-aware polyphonic TSM core | `ResampleStretch` now uses a Resample Pro path: source-level STFT analysis, phase-aware time scaling, transient phase reset, and bandlimited resampling back to the original slice duration. `SpectralPeak` remains a source-filter/peak model; `TimeStretch` is pitch-synchronous; `Varispeed` is resampled playback. | Resample Stretch is now in the same broad stretch-plus-resample family, while Ableton/zplane still has proprietary refinements and broader polyphonic/stereo/timeline behavior. |
| Separate stretch factor and pitch factor | `ResampleStretch` internally uses pitch-shift-as-time-scale-plus-resample for fixed-duration slice playback; varispeed changes playback increment. | Linnod exposes this as a slice pitch control rather than a user-facing clip warp/stretch control. |
| Spectral-envelope/formant factor plus Envelope order | Lindelion stores a pitch ratio plus optional `formant_ratio`; spectral-envelope detail is analysis-config driven, not a user-facing Envelope order. | Lindelion has a coarse formant-control concept but no Complex Pro-style Formants/Envelope parameter pair. |
| Warp markers and transient sync points | Linnod has slice markers and declicking; pitch synthesis does not use a piecewise warp map or transient sync-point API. | Complex Pro-style transient anchoring is a separate requirement from current slice-boundary handling. |
| Linked stereo analysis | Sources are decoded to mono for Linnod playback and pitch-shift cache construction. | Stereo phase-locking is outside the current Linnod pitch path. |
| CPU-heavy offline/clip-quality engine | Linnod renders Resample Stretch variants during setup/source preparation and note playback reads prepared buffers. | The Complex Pro-like path uses a prepared-cache boundary rather than a live per-note pitch-shift renderer. |

### Current Lindelion Modes

`SpectralPeak` is the default Linnod pitch mode. In pad mode it uses `formant_ratio = None`, so the pitch ratio drives the target partials while the source spectral envelope is preserved independently. Its strength is fixed-duration shifted playback with formant awareness. Its risk is that peak phases, frame interpolation, harmonic scaffolding, and residual passthrough can create a synthetic layer that differs immediately from direct source playback even for tiny shifts.

`TimeStretch` maps to the pitch-synchronous path. It is closest to PSOLA: it depends on voiced F0 and epoch analysis, reads waveform grains around pitch epochs, and overlaps them around target periods. It fits monophonic voiced material better than polyphonic clips. Its artifact profile is expected to be different from Complex Pro because it is time-domain and F0-dependent.

`ResampleStretch` renders through the Resample Pro stretch-plus-resample path and keeps the legacy enum name only for patch/UI compatibility. The compatibility path is guarded by tests that compare `ResampleStretch` output directly against the Resample Pro renderer, so it cannot silently fall back to spectral-peak residual synthesis. Linnod prepares Resample Stretch slice buffers before note playback; missing shifted variants are silent rather than replaced with an unshifted or alternate-algorithm fallback.

`Varispeed` uses playback-speed change for pitch. It preserves waveform character and avoids synthetic spectral reconstruction, but pitch and duration are coupled. It is closest to a simple Re-Pitch mode, not Complex Pro.

### Implications For 1-Cent Artifacts

The Complex Pro comparison pointed to three artifact classes in Lindelion's non-identity pitch path. `ResampleStretch` now addresses these through the Resample Pro path; the source-filter modes still need this framing when they are evaluated separately.

1. **Bypass discontinuity.** Identity playback reads source samples directly. A 1-cent shift enters synthesis/resampling paths immediately, so any fractional-read precision, interpolation, residual, or phase-model defect appears at the smallest nonzero pitch offset.
2. **Phase coherence.** Complex Pro's expected ELASTIQUE-family behavior is phase-aware across analysis frames. Lindelion's spectral paths synthesize frame-local peaks and scaffold harmonics, then blend frame outputs. That can produce roughness even when the requested pitch delta is tiny.
3. **Residual leakage.** Source-filter modes preserve residual energy from the source frame. If the residual descriptor classifies pitched or bright energy as aperiodic, the output can contain original-position/original-pitch material under shifted harmonic content.

The most relevant Complex Pro design lesson for Lindelion is not the exact zplane parameter mapping. It is that tiny pitch offsets still need the same phase-continuous, transient-aware, high-precision path as larger shifts, because users compare a 1-cent shift directly against identity playback.

[1]: https://www.ableton.com/en/manual/audio-clips-tempo-and-warping/ "Audio Clips, Tempo, and Warping — Ableton Reference Manual Version 12 | Ableton"
[2]: https://products.zplane.de/blog/gerhard-behles-interview-ableton "zplane interviews Ableton Live's CEO, Gerhard Behles"
[3]: https://www.ableton.com/en/release-notes/live-11/ "Live 11 Release Notes | Ableton"
[4]: https://products.zplane.de/blog/best-formant-shifting-plugins "Best Formant-Shifting Plugins in 2025: Transform Vocals and Audio with ELASTIQUE PITCH and five alternatives"
[5]: https://forum.ableton.com/viewtopic.php?t=197186 "Complex or Complex Pro? - Ableton Forum"
[6]: https://www.kvraudio.com/forum/viewtopic.php?start=15&t=557913 "Please recommend a Synth or Sampler with the best Single wave sample playback Engine - Page 2 - Instruments Forum - KVR Audio"
[7]: https://www.kvraudio.com/forum/viewtopic.php?t=226534 "Time Stretch - Elastique versus Elastique Pro? Opinions? - Hosts & Applications (Sequencers, DAWs, Audio Editors, etc.) Forum - KVR Audio"
[8]: https://www.ableton.com/en/release-notes/live-12/ "Live 12 Release Notes | Ableton"
