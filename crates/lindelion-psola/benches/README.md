# lindelion-psola Benchmarks

Run with:

```sh
cargo bench -p lindelion-psola
```

For a compile-only smoke check:

```sh
cargo bench -p lindelion-psola --no-run
```

The current crate exposes placeholder PSOLA synthesis and pitch-analysis summary types. These benches cover that public surface until epoch detection and full OLA reconstruction are implemented.
