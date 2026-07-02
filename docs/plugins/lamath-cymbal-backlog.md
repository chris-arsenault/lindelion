# Lamath Cymbal Backlog

Planned-but-not-built work for the Lamath Cymbal product. Each item is a positive assertion
of intended future-state behavior.

## Plate geometry

- The plate supports a circular active region via a per-cell boundary mask, with
  energy-stable free-edge conditions on the staircase boundary
  (see [ADR-0050](../adr/0050-cymbal-stiff-plate-fdtd.md)).
- Shallow-shell curvature (bow and cup) raises the low-mode band into true cymbal register
  and strengthens the quadratic nonlinear coupling, completing the gong-to-cymbal morph.

## Tonal balance

- The 250 Hz body sits ≈ +20 dB over the Iowa crash reference at the flagship voicings:
  the flat plate's gong fundamentals. Shallow-shell curvature (above) is the structural
  fix; until then per-preset voicing carries the trim.
- The octave profile shows a moderate (≈ −10 dB) dip near 1 kHz on the flagship voicings.

## Nonlinearity

- The full von Kármán cascade (Airy-stress coupling) is available as an offline-quality
  render mode if the phenomenological cascade's audition ceiling is reached
  (see [ADR-0050](../adr/0050-cymbal-stiff-plate-fdtd.md), Alternatives).
