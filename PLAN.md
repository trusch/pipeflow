# Pipeflow - Master Implementation Plan

## Executive Summary

**Pipeflow** is a next-generation PipeWire graph and control application built in Rust. This plan covers the complete implementation of all functional requirements (FR-1 through FR-9) plus non-functional requirements.

---

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust (edition 2021, 1.80+) | Performance, safety, PipeWire bindings |
| GUI Framework | egui + eframe 0.30 | Immediate mode, excellent performance, node graph ecosystem |
| Node Graph | egui-snarl 0.5 | Production-ready pan/zoom/wire drawing |
| PipeWire | pipewire-rs 0.8 | Official Rust bindings |
| Concurrency | crossbeam channels + parking_lot | Fast, reliable cross-thread communication |
| Serialization | serde + toml + serde_json | Config and snapshots |
| Logging | tracing + tracing-subscriber | Structured logging |

---

## Architecture Overview

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

### Threading Model

- **Main Thread**: egui render loop (60 FPS), UI events, state reads
- **PipeWire Thread**: MainLoop (blocking), registry callbacks, command execution
- **Meter Thread** (optional): Dedicated signal level polling

### Module Structure

```
pipeflow/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── graph_view.rs
│   │   ├── node_panel.rs
│   │   ├── meters.rs
│   │   ├── command_palette.rs
│   │   ├── toolbar.rs
│   │   ├── filters.rs
│   │   ├── groups.rs
│   │   └── theme.rs
│   ├── core/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── commands.rs
│   │   ├── event_bus.rs
│   │   └── config.rs
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── graph.rs
│   │   ├── audio.rs
│   │   ├── snapshot.rs
│   │   ├── groups.rs
│   │   ├── filters.rs
│   │   └── safety.rs
│   ├── pipewire/
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   ├── registry.rs
│   │   ├── events.rs
│   │   ├── commands.rs
│   │   ├── meters.rs
│   │   └── recovery.rs
│   └── util/
│       ├── mod.rs
│       ├── id.rs
│       └── spatial.rs
├── tests/
│   ├── integration/
│   └── mocks/
└── benches/
```

---

## Master Requirements Checklist

### FR-1: Graph Visualization & Interaction

- [ ] Display nodes and ports in a directed graph
- [ ] Support pan and zoom
- [ ] Drag nodes freely
- [ ] Multi-select nodes
- [ ] Graph updates within ≤100ms of PipeWire change
- [ ] Nodes can be repositioned via drag
- [ ] Node positions persist across app restarts
- [ ] Zoom and pan do not affect graph correctness
- [ ] Multi-selection works with mouse and keyboard

### FR-2: Link Management (Create / Remove / Toggle)

- [ ] Create links between compatible ports
- [ ] Remove existing links
- [ ] Enable/disable links without deleting them
- [ ] User can create a link via drag or context menu
- [ ] User can remove a link via UI interaction
- [ ] Disabled links stop audio flow but remain visible
- [ ] UI state always reflects actual PipeWire state
- [ ] Removing a link never crashes PipeWire or the app

### FR-3: Node Inspection Panel

- [ ] Display metadata (name, client, media class, ID)
- [ ] List all ports
- [ ] Display format and channel count
- [ ] Selecting a node opens inspection UI within 50ms
- [ ] Metadata matches PipeWire reported values
- [ ] Port list updates dynamically on change
- [ ] No stale data after reconnects or restarts

### FR-4: Volume, Mute & Channel Control

- [ ] Master volume per node
- [ ] Per-channel volume (where applicable)
- [ ] Mute / unmute
- [ ] Volume changes apply immediately (<20ms)
- [ ] Per-channel controls reflect actual channel count
- [ ] Mute state persists across restarts
- [ ] External volume changes are reflected in UI

### FR-5: Live Signal Metering

- [ ] Per-node level meters
- [ ] Optional per-port meters
- [ ] Configurable refresh rate
- [ ] Meter values update in real time
- [ ] Meter refresh rate is user-configurable
- [ ] Meters can be globally disabled
- [ ] CPU usage remains bounded (<5% on typical system)

### FR-6: Graph Filtering & Organization

