# Lamath Performance

Host metadata for this run:

- Date: 2026-05-24
- CPU: Intel(R) Xeon(R) CPU E5-2697 v4 @ 2.30GHz, 2 sockets, 36 cores / 72 threads
- Rust: rustc 1.95.0 (59807616e 2026-04-14), LLVM 22.1.2
- Command: `cargo bench -p lamath -p glirdir`
- Environment: local Linux run, not CPU-pinned, CPU governor not forced to performance

These are host-free Criterion measurements of Lamath DSP code. The numbers are useful for comparing changes on the same machine; use a CPU-pinned run with a performance governor before citing them as stable release figures.

| Bench | Mean | Effective throughput | Normalized cost | Notes |
| ---- | ----: | ----: | ----: | ---- |
| `modal/process_512_n16` | 19.475 us | 26.29 Msamples/s | 38.04 ns/sample | 16-mode modal bank, 512-sample impulse block |
| `modal/process_512_n64` | 69.858 us | 7.33 Msamples/s | 136.44 ns/sample | 64-mode modal bank, 512-sample impulse block |
| `modal/process_512_n256` | 310.52 us | 1.65 Msamples/s | 606.48 ns/sample | 256-mode hard-cap modal bank, 512-sample impulse block |
| `waveguide/string_512` | 41.678 us | 12.29 Msamples/s | 81.40 ns/sample | String-style waveguide, 512-sample impulse block |
| `waveguide/tube_512` | 45.043 us | 11.37 Msamples/s | 87.97 ns/sample | Tube-style waveguide, 512-sample impulse block |
| `engine/render_replace_512_poly1` | 142.72 us | 3.59 Msamples/s | 278.75 ns/sample | `SynthEngine::render_add`, 1 active voice |
| `engine/render_replace_512_poly8` | 1.0819 ms | 473.25 Ksamples/s | 2.11 us/sample | `SynthEngine::render_add`, 8 active voices |
| `engine/render_replace_512_poly16` | 2.0264 ms | 252.67 Ksamples/s | 3.96 us/sample | `SynthEngine::render_add`, 16 active voices |
| `engine/note_on_steady_state` | 21.875 us | 45.71 Kevents/s | 21.88 us/event | Full-polyphony `note_on`, forces voice stealing |
