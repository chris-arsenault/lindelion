# Lamath Waveguide Implementation Plan

Immediate implementation plan for replacing the current Lamath waveguide behavior with physically stronger 1D targets and an analysis-first 2D path.

## Current State

Lamath currently has a single delay-loop waveguide in `plugins/lamath/src/dsp/waveguide.rs`.

- The loop uses one `DelayLine`, one `FirstOrderAllpass`, one biquad loop lowpass, optional `soft_saturate`, and scalar boundary gain helpers.
- `WaveguideStyle::String` and `WaveguideStyle::Tube` share the same core loop. Tube behavior is currently a scalar boundary/reflection coloration, not a full bore or scattering model.
- There is no true 2D digital waveguide mesh in Lamath. `BodyColor` routing uses one resonator to color excitation into another resonator.
- The modal path sounds more natural because it has richer resonant structure and more explicit control over mode frequency, gain, decay, brightness, and strike position.

## Shared Targets

All new waveguide work must satisfy these constraints.

- Keep audio-thread processing allocation-free after construction.
- Keep per-sample work bounded and predictable.
- Preserve finite-output protection under malformed automation, modulation, and sidechain excitation.
- Add objective render tests before subjective tuning.
- Make every sonic control map to a measurable DSP behavior.
- Keep current patch compatibility unless the user explicitly approves a patch-format break.
- Treat String, Tube, and future mesh behavior as separate models internally, even if the UI keeps a compact selector.

## Phase 1: Measurement Harness

Build the measurement harness before replacing DSP.

### Deliverables

- Add deterministic offline renders for impulse, shaped pluck, noise burst, sidechain burst, and sustained excitation.
- Render across note range, sample rates, loop gain, damping, strike position, nonlinearity, and style.
- Emit analysis data under the existing plot-data pattern only if useful for local review; otherwise keep the harness as tests.
- Add helpers for pitch estimate, partial tracking, per-partial decay slope, spectral centroid over time, peak/RMS, DC, and finite-output checks.

### Acceptance

- Current waveguide failures are visible as data: pitch drift, excessive periodicity, poor high-partial decay, harsh loop-filter resonance, or style controls that only scale amplitude.
- Tests can compare old and new render metrics without relying only on golden samples.

## 1D Design Target

The 1D target is a credible string/tube waveguide family, not just a Karplus-Strong loop with a lowpass.

### 1D Model Split

Create internal model variants with separate parameter derivation.

- `String1d`: plucked/struck string with calibrated damping, excitation position, pickup position, optional stiffness dispersion, and body/radiation output.
- `Tube1d`: bore-like model with open/closed reflection behavior, reflection filtering, bore loss, and excitation coupling appropriate to pressure/velocity behavior.

The existing `WaveguideStyle` can continue to select String or Tube in the patch model, but the DSP implementation should stop sharing a mostly identical scalar loop.

### 1D Core Structure

Prefer an explicit traveling-wave or two-port formulation for the new core.

- Use two delay paths or an equivalent loop with explicit endpoint filters.
- Separate excitation injection position from output/pickup position.
- Add bridge/nut or open/closed-end termination filters instead of scalar feedback multipliers.
- Keep a simplified collapsed-loop mode only if it matches the measured behavior of the explicit model.

### 1D Tuning

Design tuning around total phase delay, not raw `sample_rate / frequency - 1`.

- Account for integer delay, fractional delay, loop-filter group delay, termination-filter group delay, and dispersion-filter delay.
- Target steady-state pitch error below 3 cents from 30 Hz through 4 kHz at 44.1, 48, 88.2, and 96 kHz.
- Compare first-order allpass, higher-order Thiran allpass, and Lagrange/FIR fractional delay.
- Choose fractional delay per model: allpass for loop energy preservation, FIR/Lagrange if phase behavior sounds better and damping compensation remains stable.

### 1D Damping

Replace generic loop lowpass behavior with calibrated loop loss.

- Express damping as target decay time rather than only cutoff/gain.
- Support frequency-dependent `T60(f)` so high partials decay faster than low partials for natural string behavior.
- Derive loop gain from target decay per period.
- Keep loop magnitude below unity across frequency after all filters and nonlinear stages.
- Make resonance controls passive or explicitly gain-compensated by measured loop magnitude, not a fixed scalar.

### 1D Dispersion

Add optional dispersion for stiff or metallic string behavior.

- Implement an allpass dispersion section or chain after the basic damping model is stable.
- Parameterize by perceptual stiffness/inharmonicity, mapped to partial-frequency stretch.
- Verify that dispersion moves upper partials without breaking the fundamental tuning target.
- Keep dispersion off or very low for natural nylon/soft-string presets.

### 1D Excitation

Replace one-sample impulse assumptions with physically plausible excitation layers.

- Pluck: finite-width displacement/velocity impulse, position-dependent harmonic notches, hardness-controlled brightness.
- Strike: short force pulse with velocity-to-brightness mapping.
- Noise/scrape: filtered noise burst for pick, mallet, breath, or sidechain onset texture.
- Sidechain: envelope-followed and spectrally shaped injection that cannot overdrive the loop into digital buzz by default.