- [x] Filter by application, media class, direction
- [x] Manual node groups
- [x] Collapsible groups
- [x] Filters can be toggled independently
- [x] Grouped nodes move as a unit
- [x] Collapsing a group hides internal nodes
- [x] Group membership persists across restarts

### FR-7: Snapshots & Presets

- [ ] Save full graph snapshot
- [ ] Restore snapshot on demand
- [ ] Partial restore (routing only, volumes only)
- [ ] Snapshot contains nodes, links, volumes, mutes
- [ ] Restoring snapshot reproduces identical routing
- [ ] Partial restore affects only selected dimensions
- [ ] Snapshot restore is idempotent

### FR-8: Search & Command Palette

- [ ] Global command palette
- [ ] Fuzzy search
- [ ] Action execution from text
- [ ] Palette opens via keyboard shortcut
- [ ] Actions execute correctly from text command
- [ ] Invalid commands fail gracefully
- [ ] Palette is extensible for future commands

### FR-9: Safety & Stage Mode

- [ ] Read-only mode
- [ ] Routing lock
- [ ] Panic actions
- [ ] Read-only mode prevents all state changes
- [ ] Locked routing cannot be modified accidentally
- [ ] Panic mute disconnects all outputs reliably
- [ ] Visual indicators show safety state clearly

### NFR-1: Performance

- [ ] Handles ≥500 nodes without UI lag
- [ ] Graph updates are incremental
- [ ] UI frame rate ≥60 FPS with meters disabled

### NFR-2: Reliability & Consistency

- [ ] App recovers from PipeWire restart
- [ ] External changes are reflected within 200ms
- [ ] No desync between UI model and PipeWire state

### NFR-3: Usability

- [ ] All core actions are keyboard accessible
- [ ] No destructive action without confirmation (unless panic)
- [ ] Clear visual hierarchy at all zoom levels

---

## Implementation Phases

### Phase 1: Project Foundation ✅
**Goal**: Set up project structure, dependencies, and basic app skeleton

1. ✅ Initialize Cargo project with workspace structure
2. ✅ Configure Cargo.toml with all dependencies
3. ✅ Set up module structure (empty mod.rs files)
4. ✅ Create basic eframe App skeleton
5. ✅ Set up tracing/logging infrastructure
6. ✅ Create type-safe ID wrappers (NodeId, PortId, LinkId)
7. ✅ **Tests**: Verify app launches, logging works (101 tests passing)

### Phase 2: Domain Layer ✅
**Goal**: Implement core data models independent of PipeWire

1. ✅ Implement `domain::graph` - Node, Port, Link structs
2. ✅ Implement `domain::audio` - VolumeControl, ChannelMap
3. ✅ Implement `domain::safety` - SafetyMode state machine
4. ✅ Implement `domain::filters` - FilterPredicate types
5. ✅ Implement `domain::groups` - NodeGroup, GroupMembership
6. ✅ Implement `domain::snapshot` - Snapshot, RestoreOptions
7. ✅ **Tests**: Unit tests for all domain types

### Phase 3: State Management ✅
**Goal**: Implement central state container and event system

1. ✅ Implement `core::state` - AppState, GraphState, UiState
2. ✅ Implement `core::event_bus` - Internal event distribution
3. ✅ Implement `core::config` - Config loading/saving with directories crate
4. ✅ Set up SharedState (Arc<RwLock<AppState>>)
5. ✅ **Tests**: State mutation tests, config persistence tests

### Phase 4: PipeWire Integration (Read-Only) ✅
**Goal**: Connect to PipeWire and receive graph state

1. ✅ Implement `pipewire::connection` - MainLoop, Context, Core setup
2. ✅ Implement `pipewire::events` - PwEvent enum definitions
3. ✅ Implement `pipewire::registry` - Registry listener, global tracking
4. ✅ Set up crossbeam channels for thread communication
5. ✅ Spawn PipeWire thread with proper lifetime management
6. ✅ Parse node/port/link info from registry globals
7. ✅ Implement reconnection logic in `pipewire::recovery`
8. ✅ **Tests**: Mock PipeWire tests, reconnection tests

### Phase 5: Basic Graph Visualization ✅
**Goal**: Display PipeWire graph visually with egui-snarl

