# Lamath Waveguide Completion Plan

Numbered plan to finish the Lamath waveguide work. The 1D string/tube split and the 2D mesh
prototype are already built; this plan captures only the **remaining** work, grounded in a
measurement-based validation.

Run it phase by phase: each phase is independently testable, measurement-first, keeps the audio
thread allocation-free and finite-output-protected, and adds objective tests before subjective
tuning. Phases tagged **[DECISION]** require sign-off before implementation.

## Verified current state (from validation)

- **String 1D tuning: correct.** DFT magnitude-peak measurement shows ~0.0 cents across
  30 Hz–4 kHz at 44.1/48/88.2/96 kHz. No work needed beyond the acceptance gate.
- **Damping: T60-calibrated.** `core::gain_for_t60` derives loop gain from a target decay time;
  the remaining sub-gap is explicit frequency-dependent `T60(f)`.
- **Tube 1D tuning: defective.** The bore resonates roughly an octave (or more) below the target
  for frequencies above ~165 Hz; correct only at the bottom. Root cause: the terminations are
  asymmetric (mouth reflection inverting, end non-inverting), which makes the bore a
  quarter-wave resonator, but the tuning assumes the string's half-wave relationship
  (`cycle_divisor = 2.0`).
- **2D mesh: prototype only.** `waveguide/mesh_2d.rs` plus `mesh_2d/promotion_tests.rs` exist
  but are not wired into the runtime style selector (`WaveguideStyle` is `String`/`Tube`).
- **Measurement caveat.** Autocorrelation pitch estimators are unreliable here (integer-lag
  quantization; window-drift on mistuned tones). A DFT magnitude-peak scan is trustworthy and
  is the basis for Phase 1.

## Shared constraints

- Allocation-free, lock-free, bounded audio-thread processing after construction (ADR-0001).
- Preserve finite-output protection under malformed automation, modulation, and sidechain input.
- Add objective render tests before subjective tuning; every sonic control maps to a measurable
  behavior.
- Preserve patch compatibility unless a break is explicitly approved (see Phase 2 and Phase 6).

## Phase 1 — Precise pitch-measurement harness (prerequisite)

Build the measurement the rest of the plan depends on; the existing autocorrelation harness
cannot resolve a few cents or bracket a badly mistuned model.

- Add a sub-cent, wide-range fundamental estimator (DFT magnitude-peak with parabolic
  interpolation, scanning wide enough to bracket at least an octave of error) usable by tests.
- Validate it against synthetic sines of known pitch across 30 Hz–4 kHz at 44.1/48/88.2/96 kHz.
- **Acceptance:** on pure sines the estimator reports < 1 cent error across the whole matrix
  (its noise floor), and it locates a deliberately detuned test tone without locking to the
  target.

## Phase 2 — Tube quarter-wave tuning fix [DECISION]

Correct the bore so `frequency_hz` is the played pitch.

- **[DECISION]** Correcting the pitch raises existing Tube patches ~an octave. Confirm the
  pitch correction is wanted and that a patch-behavior change (recorded in `CHANGELOG.md`) is
  acceptable, since today's Tube plays roughly an octave flat.
- Make the tuning quarter-wave-aware (account for the inverting-termination half-cycle phase, or
  the equivalent delay relationship) so the bore resonates at the requested frequency across
  30 Hz–4 kHz; keep the string path unchanged.
- Re-validate excitation injection, pickup taps, and boundary behavior at the corrected delay
  length (they scale with the loop length).
- **Acceptance (Phase 1 harness):** Tube tuning < 3 cents across the matrix; output stays
  finite, bounded, and decays; existing Tube render/finite tests still pass.

## Phase 3 — Tuning acceptance gate `[depends on 1, 2]`

- Add a permanent test asserting **String and Tube** steady-state tuning < 3 cents across
  30 Hz–4 kHz at 44.1/48/88.2/96 kHz, using the Phase 1 estimator. (String already passes;
  Tube passes after Phase 2.) This replaces the current `< 150 cents` smoke check.

## Phase 4 — Frequency-dependent T60(f) damping

- Extend loop damping so high partials decay faster than low partials via a calibrated
  `T60(f)`, rather than a single decay time plus the loop lowpass approximating it. Derive loop
  gain per partial band from the target decay; keep total loop magnitude below unity after all
  filters and any nonlinearity.
- **Acceptance:** a per-partial decay-slope test shows high partials decaying measurably faster
  than low partials for a natural-string setting, and overall `T60` matches the target within a
  stated tolerance.

## Phase 5 — Remaining acceptance tests

- Per-partial decay-slope test across short / medium / long damping settings.
- Nonlinearity alias test: at high drive an oversampled or alias-sensitive render shows bounded
  aliasing, and the default linear path stays clean.
- Strike/pickup position notch test confirming expected harmonic notches (if not already
  covered by `position_tests.rs`).

## Phase 6 — Promote the 2D mesh as a new resonator model [DECISION]

Promote the mesh to a distinct resonator model **alongside** ModalBank and the 1D waveguide.

- **[DECISION]** Confirm the product shape: a new model name reflecting behavior (Plate /
  Membrane / Mesh), exposed as its own selectable resonator type (not behind the String/Tube
  style), with physical, limited parameters (material, size, damping, tension/stiffness, strike
  position, pickup spread). Confirm the patch-model addition and its migration default.
- Wire the model into voice/engine routing, the patch model (with a compatible default for old
  patches), and the editor UI.
- **Promotion gates (must pass):** allocation-free `process_sample`; fixed memory bounded by
  maximum mesh dimensions; stable at all exposed parameter extremes; finite output; CPU within
  the per-voice budget; and an objective render test showing a target plate/membrane sound that
  ModalBank plus body-color routing cannot produce.

## Phase 7 — Documentation and cleanup `[depends on all]`

- Add a waveguide technique catalog (mirroring `docs/dsp/pitch-shift-techniques.md`) and/or
  extend `docs/dsp/waveguide.md`; add an ADR for the Tube tuning correction and the new 2D
  resonator model; add `CHANGELOG.md` entries.
- Remove this plan from the repo root once its content is captured durably.

## Suggested order

`1 → 2 → 3 → 4 → 5 → 6 → 7`. Phase 1 is a hard prerequisite for 2 and 3. Phases 4 and 5 are
independent of the Tube fix and can run in parallel with it if useful. Phase 6 is the largest and
is gated on the promotion criteria; it can be deferred without blocking 1–5.
