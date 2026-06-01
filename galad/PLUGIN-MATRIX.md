# Galad — Plugin Compatibility Matrix & Soak Record

Field-verification artifact for Galad's M7 robustness pass. Galad is Windows-only and its live
behaviour is verified by running on Windows (not by `make ci`); this document records the results of
that field testing. See [`README.md`](README.md) ("Robustness (M7)") for what the host contains and
its residual limits.

## How to populate (on Windows)

For each plugin, run it through Galad and record the outcome:

1. **Load** — `galad` → *scan folder…* (or *add plugin…*); the plugin validates and is added.
   (Validation runs the load probe from `vst3_host/validate.rs`.)
2. **Process** — *Start* with a live input/output device; audio passes through the plugin without
   glitching, and reordering/bypassing it behaves.
3. **Editor** — *editor* on its chain row opens the plugin's own window; parameter edits are audible.

Legend: ✓ pass · ✗ fail (note why) · — not yet tested.

## Lindelion Windows VST3s

These are the in-house Windows VST3s (ADR-0023); the macOS instruments (Lamath/Linnod/Glirdir) are
not part of this matrix.

| Plugin | Vendor | Version | Load | Process | Editor | Notes |
| ------ | ------ | ------- | :--: | :-----: | :----: | ----- |
| Cenedril | Lindelion | — | — | — | — | Passthrough visualizer; needs a Windows `.vst3` build. |
| Calóma  | Lindelion | — | — | — | — | Pending — added once Calóma ships. |

## Third-party VST3s

Populate with the real-world plugins Galad is tested against (compressors, EQs, reverbs, gates,
de-essers, etc.). Aim for a spread of vendors and editor frameworks.

| Plugin | Vendor | Version | Load | Process | Editor | Notes |
| ------ | ------ | ------- | :--: | :-----: | :----: | ----- |
| _(example)_ | _vendor_ | _x.y_ | — | — | — | _fill in on Windows_ |

## Robustness checks (field)

Record the M7 containment/recovery behaviours observed on real hardware/plugins:

| Check | Result | Notes |
| ----- | :----: | ----- |
| Incompatible plugin is rejected at add (no crash) | — | e.g. a non-audio `.vst3`. |
| A plugin emitting NaN/garbage does not reach the output | — | output stays finite (chain guard). |
| Device unplug → engine stops + UI notice (stop+notify) | — | re-select device + Start resumes. |
| Device replug → re-scan/select + Start works | — | |

## Soak record (M7 Step 10)

**Procedure.** Run Galad live for an extended session (target ≥ several hours) with a representative
chain, periodically: toggling bypass, reordering slots, adding/removing plugins, opening/closing
editors, and unplugging/replugging the audio device. Watch for drift in memory (RSS / private bytes)
and in GDI + USER handle counts (Task Manager / Process Explorer), and for any audio glitch
accumulation.

**Pass criteria.** No unbounded growth in memory or handle counts over the run; no crash; audio stays
glitch-free; chain edits and device recovery keep working to the end.

| Date | Duration | Build | Peak RSS | Handle drift | Result | Notes |
| ---- | -------- | ----- | -------- | ------------ | :----: | ----- |
| — | — | — | — | — | — | _first soak run pending on Windows_ |
