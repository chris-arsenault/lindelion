# Multi-Plugin UI Redesign

This document is the target design for the Vizia plugin surfaces. It starts from the
ideal workflow for each plugin, then maps that workflow back to the parameters and
host commands that already exist in the repository.

References used:

- Kilohearts Phase Plant UI documentation: https://kilohearts.com/docs/phase_plant
- Kilohearts modulation documentation: https://kilohearts.com/docs/modulation
- Xfer Serum 2 knob/slider interaction documentation: https://xferrecords.com/web-manual/serum-2/using-knobs-and-sliders

## Shared Design Principles

The plugin should read as one dense instrument surface, not a settings form. Every
plugin gets these layers:

1. Identity/status strip: patch name, primary state, file/tool commands.
2. Primary radiator: waveform, piano roll, resonator scope, pads, meters, or slice map.
3. Performance controls: compact knobs, segmented toggles, and small icon tools.
4. Edit detail: the selected object gets the most precise controls.
5. Utility rows: export, save/load, detection, sync, or library actions.

Large block buttons are reserved for the primary musical action only. Most commands
are small icon or text+icon tool buttons. Dropdowns are avoided unless the option
set is too large to keep visible. Short option sets are segmented controls.

## Control Mapping

| Parameter or action type | Control type | Reason |
| --- | --- | --- |
| Level, gain, pan, saturation, amount | Rotary knob | Dense, immediate, and suitable for repeated adjustment. |
| Frequency or time range | Rotary knob with value text | The exact value matters, but a long linear slider wastes space. |
| Start/end/range boundaries tied to waveform | Horizontal range lane or compact nudge row | Boundary editing should sit beside the visual object it edits. |
| Bipolar tuning, pan, modulation amount | Centered knob | The center/default position is musically meaningful. |
| Two-state values | Segmented toggle | Both states remain visible. |
| Three to six options | Segmented row | Faster than a dropdown and scannable. |
| Seven or more options | Compact selector row | Keeps density while avoiding oversized segmented controls. |
| Momentary commands | Small tool button | Commands should not dominate parameter editing. |
| Drag/drop targets | Visible drop zone on the content surface | The drop affordance belongs on the audio object, not in a separate form row. |
| Status | Colored chip and small metric | Status should be readable without consuming control space. |

Shared interaction contract:

- Drag or wheel adjusts knobs and sliders.
- Shift-drag is fine adjustment.
- Default value is visible through center/default tick styling.
- Double-click reset is the target behavior for shared controls.
- Hover surfaces show tooltips with units, default, and exact value where Vizia allows it.
- Host automation and patch persistence remain the only value surface for parameters.

## Color System

The UI uses a neutral graphite base plus role colors. The goal is not a two-tone
flat surface; color should say what kind of information the user is reading.

| Role | Colors | Use |
| --- | --- | --- |
| Base | `#111517`, `#181e20`, `#222a2d` | Window, panels, inactive tracks. |
| Text | `#f1f5f2`, `#aeb8b2`, `#6f7c76` | Primary, secondary, quiet labels. |
| Audio/source | `#59b6d8`, `#94d7ea` | Waveforms, capture, source-loaded states. |
| Synthesis/tone | `#7ed06d`, `#c0eb88` | Resonators, filter, output, tone controls. |
| Slicing/pads | `#f2a84b`, `#f6d36d` | Linnod slices, selected pads, marker focus. |
| Modulation/routing | `#9a78ff`, `#d4c4ff` | Routing, modulation, quantize relationships. |
| Transport/export | `#ef6f88`, `#ffadb8` | Record, export, destructive or high-attention actions. |
| Warning/error | `#d7a540`, `#e05f5f` | Missing source, failed analysis, invalid state. |

Panels are not nested cards. Sections are separated by gutters, hairline borders,
small headers, and local color accents.

## Lamath ASCII Design

Lamath is a resonator instrument. The largest visual surface should show how the
excitation feeds the two resonator lanes and output stage.

```text
+----------------------------------------------------------------------------------+
| LAMATH   Patch Name                         [save] [load] [export]  voices  in/out|
+----------------------------------------------------------------------------------+
| Source / Excite       | Resonator Stack                                  | Output |
| [midi|audio|side]     | +----------------------+ +--------------------+ | gain  |
| [expression off/on]   | | A  modal/wave  scope | | B  modal/wave scope| | pan   |
| live latch controls   | | preset bright decay  | | filter loop drive  | | sat   |
| audio note detection  | | style reflect        | | style reflect      | | meter |
| pitch/pressure ranges | +----------------------+ +--------------------+ | filter|
|                       | routing [parallel|series] retrigger [carry|retrig]        |
+-----------------------+--------------------------------------------------+--------+
| Library / Slots       | Envelope + Modulation                                      |
| slot 1  waveform      | amp attack release | lfo rate shape | mod source target amt |
| slot 2  waveform      | compact controls and active modulation color indicators     |
| slot 3  waveform      |                                                            |
| slot 4  waveform      |                                                            |
+----------------------------------------------------------------------------------+
```

Lamath radiators:

- Resonator A/B scopes show energy and timbre.
- Output meter stays visible at the right edge.
- Slot waveforms show whether each excitation slot is sample-backed and looping.
- Audio-expression status uses chips for sidechain required/detected/active.

Lamath parameters:

- Master, Pan, Saturation: knobs.
- Cutoff, Resonance: compact filter row with knob/value readout.
- Routing, Retrigger, Resonator Model/Style, Audio Input, Expression Enable,
  Live Excitation Mode: segmented controls.
- Resonator brightness/decay/reflect/loop/filter/drive: per-resonator knobs.
- Amp Attack/Release, LFO Rate/Shape, Mod 1 Source/Target/Amount: compact bank.
- Audio-expression and note-detection thresholds: small knob rows grouped by task.

