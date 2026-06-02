# Cenedril M3 — Step Plan

Expansion of **M3** from [`CENEDRIL-VST-PLAN.md`](CENEDRIL-VST-PLAN.md): a **scrolling
STFT-magnitude spectrogram** in the Vizia editor, fed from the M2 `FrameRing` — log-frequency
scale, dB color map, time scroll, off the audio thread. [depends on M1, M2]

## Verification reality (read first)

Like M1, the **rendering** is `cfg(windows)` Vizia code that Linux `make ci` does not compile —
so the actual on-screen spectrogram is verified by the **cargo-xwin cross-build** (compile) and a
**Windows-host runtime** check (you, manually — not a blocker). **But** the plan's exit ("validated
against known signals: steady sine → horizontal line; sweep → diagonal; silence → floor") is a
statement about the **data transform**, which is platform-neutral. So M3 puts all that correctness
in a **make-ci-tested spectrogram model** (Step 1); the Vizia view (Steps 3–4) only *draws* that
model. Tiers per step:
- **Linux `make ci`** — the spectrogram model (bins → log-freq rows → dB → intensity/color, scroll)
  and the factory/COM restructure.
- **`make build-windows` (cargo-xwin)** — the Vizia view + delivery wiring compile for Windows.
- **Galad / Windows host (manual)** — the spectrogram actually renders.

Two architecture `[DECISION]`s gate M3 (Steps 2 and 3); see the decisions table.

## Reference / reuse map (from the plan's reuse map + the Explore of the codebase)

- **M2 `FrameRing` consumer:** `plugins/cenedril/src/analysis/ring.rs` —
  `FrameRing::drain_frames(&self, scratch: &mut [f32], on_frame: impl FnMut(u64, &[f32])) -> usize`,
  `bins()`; `MeterCell::read() -> MeterSnapshot`; `CenedrilAnalysis::frame_ring()/meter()` return
  `&Arc<FrameRing>`/`&Arc<MeterCell>` (`plugins/cenedril/src/analysis/mod.rs`).
- **Vizia custom drawing:** `crates/lindelion-ui/src/resonator_vizia/platform_drawing.rs` — a custom
  `View` with `fn draw(&self, cx: &mut DrawContext, canvas: &Canvas)`, built via `Self{..}.build(cx,
  |_|{})`, redrawn via `.bind(signal, |v| v.needs_redraw())`. Drawing is **vector only** today
  (`vg::Path` + `canvas.draw_path`/`draw_rect`, `vg::Paint`). **No femtovg image/texture is used
  anywhere in `lindelion-ui`** — see Step 3's `[DECISION]`.
- **Editor refresh loop:** `glirdir_vizia/platform_layout.rs` — a 66 ms `cx.add_timer(... Tick →
  EditorEvent::SyncFromController)`; the sync handler pulls fresh data into reactive `Signal`s,
  triggering redraw. The spectrogram drains the ring on this tick.
