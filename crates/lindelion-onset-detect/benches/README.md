# lindelion-onset-detect Benchmarks

Run with:

```sh
cargo bench -p lindelion-onset-detect
```

For a compile-only smoke check:

```sh
cargo bench -p lindelion-onset-detect --no-run
```

The suite uses fixed 48 kHz synthetic inputs and deterministic PCG noise. For steadier Linux timing, run on a pinned core with the CPU governor set to performance.