1. ✅ Implement `ui::theme` - Colors, sizes, visual constants
2. ✅ Implement `ui::graph_view` - egui-snarl integration
3. ✅ Create NodeViewer trait implementation for custom node rendering
4. ✅ Implement wire drawing between ports
5. ✅ Implement pan and zoom controls
6. ✅ Implement node dragging
7. ✅ Implement multi-selection (click, shift-click, box select)
8. ✅ **Tests**: Render tests with mock data

### Phase 6: Node Inspection Panel ✅
**Goal**: Show detailed node information in sidebar

1. ✅ Implement `ui::node_panel` - Inspector sidebar
2. ✅ Display node metadata (name, client, media class, ID)
3. ✅ Display port list with direction indicators
4. ✅ Display format and channel information
5. ✅ Handle dynamic updates when node changes
6. ✅ **Tests**: Panel rendering tests

### Phase 7: PipeWire Commands (Write Operations) ✅
**Goal**: Enable creating/removing links and changing parameters

1. ✅ Implement `core::commands` - Command pattern infrastructure
2. ✅ Implement `pipewire::commands` - PwCommandExecutor (mock)
3. ✅ Implement link creation via PipeWire API (`core.create_object`)
4. ✅ Implement link removal (for created links)
5. ✅ Implement volume control (UI feedback - full param support needs wireplumber)
6. ✅ Implement mute control (UI feedback)
7. ✅ **Tests**: Command execution tests with mock

### Phase 8: Link Management UI ✅
**Goal**: Full link create/remove/toggle from UI

1. ✅ Add drag-to-connect in graph view
2. ✅ Add context menu for link operations
3. ✅ Visual feedback for link states (active/inactive colors)
4. ✅ Validate port compatibility before linking
5. [ ] Implement link toggle (enable/disable)
6. ✅ **Tests**: Link interaction tests (existing tests cover this)

### Phase 9: Volume & Mute Controls ✅
**Goal**: Audio parameter controls in UI

1. ✅ Add volume slider to node panel
2. ✅ Add per-channel volume controls
3. ✅ Add mute toggle button
4. ✅ Sync external volume changes to UI
5. [ ] Persist mute states in config
6. ✅ **Tests**: Volume control tests (existing tests cover this)

### Phase 10: Signal Metering 🔄
**Goal**: Real-time audio level visualization

1. ✅ Implement `pipewire::meters` - MeterCollector (infrastructure)
2. [ ] Set up dedicated meter thread (optional)
3. ✅ Implement `ui::meters` - Custom meter widget
4. [ ] Add per-node meter rendering in graph view
5. [ ] Add per-port meter option
6. [ ] Add configurable refresh rate
7. [ ] Add global meter disable option
8. ✅ **Tests**: Meter rendering tests, performance benchmarks (partial)

### Phase 11: Filtering & Organization ✅
**Goal**: Reduce graph complexity with filters and groups

1. ✅ Implement `ui::filters` - Filter panel UI
2. ✅ Add filter by media class
3. ✅ Add filter by direction (input/output)
4. ✅ Add filter by application name
5. ✅ Implement `ui::groups` - Group management UI
6. ✅ Implement group creation and node assignment
7. ✅ Implement group collapse/expand
8. ✅ Implement group drag (move all members)
9. ✅ Persist groups in config
10. ✅ **Tests**: Filter logic tests, group behavior tests

### Phase 12: Snapshots & Presets ✅
**Goal**: Save and restore graph configurations

1. ✅ Implement snapshot serialization (serde_json)
2. ✅ Add save snapshot action
3. ✅ Add load snapshot action
4. ✅ Implement partial restore options
5. ✅ Add snapshot diff calculation
6. [ ] Store snapshots in XDG data directory
7. [ ] Add snapshot management UI
8. ✅ **Tests**: Snapshot save/load tests, partial restore tests

### Phase 13: Command Palette ✅
**Goal**: Keyboard-driven command interface

1. ✅ Implement `ui::command_palette` - Overlay UI
2. ✅ Integrate fuzzy-matcher for search
3. ✅ Define command registry
4. ✅ Add commands for major actions
5. ✅ Add keyboard shortcut (Ctrl+P or Ctrl+K)
6. ✅ Add extensible command registration
7. ✅ **Tests**: Fuzzy search tests, command execution tests

