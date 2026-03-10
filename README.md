# Pipeflow

**A next-generation PipeWire graph and control application.**

Pipeflow replaces the patchwork of Helvum, pavucontrol, and qpwgraph with a single, keyboard-driven tool for visual routing, live audio control, and reproducible configurations. Built in Rust with egui, it handles everything from casual desktop audio management to live performance routing.

## Why Pipeflow?

Existing PipeWire tools are fragmented: Helvum visualizes but doesn't control, pavucontrol controls but doesn't route, qpwgraph routes but lacks safety features. Pipeflow combines all three into one cohesive application with stage-safe operation, snapshot presets, and remote control.

## Features

- **Visual graph editing** — Interactive node graph with pan, zoom, drag, multi-select, and smart auto-layout
- **Full audio control** — Per-node and per-channel volume, mute, live signal metering with peak/RMS display
- **Link management** — Create, remove, and toggle connections between any compatible ports
- **Snapshots & presets** — Save and restore complete graph configurations with smart node matching
- **Command palette** — Fuzzy-searchable keyboard command interface (`Ctrl+K`)
- **Safety modes** — Read-only mode, routing lock, stage mode, and panic mute for live performance
- **Filtering & groups** — Filter by media class, direction, or activity; organize nodes into collapsible groups
- **Node inspection** — Detailed metadata, port lists, format info, and connection status
- **Remote control** — Headless gRPC server mode with SSH tunnel support for controlling remote machines
- **Built-in help** — Press `H` for contextual help; `?` buttons throughout the UI

## Run Modes

Pipeflow operates in three modes:

| Mode | Command | Description |
|------|---------|-------------|
| **Local** | `pipeflow` | Full GUI with local PipeWire connection (default) |
| **Headless** | `pipeflow --headless` | gRPC server, no GUI — for remote-controlled machines |
| **Remote** | `pipeflow --remote user@host` | GUI connecting to a remote headless instance via SSH tunnel |

See [docs/REMOTE.md](docs/REMOTE.md) for the full remote control guide.

## Installation

### Dependencies

**Arch Linux:**
```bash
sudo pacman -S pipewire pipewire-audio libpipewire
```

**Fedora:**
```bash
sudo dnf install pipewire-devel
```

**Debian/Ubuntu:**
```bash
sudo apt install libpipewire-0.3-dev libspa-0.2-dev
```

### From Source

```bash
git clone https://github.com/trusch/pipeflow.git
cd pipeflow
cargo install --path .
```

Requires Rust 1.88+ (stable). The `network` feature (gRPC remote control) is enabled by default. To build without it:

```bash
cargo install --path . --no-default-features
```

## Usage

### Quick Start

```bash
pipeflow          # Launch GUI
pipeflow -v       # Launch with verbose logging
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+K` / `Ctrl+P` | Command palette |
| `H` | Toggle help panel |
| `I` | Toggle inspector |
| `F` | Toggle filters |
| `G` | Toggle groups |
| `S` | Toggle snapshots |
| `Space` / `F9` | Panic mute (instant silence) |
| `Ctrl+L` | Toggle routing lock |
| `Ctrl+Shift+R` | Smart reorganize layout |
| `+` / `-` | Zoom in / out |
| `Ctrl+0` | Reset view (fit all) |
| `Ctrl+A` | Select all visible nodes |
| `Escape` | Clear selection / close palette |

### Graph Navigation

- **Pan**: Drag on empty space
- **Zoom**: Scroll wheel or `+`/`-`
- **Select**: Click, Shift+click for multi-select, or box select
- **Connect**: Drag from an output port to an input port
- **Disconnect**: Right-click a link, or select and press Delete

### Safety Features

Pipeflow includes protection mechanisms for live and recording scenarios:

- **Normal mode** — Full control, no restrictions
- **Read-only mode** — Observe without risk of changes
- **Stage mode** — Maximum protection for live performance (read-only + routing lock + prominent panic button)
- **Routing lock** — Freeze connections while still allowing volume adjustments
- **Panic mute** — `Space` or `F9` instantly mutes all outputs; press again to restore

See [docs/SAFETY.md](docs/SAFETY.md) for detailed guidance on when to use each mode.

## CLI Reference

```
pipeflow [OPTIONS]

Options:
    --headless              Run as gRPC server without GUI
    --bind <ADDR>           gRPC bind address [default: 127.0.0.1:50051]
    --token <TOKEN>         Authentication token (also: PIPEFLOW_TOKEN env var)
    --remote <USER@HOST>    Connect to remote headless instance via SSH
    --ssh-port <PORT>       SSH port [default: 22]
    --remote-port <PORT>    Remote gRPC port [default: 50051]
    --local-port <PORT>     Local tunnel port [default: 50051]
    -i, --identity <FILE>   SSH identity file (private key)
    -v, --verbose           Enable verbose logging
    -h, --help              Print help
    -V, --version           Print version
```

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      UI Layer (egui)                         │
│  GraphView │ NodePanel │ Meters │ CommandPalette │ Toolbar   │
└──────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────┐
│                    Application Core                          │
│        State Manager │ Command Handler │ Snapshot Engine     │
└──────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────┐
│                      Domain Layer                            │
│      Graph Model │ Audio Control │ Safety Controller         │
└──────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────┐
│                 PipeWire Integration                         │
│   Connection │ Registry │ Events │ Commands │ Meters         │
└──────────────────────────────────────────────────────────────┘
```

Key design decisions:
- **Shared state**: `Arc<RwLock<AppState>>` for thread-safe access between UI and PipeWire threads
- **Immediate-mode GUI**: egui with conditional repaint (60 Hz active, 4 Hz idle)
- **Channel-based IPC**: crossbeam channels for PipeWire commands and events
- **Auto-reconnect**: Exponential backoff reconnection to PipeWire daemon
- **Graceful degradation**: Meter updates dropped before control commands under load

## Documentation

- [Remote Control Guide](docs/REMOTE.md) — Headless mode, SSH tunnels, gRPC protocol
- [Safety & Stage Modes](docs/SAFETY.md) — Protection modes for live and recording use
- [Snapshots & Presets](docs/SNAPSHOTS.md) — Saving and restoring graph configurations
- [Requirements](docs/REQUIREMENTS.md) — Product requirements document

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, code quality standards, and pull request guidelines.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Acknowledgments

- [PipeWire](https://pipewire.org/) — The multimedia routing infrastructure
- [egui](https://github.com/emilk/egui) — Immediate mode GUI framework
- [egui-snarl](https://github.com/zakarumych/egui-snarl) — Node graph widget for egui
- [pipewire-rs](https://gitlab.freedesktop.org/pipewire/pipewire-rs) — Rust bindings for PipeWire
