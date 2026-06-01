# Phase M10 — Effort-scaled surrounding effects (execution steps)

Expanded from milestone **M10** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M2], landed). Add the **"other surrounding effects on the sound"** — pick/breath/key
mechanical noise, sympathetic resonance, and radiation brightening — as a stage **around** the
resonator, scaled by the M2 effort/energy bus and **defeatable**. This is the last link of the
chain in [ADR-0014](docs/adr/0014-dynamic-response-effort-energy-bus.md):
*driver → coupling → resonator → two-way body → **surrounding***. `ModalBank` stays the untouched
reference; the stage applies to every voice (it sits on the voice output, after the resonator), but
defaults to defeated so the default patch and existing patches are unchanged.

## Reference behavior (re-derive before editing — verify against current code, not this summary)

**Where the surrounding stage attaches.** In `dsp/voice/mod.rs::process_sample_with_live_excitation`,
the voice computes `resonator_output = self.resonators.process_sample(excitation, sources.energy,
sources.effort)`, folds it into the energy follower (`self.modulation.observe_energy(...)`), then runs
`self.output.process_sample(resonator_output, …)`. The surrounding stage is a **new voice-level stage
between the resonator output and the output stage**, reading the same `sources.effort` /
`sources.energy` (M2 bus, `ModulationSources`, `Copy`). It runs at the **host rate** (the resonator
output is already decimated out of the engine's 2x loop), so no oversampling is needed.

**Effort vs energy (ADR-0014).** `ModulationSources::effort` is player effort (velocity/pressure);
`energy` is the measured resonator-energy follower. Per the confirmed split, *player effort* drives the
gesture-side surrounding effects (mechanical noise = how hard the pick/key/breath hits) and *measured
energy* drives the resonance-side ones (radiation brightening and sympathetic ringing track how much the
instrument is actually sounding). Step 1 may refine which bus drives which effect.

**Reusable DSP.** `lindelion-dsp-utils`: `BiquadCoefficients::high_shelf(sample_rate, cutoff, gain_db)` +
`Biquad` (radiation brightening = an energy-scaled high-shelf); `OnePoleLowpass`, `DelayLine` (4-point
fractional read, fixed capacity), `soft_saturate`, `Svf`. **Gaps to build:** there is **no noise/PRNG
source** in the workspace — mechanical noise needs a small, **seeded, allocation-free** deterministic
PRNG (xorshift/LCG) built locally; a seeded PRNG is deterministic, so it does not violate the unit-test
hygiene rule (no wall-clock, no `Math.random`-style nondeterminism).

**Sympathetic resonance is the underspecified piece.** The Lamath backlog separately lists "cross-coupling
or sympathetic-resonance routing where one resonator output can partially feed another resonator input"
(`docs/plugins/lamath-backlog.md`). M10's "sympathetic resonance" can be (a) a **per-voice sympathetic
bank** (a few tuned, lightly-damped resonators/combs excited by the voice output, ringing scaled by
energy), (b) **cross-resonator feed** (route part of resonator A's output into resonator B's input — an
engine/`resonator_stack` change), or (c) a **shared/global** sympathetic bank excited by the whole mix
(needs engine-level plumbing). This is the `[DECISION]`.

**Realtime safety (ADR-0001).** Every new audio-thread struct sizes its buffers at construction (the
sympathetic delay lines, the noise envelope) and adds `assert_no_allocations` coverage. Each effect is
**defeatable**: at depth `0` the stage is a pass-through, so a default patch renders identically.

## Scope note (read before step 1)

- The surrounding stage is a **new voice-level stage** (`dsp/voice/surrounding.rs`), owned by `Voice`,
  run on the resonator output before `OutputStage`. Applies to all resonator models (it is post-resonator,
  not waveguide-specific) — but defaults to defeated.
- **Radiation brightening** (energy-scaled high-shelf) and **mechanical noise** (effort-scaled attack
  transient) are the clear, cheap, measurable wins. **Sympathetic resonance** is the hard/ambiguous one;
  step 1 settles whether and how it ships.
- Keep the surrounding effects out of the shared `crates/` (they are Lamath voicing), and out of the
  oversampled loop (host-rate post-resonator).

---

## Steps

### 1. **[DECISION]** Which surrounding effects ship, sympathetic routing, and parameter seat — **needs your call**
- *(The milestone's `[DECISION]`: "Which surrounding effects ship and how sympathetic resonance is
  routed." It is underspecified on the sympathetic model, which sets step 7's scope, so this step settles
  all of it.)* No file change; the executor stops here. Resolve:
  - **Which effects ship:** **radiation brightening** + **mechanical noise** (recommended — both clear,
    cheap, measurable) ± **sympathetic resonance**. Recommend shipping all three if sympathetic earns its
    CPU under the 1–4 voice budget; otherwise ship the first two and defer sympathetic.
  - **Sympathetic routing (if it ships):** a **per-voice sympathetic bank** (recommended — a few tuned,
    lightly-damped resonators/combs excited by the voice output, scaled by energy; self-contained, fits
    the budget), vs **cross-resonator feed** (route part of resonator A into resonator B's input — the
    backlog item, a `resonator_stack` change), vs a **shared/global** bank (richest, needs engine-level
    plumbing). Also: per-voice vs global excitation.
  - **Which bus drives which:** recommend **effort → mechanical noise** (gesture), **energy → radiation
    brightening + sympathetic** (sounding level). Confirm or adjust.
  - **Parameter seat:** a new **`SurroundingConfig`** on the patch grouping the per-effect depths
    (recommended), each defaulting to `0` (defeated) so existing patches are unchanged.
- **Verify:** none (decision step). Resolution fixes the effect list, the sympathetic model, the bus
  assignment, and the parameter surface that steps 2–7 implement.

### 2. Add the surrounding-effects config to the patch  [depends on #1]
- **File(s):** `plugins/lamath/src/patch.rs` — a new `SurroundingConfig` with a `0..1` depth per shipped
  effect (e.g. `mechanical_noise`, `radiation_brightness`, and `sympathetic` if chosen), and a
  `#[serde(default)] pub surrounding: SurroundingConfig` field on `ResonatorSynthPatch`. Incidental: its
  `Default` impl (all depths `0`), inclusion in `ResonatorSynthPatch::default`, and the `lib.rs` re-export
  (mirror `ContactConfig`).
- **Reference behavior:** mirror the M9 `ContactConfig` pattern (`patch.rs`); a `#[serde(default)]` field
  + all-zero defaults means a default patch and any pre-M10 patch JSON deserialize unchanged and render
  identically (every depth `0` = defeated).
- **Change:** define `SurroundingConfig` and store it on the patch. No DSP wiring yet.
- **Verify:** a patch test asserts `SurroundingConfig::default()` has every depth `0`,
  `ResonatorSynthPatch::default().surrounding == SurroundingConfig::default()`, and that a config carrying
  non-default depths round-trips through the existing patch (de)serialization. Red→green: `SurroundingConfig`
  does not exist yet (compile-level red, greenfield).

### 3. Register the surrounding parameters  [depends on #2]
- **File(s):** `plugins/lamath/src/parameters.rs` (new `*_PARAMETER_ID`s, next free ids after 149),
  `parameters/paths.rs` (a `ParameterPath::Surrounding(SurroundingParameter)` arm + dispatch),
  `parameters/registry.rs` (one continuous `0..1` entry per depth), and a `parameters/surrounding.rs`
  read/write mapper if a sub-enum is used (mirror `parameters/contact.rs`).
- **Reference behavior:** the parameter registry is the single source of truth (CLAUDE.md). Mirror the M9
  contact-parameter wiring (`parameters/contact.rs`, the `ParameterPath::Contact` arm, the
  `CONTACT_*_PARAMETER_ID` registry rows); defaults map to `0`, so existing automation/patches are
  unchanged; route each `ParameterPath` to the #2 patch field.
- **Change:** add the depth parameters and wire their paths to `patch.surrounding`.
- **Verify:** a registry test asserts each new parameter exposes its `0..1` range with a `0` default and
  routes to the patch field (and the exhaustive `every_binding_round_trips_patch_get_set` auto-covers the
  new bindings). Red→green: the parameter ids / path arm do not exist yet (compile-level red).

### 4. Insert the surrounding stage into the voice (defeated pass-through)  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/voice/surrounding.rs` (new stage; `mod surrounding;` in
  `dsp/voice/mod.rs`), the `Voice` struct + `new`/`with_live_latch_capacity`/`clear` (own a
  `SurroundingStage`), the trigger path (set its config from `trigger.patch.surrounding`, mirroring
  `set_driver`/`set_contact`), and `process_sample_with_live_excitation` (run it on `resonator_output`
  with `sources.effort` / `sources.energy` before `self.output.process_sample`).
