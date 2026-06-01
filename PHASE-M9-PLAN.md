# Phase M9 — Coupling/contact stage and source↔body balance (execution steps)

Expanded from milestone **M9** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M7, M8], both landed). Two deliverables on the **waveguide** path:

1. **A coupling/contact stage between the driver and the resonator** — the *gesture interface*:
   **strike-position spread**, **contact time**, and **slip**, so a pick (tight, point contact) and a
   strum (spread, slower contact) read as different timbres, not different levels.
2. **An energy-dependent source↔body balance** on the String output — the *dynamic balance between
   subsystems*: warmth (body-weighted) at low dynamics, directness (pickup-weighted) at high dynamics,
   **level-preserving** so the change is timbral, not gain.

`ModalBank` stays the untouched reference (no contact stage, no balance). Mesh has no driver/body and is
out of scope unless step 1 widens it. The milestone's exit is: `make ci` green; **picked-vs-strummed and
hard-vs-soft produce distinct, measurable timbres via contact + balance (not gain alone)**; no-alloc.

## Reference behavior (re-derive before editing — verify against current code, not this summary)

**Where the driver couples into the resonator today (M8).** In
`dsp/voice/resonator_stack.rs::ResonatorEngine::process_sample`, the waveguide path runs the driver
**inside** the 2x oversampled closure: each sub-sample reads the resonator's input-end returning wave
(`waveguide.driven_feedback(params)`), runs `driver.process(sample, effort, feedback)`, then
`waveguide.process_sample(driven, params)`. The driven scalar is injected by the waveguide via
`add_symmetric_excitation`. **There is no stage between the driver output and that injection** — the
contact stage is that seam ("between driver and resonator").

**Strike-position spread lives in the geometry (control-rate).** `dsp/waveguide/core.rs::waveguide_geometry`
builds **3 excitation taps** spanning a **fixed** `EXCITATION_WIDTH_FRACTION = 0.035` around the strike
position (gains 0.25 / 0.5 / 0.25), consumed by `traveling.rs::add_symmetric_excitation`. The taps are
cached in `PreparedStringModel`/`PreparedTubeModel` behind the params dirty-check, so the **width is a
control-rate quantity**: widening it is a prepared-model input, not a per-sample cost. A wider contact
attenuates partials whose half-wavelength approaches the contact width (a broad pluck excites fewer high
partials) — that comb difference is the picked-vs-strummed timbre, independent of level.

**Contact time / slip are temporal and per-sample.** A longer contact time spreads the momentum transfer
in time (a slower attack, fewer high partials in the onset); slip is a contact release/breakaway. M8's
`PickConfig::contact_time` today only lowers a feed-forward low-pass *ceiling* inside `PickDriver`
(`resonator_stack/driver.rs`) and does nothing for the `Sample` (pass-through) driver. M9's contact stage
is the coupling itself, so contact-time/slip shape the **driven excitation** regardless of driver type.

**Source↔body balance is two fixed constants today (M7, String only).**
`dsp/waveguide/string_1d.rs` produces `output = STRING_OUTPUT_TRIM * (STRING_PICKUP_MIX * pickup_tap +
STRING_BODY_MIX * radiated)` with `STRING_PICKUP_MIX = 1.0`, `STRING_BODY_MIX = 3.0`, `STRING_OUTPUT_TRIM
= 0.85` — *constant* (ADR-0021). The pickup tap is the direct/bright "source"; `radiated` is the body's
"warm" voice. M9 makes the **pickup↔body weighting energy-dependent and level-preserving**: low measured
energy leans to `radiated` (warm), high energy leans to `pickup_tap` (direct). The body still loads the
loop two-way either way (ADR-0021), so this is a balance of two already-body-coloured terms, not a post-EQ.

**Energy/effort are already plumbed.** `voice/mod.rs` threads `sources.energy` (measured, M2) and
`sources.effort` (player, M2) into `ResonatorStack::process_sample(excitation, energy, effort)`.
`WaveguideResonator::set_energy_drive(energy)` already forwards energy to `String1d::set_tension_drive`
(M4) and `Tube1d::set_steepening_drive` (M5). The balance reads the **same measured-energy** the String
already receives; the contact stage reads **effort** (and/or a static spread control) like the driver does.

**Stability / passivity (M4–M7 discipline; Bilbao *Numerical Sound Synthesis*).** The contact stage only
shapes / redistributes the *injected* excitation (it adds no feedback path), and the balance is a bounded
crossfade of two existing terms, so neither can inject unbounded energy. Keep both **finite-clamped** and
**identity at zero depth** (the M3/M8 regression discipline: a default patch renders unchanged).