### 1D Body And Radiation

Do not output the bare loop as the final instrument sound.

- Add an output body/radiation stage after the waveguide.
- First implementation can reuse the modal bank as a lightweight body-color stage.
- Later implementation can use a compact body filter or short commuted response if needed.
- Give string presets body profiles such as wood, metal frame, glass, and muted.
- Give tube presets radiation/open-end profiles rather than only loop feedback changes.

### 1D Nonlinearity

Keep nonlinear behavior controlled and physically placed.

- Move generic loop saturation behind a parameterized model such as bridge/tension softening, reed/lip drive, or endpoint compression.
- Verify every nonlinear setting with oversampled or alias-sensitive renders before exposing high drive ranges.
- Default to linear, calibrated behavior first; add nonlinearity only after the natural baseline is good.

### 1D Tests

Add focused tests alongside implementation.

- Pitch stays within target cents across note/sample-rate matrix.
- Partial decay follows expected slope for short, medium, and long damping settings.
- Strike/pickup position creates expected harmonic notches.
- Loop remains finite and bounded under max automation and sidechain bursts.
- Style changes alter spectral/temporal behavior, not only gain.
- Audio-thread path remains allocation-free.

## 2D Design Target

The 2D target is not an immediate product replacement. It is an analysis prototype first, because mesh models can sound worse than modal banks if dispersion and boundary loss are not handled carefully.

### 2D Model Scope

Use 2D only where it fits the instrument family.

- Membranes, plates, soundboards, gongs, and spatially struck surfaces are valid 2D targets.
- Ordinary plucked strings and simple tubes should remain 1D.
- Bell, bar, and plate-like sounds may continue to use ModalBank if it remains more natural and cheaper.

### 2D Prototype

Build an offline prototype before adding realtime parameters.

- Start with a small rectangular mesh to validate scattering, boundary loss, and measurement tools.
- Add a triangular mesh variant if rectangular dispersion is too directional.
- Support fixed dimensions, sample rate, wave speed, boundary damping, strike position, and output pickup position.
- Keep prototype render functions outside the product runtime until metrics justify promotion.

### 2D Scattering And Boundaries

Implement real mesh behavior, not a colored delay loop.

- Use scattering junctions with bidirectional delay elements.
- Support fixed, free, and lossy boundaries.
- Let boundary damping vary by edge so plates and membranes can have asymmetric losses.
- Include energy checks so junction math is passive when damping is zero.

### 2D Dispersion Analysis

Treat dispersion as the main risk.

- Measure propagation speed by frequency and direction.
- Compare rectangular and triangular topology.
- Quantify anisotropy by rendering identical strikes at rotated positions.
- Reject mesh settings that produce obvious grid-tone artifacts or metallic stepping unless intentionally exposed as an effect.

### 2D Excitation And Pickup

Expose spatial controls only if they produce useful sound.

- Strike position should change modal participation and stereo image.
- Pickup position should change spectral notches and decay character.
- Excitation width should control brightness without causing unstable local energy.
- Stereo pickup can use two read positions, not a post-pan of a mono mesh.

### 2D Runtime Criteria

Promote 2D into Lamath runtime only if it passes these gates.

- It provides sounds that ModalBank plus body-color routing cannot produce convincingly.
- CPU cost fits the per-voice budget or uses a shared/resampled body model.
- Memory is fixed at construction and bounded by maximum mesh dimensions.
- The realtime implementation has no heap allocation in `process_sample`.
- Stability tests pass at all exposed parameter extremes.

### 2D Product Shape

If promoted, expose 2D as a distinct resonator model.

- Do not hide it behind the current String/Tube style selector.
- Use model names that reflect behavior, such as Plate, Membrane, or Mesh.
- Keep UI parameters physical and limited: material, size, damping, tension/stiffness, strike position, pickup spread.

## Implementation Sequence

1. Add measurement harness for current waveguide.
2. Build `String1d` offline or behind an internal feature gate.
3. Replace raw loop gain/filter mapping with calibrated damping and measured tuning.
4. Add improved excitation and pickup position handling.
5. Add body/radiation output using a modal-body stage or compact body filter.
6. Add optional string dispersion.
7. Split Tube into a real 1D bore model or remove misleading tube semantics from the implementation.
8. Run A/B render metrics against current waveguide and modal presets.
9. Build the offline 2D mesh prototype.
10. Promote 2D only after it beats ModalBank for a concrete target sound.

## First Coding Pass

The first coding pass should stay narrow.

- Add analysis helpers and tests without changing the shipped sound.
- Add pitch, partial-decay, and position-notch metrics.
- Render current String and Tube outputs into deterministic analysis cases.
- Record which metrics fail badly enough to guide the first DSP replacement.

The second coding pass should replace the 1D string core behind the existing patch surface while preserving old patch loading.