## Glirdir ASCII Design

Glirdir is a capture-to-MIDI scratchpad. The center should always show the audio
capture and MIDI analysis side by side, while left/right sections control capture
and export.

```text
+------------------------------------------------------------------------------+
| GLIRDIR   Scratchpad                    [arm] [clear] [save] [export midi]     |
| status: captured/analyzing/ready        loop  live-edit  drag-midi-ready       |
+------------------------------------------------------------------------------+
| Capture              | Preview / Analysis                                      |
| bars [1|2|4]         | +----------------------------------------------------+ |
| sync [free|host|bar] | | waveform with detected note/onset overlays          | |
| count [0|1|2]        | +----------------------------------------------------+ |
|                      | +----------------------------------------------------+ |
| Detection            | | piano roll notes, velocity tint, grid ticks         | |
| confidence knob      | +----------------------------------------------------+ |
| onset knob           |                                                        |
| min-note knob        |                                                        |
+----------------------+--------------------------------------------------------+
| Quantize / Key                         | Audition / Export                      |
| key selector scale selector            | volume knob  [play] [stop] [loop]       |
| snap segmented  grid selector          | drag status chip  library status        |
| strength knob  velocity knob           |                                          |
+------------------------------------------------------------------------------+
```

Glirdir radiators:

- Capture state chips: idle, armed, count-in, capturing, captured.
- Analysis state chips: pending, analyzing, ready, error.
- Waveform preview shows input energy and onset placement.
- Piano roll shows pitch, length, velocity, and grid alignment.
- MIDI drag status is shown next to export commands, not hidden in a button label.

Glirdir parameters:

- Capture Bars, Sync Mode, Count-In, Snap: segmented controls.
- Root, Scale, Grid: compact selector rows.
- Confidence, Onset, Min Note, Timing Strength, Velocity Amount, Audition Volume:
  knobs with value text.
- Arm, Clear, Finalize, Play, Stop, Loop, Live Edit, Save, Export MIDI: small tool
  buttons with status chips.

## Linnod ASCII Design

Linnod is a melodic slicer. The source waveform and 4x4 pads are the primary
surface. The selected slice editor sits adjacent to the pad grid so manual edits
have visible feedback.

```text
+----------------------------------------------------------------------------------+
| LINNOD   Patch Name                 [load source] [save] [load] [export]  meter   |
| source: loaded/analyzing/ready      slices 0-16  trigger [pad|chromatic] voices   |
+----------------------------------------------------------------------------------+
| Source + Detection                                 | Pad Matrix                   |
| +------------------------------------------------+ | +-----+ +-----+ +-----+ +---+|
| | waveform, slice markers, selected region       | | |  1  | |  2  | |  3  | ...||
| | drag/drop file target on empty source          | | | name| |name | |name |     ||
| +------------------------------------------------+ | +-----+ +-----+ +-----+ +---+|
| algorithm [super|complex|sparse|pitch|energy|grid]| pad color: selected, choke,  |
| sensitivity knob min-len knob window selector     | active voice, empty/assigned  |
| flux detail knobs, grid controls, [redetect]      | choke [none|1..16]            |
+---------------------------------------------------+------------------------------+
| Selected Slice                                                                       |
| mini waveform start/end handles | pitch semi/cents | gain | pan | cutoff | reverse |
| playback [one-shot|gated|looped] | tune selected | tune all | snap all              |
+----------------------------------------------------------------------------------+
```

Linnod radiators:

- Source waveform shows loaded audio, markers, selected slice region, and
  drag/drop target state.
- Detection chips show algorithm, marker count, source status, and analysis status.
- Pad cells show slice number, abbreviated name, selected state, choke group, and
  MIDI note.
- Selected slice panel shows start/end offsets, pitch, detected F0/deviation, gain,
  pan, playback mode, reverse, and filter.
- Output meter and active voices are always visible in the top strip.

Linnod parameters and patch fields:

- Host parameters: Master Gain, Detection Sensitivity, Tuning Reference are knobs.
- Detection patch fields: algorithm is segmented; min slice, lookback, filter
  radius, group delay, pitch threshold/duration, energy frame, grid divisions, and
  grid offset are compact knobs or selectors according to range.
- Slice fields: start/end offsets are boundary controls near the waveform; pitch,
  gain, pan, and cutoff are knobs; reverse is a toggle; playback mode is segmented.
- Pad fields: choke group is a compact selector on the selected pad detail surface.
- Redetect, tune selected, tune all, snap all, load source, patch file commands:
  small tool buttons with status feedback.

## Section Implementation Model

The shared UI layer should provide section primitives and controls only:

- Shell, top strip, panel, section header, status chip, metric pair.
- Dense knob cell, inline knob row, range row, segmented row, compact selector row.
- Small icon/text tool buttons with tooltip styling.
- Shared CSS tokens for role accents, typography, gutters, and focus/hover states.

Plugin-local files own domain policy:

- Which commands a button sends.
- Which parameter binding is rendered in which section.
- Which patch summary fields become status chips or labels.
- Which waveform/piano-roll/pad drawing view is used.
- Which detection, slice, or pad edit event is emitted.

## Composition Plan

1. Rebuild shared Vizia controls around the section model above.
2. Rebuild Lamath as source/resonator/output/library/modulation sections.
3. Rebuild Glirdir as capture/preview/quantize/audition-export sections.
4. Rebuild Linnod as source-detection/pad-matrix/selected-slice sections.
5. Keep all host bridge and patch persistence APIs unchanged.
6. Audit deleted UI concepts so old form-row and oversized block-button patterns do
   not remain as the primary layout.
