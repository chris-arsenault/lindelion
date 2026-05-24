# lindelion

*Quenya `lindelë` (music-making, the active art) + `-ion`: "son of music-making," or bearer of the art of music. Four syllables: LIN-deh-LEE-on.*

Lindelion is a Rust workspace for audio instruments and shared plugin infrastructure.

## Plugins

| Plugin | Meaning | Description | Links |
| ---- | ---- | ---- | ---- |
| Lamath | Sindarin, "echo" or "ringing of voices" | VST3 resonator instrument with MIDI and sidechain audio inputs | [README](plugins/lamath/README.md), [spec](docs/plugins/lamath.md), [backlog](docs/plugins/lamath-backlog.md) |
| Linnod | Sindarin measured verse unit, from the `lind`/`lin-` song root | Cargo scaffold and patch model | [README](plugins/linnod/README.md), [spec](docs/plugins/linnod.md), [backlog](docs/plugins/linnod-backlog.md) |
| Glirdir | Sindarin `glir-` + `-dir`, "singer" or "song-bearer" | VST3 sing-to-MIDI scratchpad with editor, drag/export, sample-library save, and bundle support | [README](plugins/glirdir/README.md), [spec](docs/plugins/glirdir.md), [backlog](docs/plugins/glirdir-backlog.md) |

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
