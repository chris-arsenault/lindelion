# Lamath Benchmarks

Run the full DSP Criterion suite with:

```sh
cargo bench -p lamath
```

For a compile-only smoke check:

```sh
cargo bench -p lamath --no-run
```

These benches exercise host-free Lamath DSP paths: modal banks, waveguides, and the synth engine render/note-on hot paths. For steadier Linux timing, pin the process to one core and use a performance CPU governor.
