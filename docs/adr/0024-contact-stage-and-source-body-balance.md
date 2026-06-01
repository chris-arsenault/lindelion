# ADR-0024: Coupling/Contact Stage and Source↔Body Balance

## Status

Accepted

## Date

2026-05-31

## Context

Milestone M9 of the dynamic-response program ([ADR-0014](0014-dynamic-response-effort-energy-bus.md))
adds the *gesture interface* between the driver and the resonator and the *dynamic balance between
subsystems*: a coupling/contact stage (strike-position spread, contact time) and an energy-dependent
source↔body balance, so picked-vs-strummed and soft-vs-loud read as distinct timbres rather than
levels.

Several decisions surfaced while building it that are lasting and have no existing ADR home (M4–M6
implemented [ADR-0014]/[ADR-0016] with no new architecture; the body output blend got
[ADR-0021](0021-string-two-way-body-coupling-and-output-blend.md)):

1. Which contact dimensions ship, and where the stage lives.
2. How the spatial spread reaches the resonator without busting the M1 prepared-model cache.
3. What "strike-position spread" actually does to the spectrum (it deviates from the plan's
   predicted direction).
4. How an energy-dependent source↔body balance holds output level given a very quiet body radiation.

## Decision

- **Contact dimensions: strike-position spread + contact time; slip deferred.** Stick-slip is a bow
  mechanism and M8 shipped pick + reed, no bow, so a "slip" control would be a placeholder. It is
  deferred to a future bow driver rather than invented here.
- **A dedicated `ContactConfig` and a `resonator_stack/contact.rs` coupling submodule** sit between
  the driver and the waveguide injection, inside the 2x oversampled loop ([ADR-0016](0016-oversampled-nonlinear-inner-loop.md)).
  The stage is **feed-forward** (it only shapes the injected excitation; it adds no feedback path),
  so it stays passive and bounded. The default config is a transparent pass-through, so a default
  patch renders exactly as before M9.
- **Spread is a material control widened by playing effort** ("control + effort widens"):
  `effective_spread = spread · (1 + effort · widen)`. At a `0` spread control the product stays `0`,
  so effort never widens a patch that asked for a tight pick — preserving the identity default.
- **Strike-position spread is spatial → control-rate geometry, kept out of the prepared-model cache
  key.** The widened three-tap excitation window is rebuilt at injection from the (cheap) strike +
  half-width; the heavy M1 derivations (loop damping, dispersion, delay tuning) are untouched. The
  spread therefore lives on `WaveguideParams` but is excluded from the String's `String1dParams`
  cache key and stripped from the Tube's `WaveguideParams` cache key, so a per-sample (effort-driven)
  spread change never busts the cache.
- **Spread averages the strike-position comb (it does not low-pass).** Spreading the contact across a
  range of positions fills in the strike-position notches, producing a flatter, *brighter* spectrum —
  the opposite of the M9 plan's predicted "wide strum is darker." This is physically correct (a wider
  excitation region averages the position-dependent notch pattern). The mellow, washy character of a
  strum is carried by the **contact time** (a one-pole low-pass on the onset), not the spread. The
  two dimensions are genuinely distinct: spatial comb-averaging versus temporal low-pass.
- **Source↔body balance is an equal-power crossfade between the pickup tap and a level-matched body,
  steered by measured energy, defeatable by depth.** Soft playing leans to the warm body, loud
  playing to the direct pickup. Because the body radiation is ~15–20× quieter than the pickup tap
  ([ADR-0021]), the body is scaled up by a level-match makeup gain *before* the equal-power crossfade —
  otherwise leaning to the body would just go quiet instead of warm. Equal power on the level-matched
  signals holds output level, so soft and loud differ in character, not gain. At depth `0` the weights
  collapse to the pre-M9 fixed `(1.0, 3.0)` blend (a bit-exact identity guard). The balance is String
  only (the Tube has no reduced body).

## Consequences

- The contact stage and balance default to transparent/`0`, so existing patches, the default patch,
  Modal, and Mesh are all unchanged.
- The spread couples into the resonator through `WaveguideParams::excitation_spread` but is invariant
  to the heavy cache, so an effort-driven strum stays allocation-free and control-rate-cheap.
- The body level-match makeup gain and the balance crossfade span/reference are first-principles
  values. The exact body/pickup level ratio is the same cross-resonator level / coupling-strength
  work deferred to the M11 calibration pass ([ADR-0021]); M9 sets defensible defaults to be
  re-confirmed once every subsystem can be measured together.
- Pick and reed drivers (M8) carry their own `contact_time`; the new contact stage's contact time is
  the generic coupling that also applies to the sample/sidechain driver, where the M8 control does
  nothing. The mild overlap when a Pick driver is selected is acceptable (different layers).

## Alternatives

- **Equal-power crossfade on the raw pickup/body weights (no level match).** Rejected: because the
  body radiation is ~20× quieter, leaning to the body collapsed the output level (measured ~17×
  quieter at the soft end) — a fader, not a timbral balance. Level-matching the body before the
  crossfade fixes it.
- **Model spread as a spatial low-pass (a genuinely darker wide pluck).** Rejected as the spread
  mechanism: it duplicates the contact-time low-pass and is not what spreading the *position* does.
  The comb-averaging spread plus the contact-time low-pass together give a faithful picked↔strummed
  axis.
- **Ship a generic "slip" control now.** Rejected: stick-slip is bow-specific and no bow driver
  exists; a placeholder would be invented scope. Deferred.
- **Put the contact controls on `WaveguideConfig` instead of a dedicated `ContactConfig`.** A
  reasonable alternative; a dedicated config groups the gesture controls and keeps the contact stage's
  ownership explicit.
