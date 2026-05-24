# lindelion-dsp-utils Benchmarks

Run the full Criterion suite with:

```sh
cargo bench -p lindelion-dsp-utils
```

For a compile-only smoke check:

```sh
cargo bench -p lindelion-dsp-utils --no-run
```

For steadier Linux numbers, pin the process to one core and use a performance CPU governor:

```sh
sudo cpupower frequency-set -g performance
taskset -c 2 cargo bench -p lindelion-dsp-utils
```

The benches use fixed 48 kHz inputs, fixed block sizes, and a deterministic PCG seed. Results are host-local timing measurements; commit representative numbers in `docs/perf/lindelion-dsp-utils.md` with CPU, rustc, and date metadata.
