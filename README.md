# lindelion

*Quenya `lindelë` (music-making, the active art) + `-ion`: "son of music-making," or bearer of the art of music. Four syllables: LIN-deh-LEE-on.*

Lindelion is a Rust workspace for audio instruments and shared plugin infrastructure.

## Plugins

| Plugin | Meaning | Status | Links |
| ---- | ---- | ---- | ---- |
| Lamath | Sindarin, "echo" or "ringing of voices" | Implemented VST3 resonator instrument with optional sidechain note/excitation path; DAW validation pending | [README](plugins/lamath/README.md), [design](docs/plugins/lamath.md) |
| Linnod | Sindarin measured verse unit, from the `lind`/`lin-` song root | Cargo scaffold and patch model | [README](plugins/linnod/README.md), [design](docs/plugins/linnod.md) |
| Glirdir | Sindarin `glir-` + `-dir`, "singer" or "song-bearer" | Sing-to-MIDI scratchpad with VST3 adapter, editor, drag/export, sample-library save, and bundle support; DAW validation pending | [README](plugins/glirdir/README.md), [design](docs/plugins/glirdir.md) |

## Docs

| Topic | Link |
| ---- | ---- |
| Documentation index | [docs/README.md](docs/README.md) |
| Workspace architecture | [docs/architecture.md](docs/architecture.md) |
| Local development | [docs/development.md](docs/development.md) |
| Audio performance contract | [docs/performance.md](docs/performance.md) |
| macOS VST3 build | [docs/macos-vst3-build.md](docs/macos-vst3-build.md) |

## Agent Guides

| Agent | Guide |
| ---- | ---- |
| Codex | [CODEX.md](CODEX.md) |
| Claude | [CLAUDE.md](CLAUDE.md) |

## License

Source-available/unlicensed - see [LICENSE](LICENSE).