- **Processor↔controller today:** the repo shares editor data **only via VST3 `IConnectionPoint`
  messages** (`Vst3PeerConnection.notify`), **never a shared `Arc`**; the factory create-fns are
  `fn() -> ComPtr<FUnknown>` (`vst3_factory.rs:8`) and **cannot inject shared state**. Cenedril's
  M0 processor and controller are separate, unconnected objects. So "the editor reads the ring"
  (M2's stated design) needs a deliberate delivery mechanism — Step 2's `[DECISION]`.
- *ADRs:* [ADR-0023](docs/adr/0023-new-vsts-windows-only.md) (Windows/Vizia, target-gated);
  [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (the editor never touches the audio
  thread — it drains the lock-free ring).

---

## Step 1 — Platform-neutral spectrogram model (the data transform) [depends on M2]
- **File(s):** `plugins/cenedril/src/analysis/spectrogram.rs` (new) +
  `plugins/cenedril/src/analysis/mod.rs` (`mod spectrogram;` + re-export).
- **Reference behavior:** a `Spectrogram` that turns STFT magnitude frames into a scrolling 2-D
  **intensity** image, decoupled from any drawing. Built with `new(rows, columns, sample_rate,
  bins, frame_size)`. `push_column(&mut self, magnitudes: &[f32])` appends one time column
  (ring-buffered over `columns`, oldest scrolls off): for each output **row** `r` (0..rows) on a
  **log-frequency** axis `f_r = f_min·(f_max/f_min)^(r/rows)`, pick bin `k = round(f_r·frame_size/
  sample_rate)`, and store intensity = `db_to_intensity(magnitude_to_db(mag[k]))` in `[0,1]` over a
  fixed dB window (e.g. `[-100, 0]` dBFS). Expose `columns()`/`intensity(row, col)` (or a slice) for
  the view, with the newest column at a known edge. Keep `magnitude_to_db`, `db_to_intensity`, and a
  `colormap(intensity) -> [u8; 4]` (dark floor → bright peak, monotonic) as small pure functions.
  This is **platform-neutral and allocation-light** (buffers sized in `new`).
- **Change:** add `spectrogram.rs`; no audio-thread coupling (the view owns a `Spectrogram` and
  feeds it drained frames). `Spectrogram` does **not** run on the audio thread, so it need not be
  strictly allocation-free, but size its buffers once in `new`.
- **Verify (`make ci`):** the plan's known-signal validation on the data — (a) a steady sine at
  `F` → after pushing frames, the newest column's max-intensity row equals the log-freq row for `F`
  (±1), others near floor; (b) a **sweep** (rising `F` across pushed columns) → the per-column
  max-intensity row index increases monotonically (the "diagonal"); (c) **silence** → all
  intensities at floor (≈0); (d) `colormap` is monotonic (floor darker than peak). **Red:**
  `Spectrogram` doesn't exist (greenfield). **Green:** the mappings hold.

## Step 2 — Frame delivery to the editor (architecture) [DECISION] [depends on M2]
- **File(s):** depends on the decision — either `plugins/cenedril/src/vst3_entry/` (merge
  processor+controller into a single component) or the controller + a new message/`IConnectionPoint`
  path; plus the editor delegate (`vst3_entry/editor.rs`) so it receives `Arc<FrameRing>`/
  `Arc<MeterCell>`.
- **Reference behavior:** M2 states "the editor renders from the ring." VST3 separates the audio
  **processor** from the editor **controller** (possibly cross-process), and this repo wires them
  only via typed messages — so the editor cannot read the processor's `Arc<FrameRing>` without a
  deliberate mechanism. The realistic options:
  - **(A, recommended) Single-component plugin.** Merge `CenedrilVst3Processor` +
    `CenedrilVst3Controller` into one class implementing `IComponent + IAudioProcessor +
    IEditController` (VST3 `kSimpleModeSupported`). One object owns `CenedrilAnalysis`; `createView`
    hands the editor a clone of `frame_ring()`/`meter()`. **No message marshaling, no unsafe pointer
    passing** — true to "the editor renders from the ring." Cost: a plan-specified refactor of M0's
    two-class scaffold; departs from the repo's separate-processor+controller convention.
  - **(B) Two classes + VST3 message frame stream.** Keep both classes, add `IConnectionPoint`; the
    editor's 66 ms tick requests frames, the processor's `notify` drains the ring into a message
    batch sent back. Follows the repo convention but marshals a ~94 frame/s stream through messages
    (overhead + complexity).
  - **(C) Two classes + in-process `Arc` handshake.** Add `IConnectionPoint`; on `connect()` (always
    in-process) the processor passes the `Arc` to the controller via a boxed-pointer message. Simple
    but **`unsafe` and only valid in-process** (breaks if a host runs them out-of-process).
- **[DECISION]:** pick the delivery architecture. **Recommend (A) single-component** — cleanest and
  matches M2's intent for a streaming visualizer; (B) is convention-faithful but heavy; (C) is a
  pragmatic but unsafe shortcut. The choice reshapes Steps 2–4's files.
- **Change:** implement the chosen mechanism so the editor obtains `Arc<FrameRing>` + `Arc<MeterCell>`.
- **Verify:** **`make ci`** — the factory/registration unit test still green for the chosen class
  shape (for (A): one combined class registered, exposing both `IComponent` and `IEditController`
  CIDs as the host expects); the Arc-handoff code compiles. **cargo-xwin** — the wiring compiles for
  Windows. (The live read is exercised by Step 4.)

## Step 3 — Vizia spectrogram view (custom canvas) [DECISION] [depends on #1, #2]
- **File(s):** `crates/lindelion-ui/src/cenedril_vizia/platform.rs` (the `cfg(windows)` Vizia module
  from M1) — add a `SpectrogramView` custom `View`; styles in the module's stylesheet.
- **Reference behavior:** a custom `View` (mirroring `resonator_vizia`'s `WaveformStrip`:
  `Self{..}.build(cx, |_|{})`, `fn draw(&self, cx, canvas)`) that renders the `Spectrogram`'s
  intensity image inside its bounds — log-frequency rows bottom-to-top (or top-to-bottom), time
  scrolling left→right, colored by `colormap`. The view owns a `Spectrogram` and a drained-frame
  scratch.
- **[DECISION] (draw primitive):** **lindelion-ui uses no femtovg image upload today.** Two ways to
  draw the image: **(i, recommended) femtovg image/texture** — build the RGBA buffer from the
  `Spectrogram`, upload via `vg`'s `create_image`/`update_image`, draw with an image `Paint` (one
  texture + one draw per repaint — the right tool for a pixel grid, but **unproven in this repo**, a
  Windows-runtime spike); **(ii, fallback) vector rects** — a colored `draw_rect` per cell (proven
  API, but thousands of rects per repaint). Recommend (i) with (ii) as the fallback if the pinned
  vizia/femtovg doesn't expose image drawing cleanly; resolved at the cargo-xwin/Windows-runtime
  stage.
- **Change:** add `SpectrogramView` + its draw method (chosen primitive). No audio-thread contact.
- **Verify:** **cargo-xwin** — `make build-windows` compiles the view into the Windows bundle.
  **Galad/Windows (manual)** — the view renders. (The intensity/color correctness is already proven
  in Step 1; this step is the drawing, which is Windows-only.)

## Step 4 — Wire the spectrogram into the editor + the drain-and-repaint loop [depends on #2, #3]
- **File(s):** `crates/lindelion-ui/src/cenedril_vizia/platform.rs` (replace the M1 placeholder view
  with the spectrogram; add the 66 ms drain timer) + `plugins/cenedril/src/vst3_entry/editor.rs`
  (pass the `Arc<FrameRing>` from Step 2 into the editor) + `crates/lindelion-ui/src/cenedril_vizia.rs`
  (host/size types carry the ring).
- **Reference behavior:** mirror `glirdir_vizia`'s 66 ms `cx.add_timer(Tick → drain)`; on each tick
  the editor calls `ring.drain_frames(scratch, |_, mag| spectrogram.push_column(mag))` (off the
  audio thread, lock-free read), then marks the `SpectrogramView` `needs_redraw()`. The M1
  placeholder `build_placeholder_view` is replaced by the spectrogram layout. The editor's
  `CenedrilEditorHost` (M1) gains the `Arc<FrameRing>` (+ `Arc<MeterCell>` for later meters).
- **Change:** thread the `Arc<FrameRing>` from the component (Step 2) → editor delegate (`attach`) →
  `CenedrilViziaEditor` → the view; add the timer + drain.
- **Verify:** **`make ci`** — `create_editor_view` still returns a non-null `*mut IPlugView` (the
  editor builds cross-platform); the non-Windows path stays a no-op. **cargo-xwin** —
  `make build-windows` stages a `Cenedril.vst3` whose editor compiles with the live spectrogram.
  **Galad/Windows (manual exit):** a live spectrogram renders from the ring — steady sine →
  horizontal line, sweep → diagonal, silence → floor (the M3 exit, on screen).

---

## Decisions table
| Step | Decision | Recommendation |
| ---- | -------- | -------------- |
| 2 | **Frame delivery** to the editor: (A) single-component plugin, (B) two-class VST3 message stream, or (C) two-class in-process `Arc` handshake. | **(A) single-component** — cleanest for a streaming visualizer; matches M2's "editor renders from the ring." A refactor of M0's two-class scaffold. |
| 3 | **Draw primitive** for the spectrogram: (i) femtovg image/texture upload, or (ii) vector rects. | **(i) image** (right tool; unproven here → Windows-runtime spike) with **(ii) rects** as fallback. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. The executor **stops at the Step 2 and
Step 3 `[DECISION]`s**. Step 1 (the spectrogram model) is fully `make ci`-verifiable and carries the
plan's known-signal correctness; Steps 2–4 wire and draw it, verified by cargo-xwin compile and your
manual Windows/Galad check. The editor's *meters + analysis-signal panel* (peak/RMS/LUFS,
voicing/HNR from the worker snapshot) are **M5**, not here.
