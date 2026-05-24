# lindelion-pitch-detect Benchmarks

Run with:

```sh
cargo bench -p lindelion-pitch-detect
```

For a compile-only smoke check:

```sh
cargo bench -p lindelion-pitch-detect --no-run
```

The current Criterion suite benchmarks the Linux-clean streaming zero-crossing pitch tracker on fixed 2048-sample inputs. SwiftF0 end-to-end timing is intentionally left to higher-level offline pipeline benches because model startup and cache state dominate small blocks.
