# lindelion

*Quenya `lindelë` (music-making, the active art) + `-ion`: "son of music-making," or bearer of the art of music. Four syllables: LIN-deh-LEE-on.*

Lindelion is a Rust workspace for audio instruments and shared plugin infrastructure.

## Plugins

| Plugin | Meaning | Description | Links |
| ---- | ---- | ---- | ---- |
| Lamath | Sindarin, "echo" or "ringing of voices" | VST3 resonator instrument with MIDI and sidechain audio inputs, plus an opt-in shared-body idiophone mode | [README](plugins/lamath/README.md), [spec](docs/plugins/lamath.md), [backlog](docs/plugins/lamath-backlog.md) |
| Linnod | Sindarin measured verse unit, from the `lind`/`lin-` song root | VST3 melodic slicer instrument with source analysis, slice playback, editor surface, and bundle support | [README](plugins/linnod/README.md), [spec](docs/plugins/linnod.md), [backlog](docs/plugins/linnod-backlog.md) |
| Glirdir | Sindarin `glir-` + `-dir`, "singer" or "song-bearer" | VST3 sing-to-MIDI scratchpad with editor, drag/export, sample-library save, and bundle support | [README](plugins/glirdir/README.md), [spec](docs/plugins/glirdir.md), [backlog](docs/plugins/glirdir-backlog.md) |
| Calóma | Quenya `cala` ("bright/clear") + `óma` ("voice"), "clear voice" | Windows-only speech-clarity VST3: a 20-effect serial chain with 3 signal orders, a self-contained Vizia editor (no host parameters), and per-order tuned defaults | [README](plugins/caloma/README.md), [spec](docs/plugins/caloma.md), [backlog](docs/plugins/caloma-backlog.md) |
| Cenedril | Quenya/Sindarin "mirror, looking-glass" | Windows-only passthrough Visualizer VST3: bit-exact zero-latency passthrough with a Vizia editor — magnitude + reassigned spectrogram, level/LUFS meters, analysis-signal panel, and view/scale/colormap/range controls with persisted settings | [README](plugins/cenedril/README.md), [spec](docs/plugins/cenedril.md), [backlog](docs/plugins/cenedril-backlog.md) |
| Lúmedir | Quenya `lúme` ("time/rhythm") + `-dir` ("keeper") | Windows-only passthrough Speech-Coach VST3: bit-exact zero-latency passthrough feeding an off-thread delivery worker (speaking rate/WPM, pitch dynamism, pauses, clarity) scored against configurable target bands, with a Vizia live readout + session summary | [README](plugins/lumedir/README.md), [spec](docs/plugins/lumedir.md), [DSP](docs/dsp/delivery-metrics.md) |

## Host application

| Application | Meaning | Description | Links |
| ---- | ---- | ---- | ---- |
| Galad | Sindarin, "radiance/light" | Standalone Windows realtime VST3 *host*: live mic → an arbitrary VST3 plugin chain → output device | [README](galad/README.md), [architecture](docs/architecture.md#windows-vst3-host-galad) |

## Speech effects

A port of the `hot-mic` microphone-processor effects into Rust, tuned for spoken word in a
meeting / oration context. Packaging is undecided and kept open behind a host-agnostic core.

| Topic | Link |
| ---- | ---- |
| Speech effects family | [speech/README.md](speech/README.md) |
| Shared workspace decision | [ADR-0012](docs/adr/0012-speech-effect-port-shared-workspace.md) |
| Host-agnostic effect core | [ADR-0013](docs/adr/0013-host-agnostic-effect-core.md) |

## Docs

| Topic | Link |
| ---- | ---- |
| Documentation index | [docs/README.md](docs/README.md) |
| Workspace architecture | [docs/architecture.md](docs/architecture.md) |
| Architecture decisions | [docs/adr/README.md](docs/adr/README.md) |
| Local development | [docs/development.md](docs/development.md) |
| Audio performance contract | [docs/performance.md](docs/performance.md) |
| macOS VST3 build | [docs/macos-vst3-build.md](docs/macos-vst3-build.md) |
| Workspace backlog | [docs/backlog.md](docs/backlog.md) |
| Changelog | [CHANGELOG.md](CHANGELOG.md) |
| Agent guide | [AGENTS.md](AGENTS.md) |

## License

Source-available/unlicensed - see [LICENSE](LICENSE).
