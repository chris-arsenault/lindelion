# Calóma

Quenya `cala` ("bright/clear") + `óma` ("voice") — "clear voice".

The single VST3 that packages the ported speech effects (`speech/`) as one serial chain: a
3-valued **signal-order** parameter (three curated topologies, each with an end-to-end-tuned
default), **normal VST patches** that capture the order plus the full parameter tuning, and
**compute-once** shared analysis feeding the chain. See [ADR-0020](../../docs/adr/0020-caloma-speech-vst-packaging.md)
and the implementation plan `CALOMA-VST-PLAN.md` at the repo root.

> **Reserved home only.** This directory is a placeholder; the crate is **not** yet a workspace
> member and has no sources, so the build is untouched. It is registered in `Cargo.toml` and built
> out when milestone M0 of the plan lands.
