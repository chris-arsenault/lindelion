# Lamath Mini Symphony — Implementation Plan

A five-movement mini symphony (~13–14 min) for the Lamath ensemble, built on the existing
`lamath-song` bin. Movement I is the audition-approved Olórelindë render, untouched. Four new
movements follow — a fast battle theme, a lament adagio with an operatic-style lead, a dark
waltz, and a slower triumphant final-boss closer — all derived from Olórelindë's material so
the symphony reads as one piece. Style targets: orchestral electronica and Final Fantasy / JRPG
scoring. Out of scope: realtime/plugin code, the render catalog, new DSP in the instrument
crates (only new *patches* of existing voices).

## Confirmed decisions

- **Movement I is Olórelindë verbatim.** Its score data is read-only; Movement II is composed
  to pick up from its B-minor ending. No bridge, no seed edits.
- **Total duration ~12–15 minutes**: Movements II–V each ~2.5–3.5 min, with room for real
  forms (battle B-section + rideout, full passacaglia cycles, waltz with trio, multi-section
  finale).
- **Gaps + combined render**: each movement is a self-contained WAV (own mix, own ring-out
  tail) plus a combined full-symphony WAV with ~1.5 s of silence between movements.
- **Audition is the gate for all new music** (explicit user-granted exception to the
  automated-exits default, consistent with all prior Lamath sound work): a movement is done
  when the user approves its render by ear, not when metrics pass.

## The shared DNA (cyclic material)

Extracted from the real Movement I score (`composition.rs`):

| Element | Source | Definition |
| ------- | ------ | ---------- |
| **Ascent motto** (the idée fixe) | `LEAD_A` bars 5–6 (`composition.rs:416-422`) | Scale degrees 1–2–3 rising (rhythm long–short–long: 1.5+0.5+2 beats), answered by the 3–4–3–1 falling arch |
| **Dream arpeggio** | `intro_arp` (`composition.rs:366-375`) | Rising tonic-minor arpeggio + octave (1–♭3–5–8), the "dream ascending" gesture |
| **The Lift** | bar-25 modulation (`composition.rs:211`) | The +2-semitone structural modulation; becomes the symphony's fingerprint, paid off at full scale in Movement V |

Every movement presents at least one *transformation* of the motto (the canonical cyclic
practice — re-meter, re-mode, augment/diminish, re-harmonize; verbatim return is reserved for
the apotheosis). Models: Berlioz Symphonie fantastique (idée fixe recast as the 2nd-movement
waltz), Tchaikovsky 5 (minor motto → major apotheosis), Liszt thematic transformation, Uematsu's
Dancing Mad leitmotif weave.

## Key & tempo narrative

Every key joint is motivated; the whole arc is Am → B major (the Lift writ large, minor→major):

| Mvt | Key | Tempo / meter | Link to neighbor |
| --- | --- | ------------- | ---------------- |
| I | A minor → B minor | 96 BPM, 4/4 | (exists) |
| II | B minor (B-section in D major) | ~168 BPM, 4/4 with 7/8 phrase-end bars | Picks up I's B-minor ending |
| III | D minor, ends deceptively on B♭ | ~58 BPM, 4/4 | Mediant drop from B minor; B♭ ending baits IV |
| IV | G minor (trio in B♭ major); reprise lifted to B minor | ~150 BPM quarter, 3/4 (one-in-a-bar feel ~50 bars/min) | Relative of III's B♭ ending; reprise echoes B minor |
| V | A minor (villain) → B major (apotheosis) | ~116 BPM, 4/4, broadening coda | Home key returns; final Lift +2 with Picardy = the symphony's payoff |

## Movement designs (milestone altitude)

### II — Battle (B minor, ~168 BPM, ~125 bars ≈ 3 min)

FF battle themes run ~160–177 BPM; the slower-heavier register is reserved for V.

