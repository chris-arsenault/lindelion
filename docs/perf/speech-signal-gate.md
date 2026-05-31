# Speech analysis-signal realtime-strategy gate (M2)

The M2 benchmark gate for the hot-mic speech-effect port. Records, per analysis signal, whether
it is cheap enough to recompute per consumer (`self-derive-ok`) or must run off the audio thread
(`needs-worker`), and whether redundant per-effect computation should be merged into one shared
worker (`needs-sharing`).

## Measured cost

Criterion, `speech/signals/benches/signals.rs` (representative dev machine; relative magnitudes
are the load-bearing result, not absolute numbers):

| Signal(s) | Cost | N× redundant |
| --- | --- | --- |
| SpeechPresence | 12.2 µs / 512-sample block (~24 ns/sample) | ×4 = 43.7 µs (≈0.4% of a core for a 10.7 ms block) |
| FricativeActivity | 9.7 µs / 512 | trivial |
| SibilanceEnergy | 9.9 µs / 512 | trivial |
| Analyzer bundle — PitchHz, PitchConfidence, VoicingScore, VoicingState, OnsetFluxHigh, SpectralFlux, HnrDb | 24 ms / 4096-block (~28% of a core; SwiftF0-dominated; allocates) | ×3 = 49.8 ms (3 independent workers) |

## Verdict

| Signal(s) | Verdict |
| --- | --- |
| SpeechPresence, FricativeActivity, SibilanceEnergy | **self-derive-ok** — inline, allocation-free, negligible even at ×4 |
| PitchHz, PitchConfidence, VoicingScore, VoicingState, OnsetFluxHigh, SpectralFlux, HnrDb | **needs-worker** — off the audio thread (allocates; SwiftF0 + FFT) |

## Sharing decision: deferred

`needs-sharing` is **not decided here.** Each effect keeps its own analysis worker for now, so
the whole chain is built as individual, self-contained signals. The micro-benchmarks above
measure isolated and synthetic-redundant cost — not the real steady-state load of the full
effect chain running end to end. The decision to merge redundant workers into one shared worker
(milestone M4) is deferred until **real, steady, end-state performance statistics** for the
assembled chain exist. M4 remains contingent and is gated on that end-state measurement, not on
these isolated numbers.
