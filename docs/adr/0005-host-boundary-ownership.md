# 0005 — Host boundary ownership

- Status: Accepted
- Date: 2026-05-23

## Context

VST3 (and future CLAP/AU) host integration involves factory registration, FFI string handling, entrypoint exports, fixed-size `IPlugView` base behavior, typed `IMessage` wrappers, malformed-message handling, and host-MIDI normalization. Each of these has a correct answer that should be shared across products; each also has product-specific tails (CIDs, class names, parameter sets, message payloads).

## Decision

Host protocol mechanics live in `crates/lindelion-plugin-shell::vst3`. Plugin crates declare CIDs, class names, parameter sets, processor/controller construction, and product-specific message payloads on top of the shared VST3 layer. Audio input buffers and transport state belong in `ProcessContext`; product processors do not invent parallel host-context DTOs. Host MIDI is normalized through `MidiEventNormalizer` before reaching product code.

## Alternatives considered

- **Pull host mechanics into each plugin.** Duplicates FFI handling, risks malformed-message panics across products, and re-introduces drift across CIDs and parameter wiring.
- **Pull product behavior into the shared shell.** Sound generation is the product. Shared code should not know about modal banks, waveguide topologies, or capture flows.
- **A separate "host adapter" crate per host (VST3, CLAP, AU).** Useful longer term but premature with one host live.

## Consequences

- Glirdir and Lamath's `vst3_entry/` modules are thin adapters: processor, controller, factory, messages, MIDI mapping, state, editor.
- Future CLAP/AU support adds new host-specific layers in `lindelion-plugin-shell` without changing product code.
- Sidechain audio routing uses the existing shared `ProcessContext::input`/`AudioInputBuffer` path; products do not re-implement host-context plumbing for new bus types.
