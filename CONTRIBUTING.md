# Contributing to Pipeflow

## Development Setup

1. Install Rust (stable toolchain): https://rustup.rs
2. Install PipeWire development libraries:
   ```bash
   # Arch Linux
   pacman -S pipewire libspa
   # Ubuntu/Debian
   apt install libpipewire-0.3-dev libspa-0.2-dev
   ```
3. Clone and build:
   ```bash
   git clone <repo-url>
   cd pipeflow
   cargo build --release
   ```

## Running

```bash
# Local mode (default, requires PipeWire daemon)
cargo run --release

# Headless mode (gRPC server, no GUI)
cargo run --release -- --headless

# With verbose logging
cargo run --release -- -v
```

## Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run benchmarks
cargo bench

# Run clippy (must pass with zero warnings)
cargo clippy --all-targets -- -D warnings
```

## Code Quality Standards

- **Zero clippy warnings** with `-D warnings`
- **Zero unwraps** in production code (test code is fine)
- **All public types and functions** must have doc comments (`///`)
- **Conventional commits**: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `perf:`, `chore:`

## Architecture

```
src/
  app/          # Main application (egui App impl, split into submodules)
  core/         # State management, commands, config, typed errors
  domain/       # Domain models (graph, audio, safety, filters, groups, rules)
  pipewire/     # PipeWire connection, events, meter streams
  network/      # gRPC server/client for remote control
  ui/           # UI panels and components
  util/         # ID types, spatial positioning, layout algorithms
```

### Key Design Decisions

- **Shared state**: `Arc<RwLock<AppState>>` for thread-safe state between UI and PipeWire threads
- **Immediate-mode GUI**: egui with conditional repaint (60Hz active, 4Hz idle)
- **Channel-based communication**: crossbeam channels for PipeWire thread commands and events
- **Auto-reconnect**: Exponential backoff reconnection to PipeWire daemon
- **Graceful degradation**: Meter updates dropped before control commands under load

## Pull Request Guidelines

1. Ensure `cargo clippy --all-targets -- -D warnings` passes
2. Ensure `cargo test` passes (all 270+ tests)
3. Add tests for new functionality
4. Keep PRs focused on a single concern
5. Update documentation for public API changes