- **Reference behavior:** ADR-0014's "surrounding" link of the chain. The stage is a voice-level,
  host-rate stage reading the M2 bus (`ModulationSources`). At the defeated default (all depths `0`) it is
  an **identity pass-through** — the same regression discipline as M8's pass-through driver and M9's
  transparent contact stage — so the voice render is bit-identical to pre-M10.
- **Change:** add the `SurroundingStage` with its config setter and a `process(sample, effort, energy)`
  that returns `sample` unchanged at the defeated default; thread it into the voice between the resonator
  and the output stage. No effect DSP yet.
- **Verify:** a voice/stage test renders a voice with the default (defeated) surrounding config and asserts
  the output equals the pre-M10 path (RMS + sample-wise within tolerance), plus `assert_no_allocations`
  over the seam. Red→green: the `surrounding` stage/arguments do not exist on the process path yet
  (compile-level red); green = identical render through the new seam.

### 5. Implement radiation brightening (energy-scaled high-shelf)  [depends on #4]
- **File(s):** `plugins/lamath/src/dsp/voice/surrounding.rs` — an energy-scaled high-shelf using
  `BiquadCoefficients::high_shelf` + `Biquad` (`lindelion-dsp-utils`), coefficients sized/armed at
  construction and re-set at control rate from the depth + bus value.
