# Glirdir Benchmarks

Run with:

```sh
cargo bench -p glirdir
```

For a compile-only smoke check:

```sh
cargo bench -p glirdir --no-run
```

The Criterion suite measures the offline analysis job over a fixed 3-second synthetic singing buffer. It is not a realtime budget; it is a Linux-clean throughput check for the host-free analysis pipeline.