- **Engine**: driving pick-bass ostinato (the FF signature opening) + sixteenth "sequencer"
  ostinato on modal keys built from the Dream arpeggio — the orchestral-electronica layer.
  Written-in sidechain-style pumping: bowed-pad envelopes duck on each low-mesh hit.
- **A theme**: the Ascent motto in diminution as the main riff; every 4th bar is 7/8 (dropped
  eighth) — the Uematsu prog-meter trick.
- **B-section**: Royal Road progression (IVM7–V7–iii7–vi → GM7–A7–F♯m7–Bm) in D major, the
  J-pop/JRPG lyrical lift.
- **Episode**: Phrygian-dominant/Hijaz statement of the motto (the first Arabian color seed).
- **Shock**: one semitone-shift restatement of the riff (+1, C minor) before the rideout.
- **Rideout**: Aeolian bVI–bVII–i cadence riff (G–A–Bm).
- **New patches** (added this phase, auditioned in-movement): harder/brighter battle tube; a
  low damped "tom" mesh voicing for the drive layer; tight-ride variant if needed.

### III — Adagio / lament (D minor, ~58 BPM, ~40 bars ≈ 2.8 min)

Lineage: Dido's Lament + Aria di Mezzo Carattere → Aerith's Theme.

- **Ground**: lament-bass passacaglia — descending chromatic tetrachord (D–C♯–C–B–B♭–A) on
  the pick bass, ~10 four-bar cycles; Andalusian-cadence harmonization (Dm–C–B♭–A) ties the
  ground to flamenco/maqam practice in one progression.
- **Maqam color**: one ground cycle harmonized with the Hijaz tetrachord on the dominant
  (A–B♭–C♯–D) — 12-TET-safe (Hijaz/Phrygian dominant/double harmonic survive equal
  temperament; Bayati does not).
- **Operatic lead** (tube): written as if for soprano — range ~C4–A5, mostly stepwise,
  expressive 6th leaps recovered by step, appoggiatura/suspension on nearly every strong beat,
  written breaths (rests), one single peak note placed ~2/3 through. This is the **augmented
  motto**: 1–2–3 stretched ×4 over the ground.
- **Climax surprise**: an omnibus progression (contrary-motion chromatic wedge) — it extends
  the lament-bass family, so it sounds inevitable rather than pasted.
- **Ending**: Picardy third set up, then *withheld* — deceptive resolution to B♭ (bVI). The
  real major-mode payoff waits for Movement V.

### IV — Waltz (G minor, 3/4, ~150 BPM quarter, ~125 bars ≈ 2.5 min)

Models: Sibelius Valse triste, Shostakovich Waltz No. 2; Berlioz "Un bal" for the motto.

- **Opening**: rhythm first — oom-pah-pah alone (pick bass on 1, modal keys/short bow chords
  on 2 & 3) before melody enters (the Valse-triste move).
- **Waltz tune**: the Ascent motto re-metered into 3/4 with grace-note/turn decorations —
  Berlioz's exact transformation of the idée fixe.
- **Craft**: hemiola cadences (two 3/4 bars grouped as three 2-beat units); written-in rubato
  illusion at fixed BPM (melody entering an eighth late, tied anticipations, accompaniment
  dropping out for a beat before cadences).
- **Trio**: B♭ major (relative), Dream arpeggio as the trio's flowing accompaniment.
- **Surprise**: final reprise lifted via common-tone diminished 7th to **B minor** — a
  chromatic-mediant slide (shares D) that foreshadows V's B-major apotheosis and echoes
  Movement I's ending.

### V — Finale: boss → apotheosis (A minor → B major, ~116 BPM, ~100 bars ≈ 3.5 min)

Final-boss craft: *slower and heavier than the battle theme*, menace from weight — slow
harmonic rhythm under a relentless sixteenth ostinato surface (Dancing Mad's multi-section
organ-prog shape is the model).