- **Reference behavior:** a more energetically-radiating instrument throws more high-frequency energy into
  the room — model it as a high-shelf whose boost rises with the bus (energy per #1) and the depth. At
  depth `0` the shelf gain is `0 dB` (identity). Keep it bounded (a capped max boost) so it cannot run away.
- **Change:** apply the high-shelf to the surrounding-stage sample, shelf gain = f(depth, bus).
- **Verify:** an objective test renders a voice at low vs high energy with brightening engaged and asserts
  the high-frequency ratio / `spectral_centroid_hz` rises measurably with energy; and that at depth `0`
  the render is unchanged vs energy (defeatable). Add `assert_no_allocations`. Red→green: at depth `0`
  (or pre-#5) the centroid does not move with energy (the behavior is new).

### 6. Implement effort-scaled mechanical noise (pick/breath/key)  [depends on #4]
- **File(s):** `plugins/lamath/src/dsp/voice/surrounding.rs` — a seeded, allocation-free PRNG
  (xorshift/LCG) + a fast attack envelope + a shaping filter (`OnePoleLowpass`/`Biquad`), all sized at
  construction; the noise burst is added to the surrounding-stage sample, scaled by effort and depth.
- **Reference behavior:** the mechanical transient of the gesture — a pick/key click or breath rush — is a
  short, force-dependent noise burst at the attack, not the resonator tone. Harder playing (more effort)
  makes it louder/brighter. Re-derive the envelope (a fast note-on-triggered decay) and the spectral shape
  (pick = bright, key = low, breath = band-limited; the exact archetype(s) per #1). The PRNG is **seeded**
  (deterministic for tests), allocation-free (ADR-0001). At depth `0` it adds nothing (defeatable).
- **Change:** trigger the noise burst at note-on, shape it, add it scaled by `effort · depth`.
- **Verify:** an objective test renders the attack at low vs high effort and asserts the attack-window
  broadband/aperiodic energy rises with effort; and that at depth `0` the noise contribution is silent
  (defeatable). Deterministic (seeded PRNG). Add `assert_no_allocations`. Red→green: at depth `0` (or
  pre-#6) there is no effort-scaled attack noise.

### 7. Implement sympathetic resonance  [depends on #1, #4]  [DECISION]
- *(Scope is fixed by step 1; if step 1 deferred sympathetic resonance, **skip this step** and note it in
  the exit gate.)*
- **File(s):** `plugins/lamath/src/dsp/voice/surrounding.rs` (per-voice bank) **or**
  `plugins/lamath/src/dsp/voice/resonator_stack.rs` (cross-resonator feed) — per the #1 routing decision —
  with fixed-capacity delay lines/resonators sized at construction.
- **Reference behavior:** sympathetic strings/modes ring in response to the sounding note, their level
  tracking how much the instrument is sounding (energy). Re-derive the chosen model: a per-voice bank of a
  few tuned, lightly-damped resonators (one-pole-damped combs / `DelayLine` loops) excited by the voice
  output and summed back, scaled by energy and depth; **or** the cross-resonator feed (a bounded fraction
  of one resonator's output into the other's input). Passive/bounded (it must not inject unbounded energy);
  at depth `0` it adds nothing (defeatable).
- **Change:** add the sympathetic bank/feed per #1 and sum its (energy-scaled) contribution.
- **Verify:** an objective test asserts measurable sympathetic ringing that scales with energy — e.g. a
  longer/again-excited tail or energy at the sympathetic frequencies that grows with the bus — and that at
  depth `0` it is inert (defeatable); finite/bounded across the sweep. Add `assert_no_allocations`.
  Red→green: at depth `0` (or pre-#7) there is no sympathetic content.

### 8. Exit gate + doc/ADR surface  [depends on #5, #6, #7]
- **File(s):** verification only, plus the doc surface: `CHANGELOG.md`, and a new ADR (next free number is
  **ADR-0028**) if step 1's choices are lasting (sympathetic routing especially) — delegate to the
  **repo-docs** skill for the ADR + index row, as M9 did with ADR-0027. Update the Lamath backlog line if
  the sympathetic/cross-coupling item is now satisfied or re-scoped.
- **Reference behavior:** M10 exit — "`make ci` green; surrounding components scale measurably with effort
  and are defeatable; no-alloc." Per-milestone CPU via `make bench` at the 1–4 voice budget.
- **Change:** one curated `CHANGELOG.md` line (the waveguide/instrument gains effort/energy-scaled
  surrounding effects — mechanical noise, radiation brightening, and sympathetic resonance — each
  defeatable and off by default); write ADR-0028 if warranted; reconcile the backlog item.
- **Verify:** run `make ci` (green) and show output; the #5/#6/#7 objective + defeated-default tests
  satisfy the scale-with-effort and defeatable clauses; the #4–#7 `assert_no_allocations` cover no-alloc;
  `make bench` shows the per-voice cost within the 1–4 voice budget.

---

## Decisions to resolve at execution (step 1)

| Facet | Decision you own | Recommended default |
| ----- | ---------------- | ------------------- |
| Effects shipped | radiation brightening / mechanical noise / sympathetic resonance — which ship. | Brightening + mechanical noise; sympathetic if it fits the budget, else defer. |
| Sympathetic routing | Per-voice bank vs cross-resonator feed (backlog item) vs shared/global; per-voice vs global excitation. | Per-voice sympathetic bank, scaled by energy. |
| Bus assignment | Which of effort/energy drives each effect. | effort → mechanical noise; energy → brightening + sympathetic. |
| Parameter seat | `SurroundingConfig` on the patch vs another home; per-effect depth controls. | `SurroundingConfig` with a `0..1` depth per effect, default 0 (defeated). |
| Noise archetypes | Which mechanical-noise archetype(s) ship (pick / breath / key) and their shaping. | Pick + breath, shaped by a one-pole; key optional. |

---

To execute, run the EXECUTE-PHASE companion prompt on this file; the executor stops at step 1's `[DECISION]`.
