# Architecture Decision Records

| # | Title | Status | Date |
| - | ----- | ------ | ---- |
| [0001](0001-allocation-free-audio-thread.md) | Allocation-free audio thread | Accepted (refined by ADR-0014) | 2026-05-23 |
| [0002](0002-no-plugin-framework.md) | No general-purpose plugin framework | Accepted | 2026-05-23 |
| [0003](0003-shared-core-extraction.md) | Shared-core extraction policy | Accepted | 2026-05-23 |
| [0004](0004-parameter-registry.md) | Parameter registry as single source of truth | Accepted | 2026-05-23 |
| [0005](0005-host-boundary-ownership.md) | Host boundary ownership | Accepted | 2026-05-23 |
| [0006](0006-typed-ui-commands.md) | Typed UI commands | Accepted | 2026-05-23 |
| [0007](0007-macos-vst3-build-path.md) | macOS-only VST3 build path | Accepted | 2026-05-23 |
| [0008](0008-capture-first-analysis.md) | Capture-first voice-to-MIDI analysis | Accepted | 2026-05-23 |
| [0009](0009-linnod-setup-time-resample-pro.md) | Linnod setup-time Resample Pro rendering | Accepted | 2026-05-28 |
| [0010](0010-resample-pro-fidelity-strategy.md) | Resample Pro pitch-shift fidelity strategy | Accepted | 2026-05-29 |
| [0011](0011-waveguide-tube-tuning-and-2d-mesh.md) | Waveguide tube tuning correction and 2D mesh resonator | Accepted | 2026-05-30 |
| [0012](0012-speech-effect-port-shared-workspace.md) | Speech effect port shares the workspace | Accepted | 2026-05-30 |
| [0013](0013-host-agnostic-effect-core.md) | Host-agnostic effect core | Accepted | 2026-05-30 |
| [0014](0014-dynamic-response-effort-energy-bus.md) | Whole-system dynamic response via an effort/energy bus and prepared operators | Accepted | 2026-05-30 |
| [0015](0015-expressive-low-polyphony-budget.md) | Expressive low-polyphony per-voice budget | Accepted | 2026-05-30 |
| [0016](0016-oversampled-nonlinear-inner-loop.md) | Global 2x oversampled nonlinear inner loop | Accepted | 2026-05-30 |
| [0017](0017-additive-physical-driver-layer.md) | Additive physical-driver layer | Accepted | 2026-05-30 |
| [0018](0018-nn-inference-allocation.md) | NN inference runs inline, not on an audio-through worker | Accepted | 2026-05-30 |
| [0019](0019-denoiser-native-onnx-runtime.md) | Speech NN effects (denoiser, voice gate) run on the native ONNX Runtime (`ort`) | Accepted | 2026-05-30 |
| [0020](0020-caloma-speech-vst-packaging.md) | Speech effects ship as a single VST3 (Calóma) | Accepted | 2026-05-31 |
| [0021](0021-string-two-way-body-coupling-and-output-blend.md) | String two-way body coupling and output blend | Accepted | 2026-05-31 |
| [0022](0022-windows-vst3-host.md) | Windows realtime VST3 host application | Accepted | 2026-05-31 |
| [0023](0023-new-vsts-windows-only.md) | New VSTs target Windows: build path and egui editor | Accepted | 2026-05-31 |