## Scope note (read before step 1)

- **Strike-position spread** is spatial → control-rate **geometry** (a new `excitation_spread` reaching
  `waveguide_geometry`). Applies to **String and Tube** (both use `add_symmetric_excitation`).
- **Contact time / slip** are temporal → a **per-sample coupling stage** between the driver and the
  waveguide injection, applied regardless of driver (so the picked-vs-strummed axis works on the `Sample`
  driver too). Applies to **String and Tube**.
- **Source↔body balance** is **String only** (M7's body is String-only).
- **"Slip" is underspecified for the shipped drivers.** Stick-slip is a *bow* mechanism, and M8 shipped
  **pick + reed**, no bow. Do not invent a bow archetype here. Step 1 decides whether a generic
  contact-release/slip parameter ships in M9 or slip is **deferred** to a future bow driver.

---

## Steps

### 1. **[DECISION]** Contact-model voicing, balance curve, parameter seat & scope — **needs your call**
- *(This is the milestone's `[DECISION]` — "Default balance curve and contact-model voicing." It is
  underspecified on the contact stage's architecture and on slip, which set steps 2–6's scope, so this
  step settles all of it.)* No file change; the executor stops here. Resolve:
  - **Which contact dimensions ship:** **strike-position spread** (required — it is the picked-vs-strummed
    mechanism the exit test names) + **contact time** (recommended — turns M8's brightness-only
    `contact_time` into a real coupling) ± **slip** (recommend **defer**: it is bow-specific and no bow
    driver exists; or ship a generic contact-release if you want it now).
  - **Contact-model voicing:** the default + range/taper of the spread control (narrow "pick" ↔ wide
    "strum"; how much of the string length the strum spans) and of contact time; whether the contact stage
    is a **static material control**, an **effort/energy-driven gesture**, or **both** (recommend: a
    material spread control that effort can optionally widen, so harder playing strums wider).
  - **Parameter seat:** spread as a field on **`WaveguideConfig`** (recommended — a material control next
    to `position_of_strike`) vs a new `ContactConfig`; contact stage as a **`resonator_stack/contact.rs`
    coupling submodule** (recommended) vs an extension of `driver.rs`.
  - **Source↔body balance curve:** how normalized measured-energy maps to the pickup↔body weights — the
    **warmth amount at the soft end** (how body-dominant a pp note is), the **directness at the loud end**,
    the **crossfade shape** (recommend an equal-power-ish crossfade re-trimmed to hold output level so it
    is "not gain alone"), and whether balance is a **fixed internal curve** (like the M7 constants) or a
    **defeatable depth control** (recommend a depth control defaulting to a musical amount, `0` = today's
    fixed 1.0/3.0 mix for the regression guard).
  - **Resonator scope:** confirm contact stage on **String + Tube**, balance on **String only**, Mesh/Modal
    excluded.
- **Verify:** none (decision step). Resolution fixes the contact dimensions, the parameter seat/voicing,
  the balance curve, and the resonator scope that steps 2–6 implement.

### 2. Add the contact-stage and balance controls to the patch  [depends on #1]
- **File(s):** `plugins/lamath/src/patch.rs` — the spread (+ contact-time, ± slip) control(s) on the seat
  chosen in #1 (default: `excitation_spread` on `WaveguideConfig`) and a balance control on the String
  (default: a `source_body_balance` depth on `WaveguideConfig`, `0` = today's fixed mix). Incidental: their
  `Default` impls and `#[serde(default = …)]` so existing patches deserialize unchanged.
- **Reference behavior:** mirror the existing `WaveguideConfig` field + `FloatRange` constant pattern
  (`position_of_strike` ↔ `STRIKE_POSITION`, `patch.rs:218`); defaults must reproduce **current behavior**
  (spread → today's `EXCITATION_WIDTH_FRACTION`; balance depth `0` → today's 1.0/3.0 mix), so a default
  patch is unchanged. New `FloatRange`s go in `dsp/constants.rs` next to `STRIKE_POSITION` (`constants.rs:143`).
- **Change:** add the field(s) + their range constants; include them in `WaveguideConfig::default`. No DSP
  wiring yet.
- **Verify:** a patch test asserts `WaveguideConfig::default()` carries the identity defaults (spread =
  current width fraction, balance depth = 0) and that a config carrying non-default values round-trips
  through the existing patch (de)serialization. Red→green: the field(s) do not exist yet (compile-level
  red, greenfield); the `#[serde(default)]` clause makes a pre-M9 patch JSON still deserialize.

### 3. Register the contact-stage and balance parameters  [depends on #2]
- **File(s):** `plugins/lamath/src/parameters/registry.rs`, `.../parameters/paths.rs`,
  `.../parameters/bindings.rs` (+ `.../parameters/helpers.rs` if a read-back mapping is needed). Follow the
  driver-parameter wiring landed in M8 (`registry.rs:478`, `ParameterPath::Driver(...)`,
  `parameters/driver.rs`).
- **Reference behavior:** the parameter registry is the single source of truth (CLAUDE.md). Mirror a
  continuous `WaveguideConfig` parameter's `ParameterInfo::continuous` + `ParameterPath` + `apply`/`runtime`
  wiring; defaults map to the identity values from #2 so existing automation/patches are unchanged.
- **Change:** add the continuous spread (+ contact-time, ± slip) and source↔body-balance parameters, routing
  each `ParameterPath` to the #2 patch field.
- **Verify:** a registry test asserts each new parameter exposes its declared range and that its default
  plain value maps to the #2 identity default. Red→green: the parameter ids / paths do not exist yet
  (compile-level red).

### 4. Strike-position spread → waveguide geometry (picked-vs-strummed)  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/waveguide.rs` (add `excitation_spread` to `WaveguideParams`),
  `dsp/waveguide/core.rs` (`waveguide_geometry` takes the spread; `EXCITATION_WIDTH_FRACTION` becomes the
  *narrow* end of a spread-driven span), and the `from_config` mappers
  (`dsp/voice/resonator_stack/mapping.rs::waveguide_params_from_config`).
- **Reference behavior:** re-derive from `core.rs::waveguide_geometry` (`core.rs:179`) and
  `traveling.rs::add_symmetric_excitation` (`traveling.rs:86`): a wider tap span is a stronger low-pass
  comb on the injected excitation, so a strum (wide) excites fewer high partials than a pick (narrow), at
  equal injected energy. The span is part of the **prepared model** (cached behind the params dirty-check),
  so widening it stays control-rate and allocation-free. Keep taps clamped within `STRIKE_POSITION` (as
  today) so a wide spread near an end stays in-bounds.
- **Change:** thread `excitation_spread` from config → `WaveguideParams` → `waveguide_geometry`, scaling the
  half-width from `EXCITATION_WIDTH_FRACTION` (narrow) up to the step-1 strum width; at the identity default
  the width equals today's fixed value. *(If #1 chose effort-widening, also fold the per-sample/effort term
  in here at control rate.)*
- **Verify:** an objective **picked-vs-strummed** test renders a String at narrow spread vs wide spread with
  matched injected energy; gain-normalize both outputs, then assert their spectral balance differs by a
  measurable margin (high-frequency ratio / `spectral_centroid_hz` drops for the wide strum) — i.e. distinct
  timbre, not gain. Add `assert_no_allocations` over the geometry recompute + a steady-state render.
  Red→green: today the width is fixed, so narrow and wide render identically (the difference assertion
  fails) and `excitation_spread` does not exist (compile red); green = the two render distinctly.

### 5. Contact-time / slip temporal coupling stage  [depends on #1, #2, #4]
- **File(s):** `plugins/lamath/src/dsp/voice/resonator_stack/contact.rs` (new coupling submodule on the seat
  from #1, `mod contact;` in `resonator_stack.rs`) applied to the **driven** sample before
  `waveguide.process_sample`, inside the existing 2x closure in
  `ResonatorEngine::process_sample`. Fixed-size state sized at construction (ADR-0001).
- **Reference behavior:** the contact stage shapes the *temporal* coupling of the driven excitation: a
  longer contact time spreads the momentum transfer over more (oversampled) samples, lowering the onset's
  high-frequency content (distinct from #4's spatial comb and from level); slip (if shipped per #1) is a
  bounded contact release. Re-derive that this is a feed-forward shaping of the injected excitation only —
  no new feedback path — so it stays passive/bounded (M4–M7 discipline). It applies regardless of driver
  (so it works on the `Sample` pass-through driver, where M8's `PickConfig::contact_time` does nothing).
- **Change:** add the contact stage and run it on the driven sample inside the 2x closure; at the identity
  default (contact time = today's value, no slip) it is a pass-through so the M8 render is unchanged.
- **Verify:** an objective **contact-time** test on the `Sample` driver renders short vs long contact time;
  gain-normalize, then assert the **attack-window** spectral centroid drops measurably for the longer
  contact (and that it differs from the #4 spread axis — contact changes the onset, spread changes the
  steady comb). Identity guard: at the default contact value the render equals the pre-M9 path (RMS +
  sample-wise within tolerance). Add `assert_no_allocations` over the stage. Red→green: the contact stage
  does not exist (compile red) and the `Sample` driver shows no contact-time effect today (behavior red).

### 6. Energy-dependent source↔body balance on the String output  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs` — replace the constant `STRING_PICKUP_MIX` /
  `STRING_BODY_MIX` blend (`string_1d.rs:227`) with an energy-driven, level-preserving crossfade; add a
  smoothed balance follower fed by the measured energy the String already receives via
  `set_tension_drive`/`set_energy_drive` (or a new `set_balance_drive` forwarded in `waveguide.rs::set_energy_drive`).
- **Reference behavior:** re-derive from ADR-0021 — both terms are already body-loaded (the pickup reads the
  two-way-loaded loop), so this balances **source vs body voice**, not a post-EQ. Low measured energy →
  body-weighted (warm); high → pickup-weighted (direct). Hold output level across the crossfade (the
  step-1 curve, re-trimmed like `STRING_OUTPUT_TRIM`) so loud/soft differ in **timbre, not gain**, and loud
  plucks keep the M7 headroom. Smooth the balance weight per sample (a one-pole, like `ScalarSmoother`) so a
  dynamic swell does not zipper. At balance depth `0` (#2) the weights collapse to today's fixed 1.0/3.0.
- **Change:** derive a smoothed normalized energy → pickup/body weights per the #1 curve; apply the
  level-preserving blend; gate by the balance depth so `0` reproduces the fixed mix.
- **Verify:** an objective **hard-vs-soft balance** test: at a **fixed** energy, balance-on vs balance-off
  shifts the body-band/pickup spectral balance (isolating the mix from M4 tension); and across low→high
  energy with balance-on, the body-energy fraction decreases **monotonically** — both gain-normalized so it
  is not level. Regression guard: balance depth `0` renders identically to the pre-M9 String (RMS +
  per-harmonic-decay within tolerance). Add `assert_no_allocations` over the per-sample balance. Red→green:
  the mix is constant today, so fixed-energy on/off and low-vs-high give the same spectral balance (the
  assertions fail) and the balance state does not exist (compile red).

### 7. Exit gate + doc/ADR surface  [depends on #4, #5, #6]
- **File(s):** verification only, plus the doc surface: `CHANGELOG.md`, `docs/architecture.md` + the AGENTS
  code-map current-state, and a new ADR for the lasting M9 decisions (next free number is **ADR-0027**;
  delegate to the **repo-docs** skill for the ADR + index wiring, as M7 did with ADR-0021).
- **Reference behavior:** M9 exit — "`make ci` green; picked-vs-strummed and hard-vs-soft produce distinct,
  measurable timbres via contact + balance (not gain alone); no-alloc." Per-milestone CPU via `make bench`
  at the 1–4 voice budget. The ADR records the resolved step-1 decisions (contact-model voicing, balance
  curve) — the only durable home for those trade-offs (repo-docs convention; mirrors ADR-0021).
- **Change:** one curated `CHANGELOG.md` line (the waveguide gains a picked↔strummed contact stage and an
  energy-dependent source↔body balance — warm soft, direct loud); update `architecture.md` / AGENTS
  code-map if the contact stage adds a module (`resonator_stack/contact.rs`); write ADR-0027.
- **Verify:** run `make ci` (green) and show output; the #4/#5/#6 objective + identity tests satisfy the
  distinct-timbre and regression clauses; the #4/#5/#6 `assert_no_allocations` cover no-alloc; `make bench`
  shows the per-voice cost within the 1–4 voice budget.

---

## Decisions to resolve at execution (step 1)

| Facet | Decision you own | Recommended default |
| ----- | ---------------- | ------------------- |
| Contact dimensions | Which of strike-position spread / contact time / slip ship in M9. | Spread + contact time; **defer slip** (no bow driver). |
| Contact voicing | Spread/contact-time range & taper; static control vs effort-driven gesture vs both. | Material spread control, optionally widened by effort. |
| Parameter seat | Spread on `WaveguideConfig` vs new `ContactConfig`; contact stage as `contact.rs` submodule vs `driver.rs` extension. | `WaveguideConfig` field + `resonator_stack/contact.rs`. |
| Balance curve | Energy→pickup/body mapping; warmth at soft end; crossfade shape; fixed internal vs defeatable depth. | Equal-power-ish, level-trimmed; defeatable depth, `0` = today's 1.0/3.0. |
| Resonator scope | Confirm contact on String+Tube, balance String-only, Mesh/Modal excluded. | As stated. |

---

To execute, run the EXECUTE-PHASE companion prompt on this file; the executor stops at step 1's `[DECISION]`.
