# Speech-effect model provenance

Neural-network model weights bundled with the speech effects, with source and license. Models run
inline per [ADR-0014](../docs/adr/0014-nn-inference-allocation.md). The denoiser and the voice gate
run on the native ONNX Runtime via `ort` ([ADR-0015](../docs/adr/0015-denoiser-native-onnx-runtime.md)) —
`tract` cannot optimize either model; SwiftF0 runs on pure-Rust `tract`.

| Model | File | Used by | Source | License |
| --- | --- | --- | --- | --- |
| Silero VAD | `speech/voice-gate/assets/silero_vad.onnx` | Voice Gate | [snakers4/silero-vad](https://github.com/snakers4/silero-vad) `src/silero_vad/data/silero_vad.onnx` | MIT |
| DeepFilterNet 3 (streaming) | `speech/speech-denoiser/assets/denoiser_model.onnx` | Speech Denoiser | Copied from the `hot-mic` project (repo owner's); a streaming-safe DFN3 ONNX export derived from [DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) | MIT / Apache-2.0 |

## DeepFilterNet 3 integration note

`denoiser_model.onnx` is a **single self-contained streaming model**: the whole DFN3 pipeline
(ERB analysis, complex DF coefficients, recurrent state, STFT/ISTFT) is baked into the graph,
with the recurrent state exposed as an explicit tensor. One ONNX Runtime run per 10 ms hop:
- inputs: `input_frame` (480 f32), `states` (45304 f32), `atten_lim_db` (scalar)
- outputs: `enhanced_audio_frame` (480 f32), `new_states` (45304 f32), `lsnr` (scalar)
Hop 480, 48 kHz only (auto-bypass otherwise). End-to-end latency is 1920 samples (four hops),
measured by best-lag alignment of clean speech. Run inline per
[ADR-0014](../docs/adr/0014-nn-inference-allocation.md); the default (non-streaming) DFN3 is not
realtime-safe — this hot-mic export is.

## Silero VAD integration note

`silero_vad.onnx` is Silero v5. One run per 16 kHz window:
- inputs: `input` (576 f32 = 64 context + 512 window @ 16 kHz), `state` (`[2,1,128]` f32), `sr`
  (int64 `[1]` = 16000)
- outputs: `output` (speech probability), `stateN` (`[2,1,128]` f32)
The 64-sample context (the last 64 samples of the previous window) is **required**: without it the
internal STFT windows are misaligned and the model scores even clean speech as non-speech. The
voice gate runs at 48 kHz and decimates 3:1 to feed 16 kHz windows; the speech probability drives a
hysteretic, hold-and-release-smoothed gate applied to the audio with zero reported latency. `tract`
cannot optimize this model (an `If`/`Squeeze` in the decoder fails analysis), so it runs on `ort`
like the denoiser.