- **Section 1 — menace** (A minor): low mesh + bass pedal, hexatonic-pole oscillation
  (A minor ↔ D♭ major — no common tones, the "uncanny" chromatic mediant), motto fragments.
- **Section 2 — toccata**: rapid modal-keys figuration (the organ-prog role), the motto
  re-intervaled through double harmonic / Hijaz Kar (one step bent to an augmented 2nd —
  contour preserved, color transformed).
- **Section 3 — recall episode** (quiet): the adagio theme returns, consoling, over the waltz
  rhythm in augmentation — the cyclic gathering before the end (Franck D-minor finale practice).
- **Section 4 — apotheosis** (B major): the motto verbatim-contour, fortissimo, major mode —
  arrived at by the final Lift (+2 from A) *and* the Picardy third withheld in III, both paid
  at once. bVI–bVII–I arrival cadence (G–A–B), Mixolydian/plagal coda over a tonic pedal,
  one Lydian ♯4 inflection on the last melodic statement, broadening tempo feel via written
  augmentation. Full ensemble; final crash + rolled chord + ring-out tail.

## Context / reuse map

| What | Where | State |
| ---- | ----- | ----- |
| Score engine (`Part`, `Chord`, bar/beat addressing, per-section transpose) | `plugins/lamath/src/bin/lamath-song/score.rs` | Reuse; **refactor**: `BPM`/`BEATS_PER_BAR` are global consts (`score.rs:4-5`) — thread a per-movement `(bpm, beats_per_bar, start_offset)` context for tempo variety and the 3/4 waltz |
| Movement I score data | `composition.rs` (574 lines) | **Read-only**; file splits into shared types + per-movement modules (600-line lint headroom is 26 lines) |
| Instrument voices/patches | `render.rs:60-151` (8 patches, 10 tracks) | Reuse; new movement colors = new `Voice` variants + patch fns (~3–30 lines each), added within each movement's phase |
| Mixer (active-RMS/peak leveling, constant-power pan, bar-addressed fader rides, master normalize) | `main.rs:42-250` | Reuse; rides/levels become movement-local (each movement defines its own `TrackSpec`s) |
| Render pipeline (48 kHz, 512-block, stems + mix + MIDI) | `main.rs`, `render.rs` | Reuse; extend to per-movement outputs + combined WAV with 1.5 s gaps |
| Polyphony pattern (chords = parallel mono instances) | `composition.rs:79-95` | Reuse as-is (tube/bow are one-voice models) |
| Harness | `make render-lamath-song`, `make compress-review-audio` | Reuse; add single-movement filter (`MOVEMENT=`) so iteration never re-renders the whole symphony |
| Research brief (techniques, citations) | This plan §Movement designs; full cited brief produced during planning | Source of named devices; each device above is grounded in it |

## Cross-cutting constraints

- **Movement I is regression-locked**: after the M0 framework refactor, Movement I's stems
  must be sample-identical to the pre-refactor render (hash compare). Any diff fails the gate.
- **Audition is the only gate for sound.** Every movement phase ends with `make ci` green +
  a movement render + preview compression, then **stops for the user's ear**. No metric
  substitutes. Renders go to `review/lamath-song/` with compressed previews.
- **Use the harnesses**: renders only via `make render-lamath-song` (with `MOVEMENT=` once it
  exists); never render the full symphony to iterate one movement; debug profile, `./target`.
- **File-size lint (600 lines)**: composition becomes `movements/` modules
  (`olorelinde.rs` move-only, `battle.rs`, `adagio.rs`, `waltz.rs`, `finale.rs` + shared
  types); no module may approach the limit with headroom under ~10%.
- **`make ci` unit rules**: any new tests are in-memory and fast (timeline math, movement
  bounds, notes-nonempty data checks); no file I/O, threads, or wall-clock in units. The bin
  is offline — no realtime/no-alloc constraints apply to it.
- **No silent movements**: every movement's render must be audible end-to-end; a movement
  section that renders silence is a defect.
