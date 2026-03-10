# Pipeflow

A next-generation PipeWire graph and control application built in Rust.

Pipeflow combines visual routing, live audio control, and reproducibility into a single, powerful tool. It aims to replace the need for multiple utilities (Helvum, pavucontrol, qpwgraph) with one cohesive application suitable for both daily use and live performance.

## Features

- **Visual Graph Editing** - Interactive node graph with pan, zoom, drag, and multi-select
- **Link Management** - Create, remove, and toggle audio connections between ports
- **Node Inspection** - Detailed metadata, port lists, and format information
- **Volume & Mute Control** - Per-node and per-channel volume with instant feedback
- **Live Signal Metering** - Real-time audio level visualization
- **Filtering & Groups** - Filter by application or media class; organize nodes into collapsible groups
- **Snapshots & Presets** - Save and restore complete graph configurations
- **Command Palette** - Fuzzy-searchable keyboard-driven command interface
- **Safety & Stage Mode** - Read-only mode, routing lock, and panic mute for live performance

## Requirements

- Linux with PipeWire running
- Rust 1.80 or later
- PipeWire development libraries

### Installing Dependencies

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

## Installation

### From Source

```bash
git clone https://github.com/trusch/pipeflow.git
cd pipeflow
cargo build --release
```

The binary will be at `target/release/pipeflow`.

### Running

```bash
cargo run --release
```

Or after building:
```bash
./target/release/pipeflow
```

## Usage

### Graph Navigation

- **Pan**: Click and drag on the background
- **Zoom**: Scroll wheel or `+`/`-` keys
- **Reset View**: `Ctrl+0`

### Node Interaction

- **Select**: Click on a node
- **Multi-select**: Shift+click or box select
- **Move**: Drag selected nodes
- **Inspect**: Click to open the inspector panel (`I` to toggle)

### Link Management

- **Create Link**: Drag from an output port to an input port
- **Remove Link**: Right-click on a link and select remove
- **Context Menu**: Right-click on nodes or ports for options

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+K` / `Ctrl+P` | Open command palette |
| `Escape` | Clear selection / close palette |
| `Space` / `F9` | Toggle panic mode (mute all) |
| `F` | Toggle filters panel |
| `G` | Toggle groups panel |
| `I` | Toggle inspector panel |
| `+` / `-` | Zoom in / out |
| `Ctrl+0` | Reset view |

### Safety Features

- **Read-Only Mode**: Prevents all modifications to the graph
- **Routing Lock**: Prevents link creation/deletion while allowing volume changes
- **Panic Mute**: Instantly mutes all outputs (Space or F9)

## Configuration

Configuration is stored in your XDG config directory:
- Linux: `~/.config/pipeflow/`

Settings include:
- Node positions
- Group definitions
- Filter preferences
- Window state

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

## Development

### Running Tests

```bash
cargo test
```

### Running Benchmarks

```bash
cargo bench
```

### Debug Logging

```bash
RUST_LOG=pipeflow=debug cargo run
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo test` and `cargo clippy`
5. Submit a pull request

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

- [PipeWire](https://pipewire.org/) - The audio/video routing infrastructure
- [egui](https://github.com/emilk/egui) - Immediate mode GUI framework
- [egui-snarl](https://github.com/zakarumych/egui-snarl) - Node graph widget for egui
- [pipewire-rs](https://gitlab.freedesktop.org/pipewire/pipewire-rs) - Rust bindings for PipeWire