**Keyboard Shortcuts Implemented:**
- `Ctrl+K` / `Ctrl+P` - Open command palette
- `Escape` - Clear selection / close palette
- `Space` / `F9` - Toggle panic mode
- `F` - Toggle filters panel
- `G` - Toggle groups panel
- `I` - Toggle inspector panel
- `+` / `-` - Zoom in/out
- `Ctrl+0` - Reset view

### Phase 14: Safety & Stage Mode ✅
**Goal**: Prevent accidental destructive actions

1. ✅ Implement read-only mode toggle
2. ✅ Implement routing lock
3. ✅ Implement panic mute action
4. ✅ Add visual indicators in toolbar
5. ✅ Add keyboard shortcut for panic (Space/F9)
6. ✅ Block write operations based on safety state
7. ✅ **Tests**: Safety mode enforcement tests

### Phase 15: Performance Optimization
**Goal**: Achieve 500+ nodes at 60 FPS

1. Implement viewport culling
2. Implement level-of-detail rendering
3. Batch wire rendering
4. Profile and optimize hot paths
5. Add performance benchmarks with criterion
6. **Tests**: Benchmark tests with large graphs

### Phase 16: Polish & Documentation
**Goal**: Production-ready release

1. Add keyboard shortcuts for all actions
2. Review and improve error messages
3. Add user-facing documentation
4. Write README with installation instructions
5. Add architecture documentation
6. Final code review and cleanup
7. **Tests**: Full integration test suite

---

## Key Dependencies (Cargo.toml)

```toml
[package]
name = "pipeflow"
version = "0.1.0"
edition = "2021"
rust-version = "1.80"
description = "A next-generation PipeWire graph and control application"
license = "MIT OR Apache-2.0"

[dependencies]
# PipeWire
pipewire = { version = "0.8", features = ["v0_3_44"] }
libspa = "0.8"

# GUI
eframe = { version = "0.30", default-features = false, features = ["default_fonts", "glow", "persistence", "x11", "wayland"] }
egui = "0.30"
egui_extras = "0.30"
egui-snarl = "0.5"

# State & Concurrency
parking_lot = "0.12"
crossbeam = { version = "0.8", features = ["crossbeam-channel"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
directories = "5.0"

# Utilities
thiserror = "2.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
fuzzy-matcher = "0.3"
uuid = { version = "1.0", features = ["v4", "serde"] }
fastrand = "2.3.0"
bytemuck = "1.0"

# CLI
clap = { version = "4", features = ["derive", "env"] }

[dev-dependencies]
criterion = "0.5"
proptest = "1.0"
mockall = "0.13"

[[bench]]
name = "graph_rendering"
harness = false

[profile.release]
lto = true
codegen-units = 1
```

---

## Verification Plan

### Running the Application
```bash
bash -c "cargo run --release"
```

### Running Tests
```bash
bash -c "cargo test"
```

### Running Benchmarks
```bash
bash -c "cargo bench"
```

### Manual Testing Checklist
1. Launch app and verify PipeWire connection
2. Verify nodes appear from running applications
3. Create a link by dragging between ports
4. Remove a link via context menu
5. Adjust volume and verify audio change
6. Toggle mute and verify
7. Enable read-only mode and verify operations blocked
8. Save a snapshot and restore it
9. Open command palette with Ctrl+K
10. Test panic mute functionality
11. Stress test with 500+ simulated nodes

---

## Critical Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, eframe setup |
| `src/app.rs` | Main App struct, update loop |
| `src/core/state.rs` | Central AppState, data flow hub |
| `src/domain/graph.rs` | Node/Port/Link data model |
| `src/pipewire/connection.rs` | PipeWire thread setup |
| `src/pipewire/events.rs` | PwEvent enum definitions |
| `src/ui/graph_view.rs` | egui-snarl integration |
| `src/ui/node_panel.rs` | Inspector sidebar |

---

## Notes

