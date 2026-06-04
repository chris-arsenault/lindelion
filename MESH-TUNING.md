# Mesh (Idiophone) Tuning Tracker

Working tracker for tuning the 2-D waveguide-mesh idiophone. Not durable docs — fold into
the spec/CHANGELOG and delete when the open items land.

**Where the code lives now** (post family-extraction):
- DSP: `crates/lindelion-idiophone/` — `mesh.rs` (kernel), `runtime.rs` (`MeshVoiceParams`,
  `voice_config`, damping/note maps), `boundary.rs` (HF-loss).
- Plugin: `plugins/lamath-cymbal/` — `patch.rs` (`CymbalPatch`), `parameters.rs` (7 host params),
  `processor.rs` (strike/damp/injector/energy→bloom).

**Model fact that governs everything here:** it is a *unit-delay* rectangular waveguide mesh, so
the wave speed is structurally fixed at 1 cell/sample. Pitch and modal structure come from the
**grid cell count**, not the played note. It is a fixed-pitch struck idiophone, not a tuned voice.
`wave_speed_mps` / `physical_width_m` / `physical_height_m` have no slot in the update and are dead.

## Done (verified)

- **Damping range (P2).** `boundary_damping_loss` / `mesh_decay_k` (`runtime.rs:198`, `:206`) invert
  the (1,1)-mode decay `(1−loss)^(t·fs·(1/W+1/H))` and map the knob geometrically over a musical T60
  band `MESH_T60_MAX_S=4.0` → `MESH_T60_MIN_S=0.30` (`runtime.rs:187`). No degenerate/thud zone;
  sample-rate- and grid-independent. Guard: `mesh_damping_control_has_no_degenerate_region`
  (`runtime.rs`, pure math, runs in `make ci`).
- **Grid-density timbre.** `size`/`tension` → active grid width/height in cells via `grid_dim`
  (`runtime.rs:120`, range `MESH_MIN_WIDTH=10`..`MAX_MESH_WIDTH=64` × `MESH_MIN_HEIGHT=8`..
  `MAX_MESH_HEIGHT=48`). Buffers allocated once at `MAX_MESH_CELLS` (`mesh.rs:21`); active sub-region
  (stride = active width) so re-tuning is allocation-free. Small grid = sparse/near-pitched
  (triangle), large = dense/inharmonic (cymbal). **User: timbres distinct & on-name, "ride especially good."**
- **Output level / radiation model.** The old pre-extraction `ResonatorStack` makeup
  (`MESH_OUTPUT_MAKEUP=300.0`) was masking a mesh I/O bug: the strike was sum-normalized over a
  2-D footprint and the pickup averaged pressure over its aperture, so large grids produced tiny
  local pressure and needed ~45 dB of downstream gain. The mesh now fixes this in the simulation:
  strike weights are energy-normalized, the broad pickup is a signed body/radiation aperture, and
  dense plates add a density-scaled narrow energy-normalized shimmer/curvature tap. The old
  `cells/MESH_LEVEL_REF_CELLS` compensation is neutral. Current mesh-tag auditions are audible
  without family output makeup; final mesh-tag render: highest peak `mesh_chord_polyphonic`
  ≈ −2.84 dBFS, quietest audible `mesh_timbre_density_sparse` ≈ −43.42 dBFS; timbre peaks:
  ride/crash ≈ −8.4 dBFS, dense ≈ −4.1 dBFS, triangle ≈ −13.1 dBFS.
- **Note → timbre, not pitch.** `mesh_note_position(freq)` maps C2–C6 to `0..1`; the note shapes
  modal color and never changes wave speed/grid tuning (correct for an idiophone).
- **Wider note → timbre range (A).** `frequency_hz` still does not tune the unit-delay mesh, but
  now drives both strike and pickup over independent 2-D paths (`mesh_note_strike_position`,
  `mesh_note_pickup_position`, `runtime.rs`). The pickup moves partly opposite the strike so the
  note has two modal selectors instead of one fixed readout. Guard:
  `mesh_note_mapping_uses_independent_strike_and_pickup_paths` (pure math, runs in `make ci`).
- **Retune/restrike click fix.** The mesh now reads the radiating state before injecting the
  current strike sample, so nearby source/pickup apertures cannot output an unpropagated hammer
  impulse. The note pickup path also enforces a minimum source/pickup separation. Guard:
  `pickup_does_not_read_same_sample_strike_force`; cymbal retune continuity guard tightened to
  `max_adjacent_delta < 0.2`. Final mesh articulation WAVs stay under ≈0.15 local onset delta.
- **Crash bloom restored.** The dense crash path had collapsed to a dark ~1 kHz area-integral
  readout. The density-scaled shimmer/curvature radiation tap restores sustained high-band content:
  final crash early centroid ≈2.1 kHz, >3 kHz ratio ≈0.063, and mid/early RMS ≈0.71 (ride ≈0.49).
  Guard: `crash_voicing_keeps_dense_bloom`.

## Open / identified (priority order)

### A. Multi-param degeneracy audit
Each control — `material`, `size`, `tension`, `pickup_spread`, `position_of_strike` — must be valid
across its whole `0..1` range (no silent/degenerate sub-region) **or be fixed**. Use bisection /
gradient-free optimization, **not** grid sweeps (see method note below). Only `damping` is done.

### B. Geometric "bloom" calibration vs grid size
The radiation-side crash bloom is restored, but the nonlinear geometric drive still uses the
measured output-energy bus via `GEOMETRIC_ENERGY_REF=0.003` (`mesh.rs`) rather than an internal
stored-energy estimate. Revisit this if future tuning shows bloom engagement drifting across
`size`; prefer an internal/density-normalized drive over any post-output gain.

### C. Dead-field cleanup
Remove `wave_speed_mps` / `physical_width_m` / `physical_height_m` from `RectangularMesh2dConfig`
(`mesh.rs`) + the `#[cfg(test)] mode_frequency_hz` helper — unused by the unit-delay update.

## Method note
For DSP parameter ranges/bad-states: derive the physics in closed form and invert it (as in the
damping fix), validate against existing data instead of re-rendering, bisect to find boundaries, and
use ML-adjacent optimization for the multi-param space — never brute-force grid sweeps.

## Audition
Cymbal audition path is being built separately. Render via the catalog `--tag mesh` (renders all
mesh cases) / `--case <id>`; `make render-lamath-audio` builds release into `target-release/`.
