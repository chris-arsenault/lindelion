# lindelion-dsp-utils Performance

Host metadata for this run:

- Date: 2026-05-24
- CPU: Intel(R) Xeon(R) CPU E5-2697 v4 @ 2.30GHz, 2 sockets, 36 cores / 72 threads
- Rust: rustc 1.95.0 (59807616e 2026-04-14), LLVM 22.1.2
- Command: `cargo bench -p lindelion-dsp-utils`
- Environment: local Linux run, not CPU-pinned, CPU governor not forced to performance

| Bench | Mean | Effective throughput | Normalized cost | Notes |
| ---- | ----: | ----: | ----: | ---- |
| `analysis/peak_abs_512` | 174.46 ns | 2.93 Gsamples/s | 0.34 ns/sample | 512-sample seeded noise block |
| `analysis/rms_512` | 438.59 ns | 1.17 Gsamples/s | 0.86 ns/sample | 512-sample seeded noise block |
| `analysis/dft_magnitude_at` | 18.534 us | 53.96 Kops/s | 9.05 ns/input sample | 2048-sample block, one 440 Hz bin |
| `analysis/spectral_centroid_hz` | 31.639 ms | 31.61 ops/s | 15.45 us/input sample | 2048-sample block; direct DFT-style implementation |
| `delay/process_1024` | 15.301 us | 66.92 Msamples/s | 14.94 ns/sample | `DelayLine` read/write with fractional read offset |
| `envelope/adsr_tick_1024` | 4.3326 us | 236.35 Msamples/s | 4.23 ns/sample | Fixed ADSR settings with deterministic trigger pattern |
| `smoothing/tick_1024` | 2.9143 us | 351.37 Msamples/s | 2.85 ns/sample | Smoothed target change every 64 samples |
| `one_pole_lowpass/process_1024` | 9.2529 us | 110.67 Msamples/s | 9.04 ns/sample | 1 kHz cutoff at 48 kHz |
| `biquad/lowpass_1024` | 8.0402 us | 127.36 Msamples/s | 7.85 ns/sample | RBJ lowpass coefficients, DF-I implementation |
| `biquad/coefs_lowpass` | 103.02 ns | 9.71 Msweeps/s | 25.76 ns/coefficient | Four-cutoff coefficient sweep per iteration |