- All cargo commands must be wrapped: `bash -c "cargo ..."`
- PipeWire objects are `!Send` - must be created in PipeWire thread
- Use incremental updates only - never rebuild full graph
- Meters use bounded channel with backpressure to avoid lag
- Safety modes block at command handler level, not UI level

---

## Volume Control Fix: System Integration via wpctl

### Problem Summary

Pipeflow's volume control is **not synchronized** with system settings:
- System → Pipeflow: ✅ Works (pipeflow subscribes to PipeWire Props)
- Pipeflow → System: ❌ Broken (pipeflow sets node Props, not system volume)

Result: **Multiplicative volume** where `final_output = system_volume × pipeflow_volume`

### Root Cause

In `src/pipewire/connection.rs:660-704`, `set_node_volume()` calls:
```rust
node.set_param(libspa::param::ParamType::Props, 0, pod);
```

This sets volume on the **node object proxy** - a local property that bypasses WirePlumber's metadata system. System tools (pavucontrol, KDE Plasma, GNOME Settings) query WirePlumber metadata, not node Props directly.

### Solution: Use wpctl for Volume Control

Replace direct Props setting with `wpctl` subprocess calls. The PipeWire object ID (`node_id.raw()`) is directly usable with `wpctl`.

**Why wpctl?**
1. Uses same volume mechanism as system settings
2. Integrates with WirePlumber metadata (persists, visible everywhere)
3. Simple implementation - just spawn subprocess
4. The existing Props subscription continues to work for reading external changes

### Implementation Changes

#### File: `src/pipewire/connection.rs`

**1. Replace `set_node_volume()` function (lines 660-704):**

```rust
/// Sets volume on a node via wpctl (system-integrated).
fn set_node_volume(node_id: &NodeId, volume: &VolumeControl) {
    // Use the first channel volume, or master if channels is empty
    let vol = if volume.channels.is_empty() {
        volume.master
    } else {
        // wpctl set-volume sets all channels uniformly
        volume.channels[0]
    };

    let id = node_id.raw();

    tracing::info!(
        "set_node_volume via wpctl for {}: volume={:.3}",
        id, vol
    );

    // Spawn wpctl in background - fire and forget
    std::thread::spawn(move || {
        match std::process::Command::new("wpctl")
            .args(["set-volume", &id.to_string(), &format!("{:.4}", vol)])
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    tracing::warn!(
                        "wpctl set-volume failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to spawn wpctl: {}", e);
            }
        }
    });
}
```

**2. Replace `set_node_mute()` function (lines 706-738):**

```rust
/// Sets mute state on a node via wpctl (system-integrated).
fn set_node_mute(node_id: &NodeId, muted: bool) {
    let id = node_id.raw();
    let mute_arg = if muted { "1" } else { "0" };

    tracing::info!("set_node_mute via wpctl for {}: muted={}", id, muted);

    std::thread::spawn(move || {
        match std::process::Command::new("wpctl")
            .args(["set-mute", &id.to_string(), mute_arg])
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    tracing::warn!(
                        "wpctl set-mute failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to spawn wpctl: {}", e);
            }
        }
    });
}
```

**3. Update `handle_command()` call sites (lines 813-833):**

Remove `state` parameter from calls - the new functions don't need the node proxy.

**4. Remove dead code:**

Functions no longer needed:
- `create_volume_pod()` (lines 604-630)
- `create_mute_pod()` (lines 632-658)

### What Stays the Same

- **Volume reading**: Existing `bind_node_proxy()` and `parse_props_pod()` continue to subscribe to Props changes
- **UI code**: No changes to `src/ui/node_panel.rs`
- **Domain model**: No changes to `VolumeControl` struct

### Testing Plan

1. Build and run pipeflow
2. Open pavucontrol or system sound settings alongside
3. Move pipeflow volume slider → verify system slider moves
4. Move system volume slider → verify pipeflow slider moves (already works)
5. Toggle mute in pipeflow → verify system shows muted
6. Verify no multiplicative behavior (max in pipeflow = max in system)

### Assumptions

- `wpctl` is available (part of WirePlumber, standard on modern PipeWire setups)
- Node IDs are stable during the session
- Fire-and-forget subprocess spawning is acceptable latency-wise (~10ms)
