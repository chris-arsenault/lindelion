# Calóma - Backlog

Planned work for Calóma. The implemented behavior is in [caloma.md](caloma.md).

## Verification

- **On-target Windows load-and-run.** Load the staged `Caloma.vst3` (`make build-windows`) in a
  Windows DAW or the Galad host and confirm it instantiates, processes, and the Vizia editor renders
  and is interactive. The bundle cross-compiles + stages cleanly, but Windows behavior is unverified.

## Tuning / quality

- **Light HF character.** The Light order reduces HF presence on clean speech (its denoiser/de-esser)
  more than a strict transparency-first reading implies. Revisit its de-ess/EQ defaults if Light's
  character matters.
- **Re-tune knobs.** The committed defaults come from a small spoken-word battery and a single-pass
  coordinate search. A broader battery, more search passes, or per-effect range adjustments are
  available via `make tune-defaults` if the defaults want refining.

## Editor

- Surface more per-effect parameters in the Vizia editor if the order/enable/intensity + level
  controls prove insufficient to dial in the sound in practice.
