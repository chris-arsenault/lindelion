# Glirdir Performance

Host metadata for this run:

- Date: 2026-05-24
- CPU: Intel(R) Xeon(R) CPU E5-2697 v4 @ 2.30GHz, 2 sockets, 36 cores / 72 threads
- Rust: rustc 1.95.0 (59807616e 2026-04-14), LLVM 22.1.2
- Command: `cargo bench -p lamath -p glirdir`
- Environment: local Linux run, not CPU-pinned, CPU governor not forced to performance

Glirdir's benchmark covers the offline analysis job over a deterministic 3-second mono synthetic singing buffer. This is not an audio-thread realtime budget; it is a throughput check for the Linux-clean analysis pipeline.

| Bench | Mean | Effective throughput | Normalized cost | Notes |
| ---- | ----: | ----: | ----: | ---- |
| `glirdir/analyze_synthetic_3s` | 192.88 ms | 746.56 Ksamples/s | 1.34 us/input sample | Full `run_analysis_job` over 144,000 samples; about 15.55x realtime for 48 kHz mono input |
