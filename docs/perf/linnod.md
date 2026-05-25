# Linnod Performance

Linnod's benchmark target covers host-free runtime rendering through the public plugin process boundary:

- `linnod/render_512_poly1`: one active source-backed slice voice rendering a 512-sample stereo block.
- `linnod/render_512_poly16`: sixteen active source-backed slice voices rendering a 512-sample stereo block.

The fixture publishes deterministic source analysis, triggers voices through MIDI note-on events, then measures steady block rendering. Source loading, hashing, pitch detection, onset detection, and pitch-shift cache construction are setup work and are intentionally outside the measured realtime block.

Run:

```bash
cargo bench -p linnod
```

For release-cited numbers, use a pinned-core host with the CPU governor forced to performance and record the date, CPU, Rust version, command, and environment. `make ci` compiles the benchmark with `cargo bench --workspace --no-run`.
