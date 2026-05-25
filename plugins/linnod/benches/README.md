# Linnod Benchmarks

Criterion benches for host-free Linnod runtime paths.

```bash
cargo bench -p linnod
cargo bench -p linnod --no-run
```

The runtime benchmark loads deterministic source analysis, triggers voices through the public plugin process boundary, and measures steady block rendering.