- **Docs/changelog at the end** via repo-docs conventions: `docs/lamath-song.md` rewritten to
  current state, one CHANGELOG line under a real version heading + `Cargo.toml` bump.

## Milestones

### M0 — Movement framework

Make the song bin multi-movement with zero audible change to Movement I.

- Thread per-movement `(bpm, beats_per_bar)` + start-offset context through `score.rs`
  (replacing the global consts) and the frame math in `composition.rs`/`main.rs`.
- Split composition into `movements/` modules; move Olórelindë data verbatim into
  `movements/olorelinde.rs`; shared types (Chord/MixSpec/TrackSpec/helpers) stay in one place.
- Per-movement render outputs (`review/lamath-song/<movement>/…` mix + stems + MIDI) plus the
  combined symphony WAV with 1.5 s inter-movement gaps; movement loudness aligned by
  active-RMS before the final master normalize.
- `make render-lamath-song MOVEMENT=<name>` single-movement filter.
- Cheap unit tests: movement offset/3/4 bar math, gap insertion, Movement I note-data
  equivalence.
- Exit: `make ci` green; Movement I stems sample-identical to a pre-refactor reference render;
  combined WAV is Movement I + correct tail/gap handling.

### M1 — Movement II: Battle [depends on M0]

Compose and render the battle movement per its design above (incl. its new patches: battle
tube, tom mesh).

- Exit: `make ci` green; movement render + compressed preview delivered;
  **[DECISION] user audition approves the battle movement by ear.**

### M2 — Movement III: Adagio [depends on M0]

Compose and render the lament per its design above (lament-bass passacaglia, operatic tube
line, Hijaz cycle, omnibus climax, withheld Picardy).

- Exit: `make ci` green; render + preview;
  **[DECISION] user audition approves the adagio by ear.**

### M3 — Movement IV: Waltz [depends on M0]

Compose and render the waltz per its design above (3/4 context exercised for real; oom-pah-pah
voicing, hemiola cadences, ct°7 reprise lift).

- Exit: `make ci` green; render + preview;
  **[DECISION] user audition approves the waltz by ear.**

### M4 — Movement V: Finale [depends on M0; benefits from M1–M3 themes being settled]

Compose and render the boss→apotheosis finale per its design above. Composed last by design:
its recall episode quotes II–IV's settled themes.

- Exit: `make ci` green; render + preview;
  **[DECISION] user audition approves the finale by ear.**

### M5 — Full-symphony assembly [depends on M1–M4]

The whole piece as one listen.

- Combined render with gaps; cross-movement loudness consistency pass; per-movement +
  combined MIDI export; any inter-movement mix rides the full listen reveals.
- Exit: `make ci` green; full-symphony WAV + preview;
  **[DECISION] user audition approves the complete symphony front-to-back.**

### M6 — Title, docs, release [depends on M5]

- **[DECISION] symphony title** (Quenya/Sindarin naming options proposed, e.g. built on
  *lindalë*/“music” with Olórelindë as Movement I's title) + movement titles + cover prompt.
- `docs/lamath-song.md` rewritten to describe the symphony (repo-docs conventions);
  CHANGELOG entry under a version heading; `Cargo.toml` version bump; archive pass for
  `review/lamath-song/` versions.
- Exit: `make ci` green; docs current-state-accurate; changelog/version consistent.

## Decisions needing your input

| Where | Decision you own |
| ----- | ---------------- |
| M1 exit | Audition verdict: battle movement |
| M2 exit | Audition verdict: adagio |
| M3 exit | Audition verdict: waltz |
| M4 exit | Audition verdict: finale |
| M5 exit | Audition verdict: full symphony |
| M6 | Symphony + movement titles, cover prompt |

Everything else (module layout, patch parameter values pre-audition, exact bar counts, voicing
details) is settled by the executor within the constraints above. This plan is the single
source of truth; expand one phase at a time with `plan-phase` before executing it.
