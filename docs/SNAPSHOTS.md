# Snapshots & Presets

Snapshots save your complete audio configuration — connections, volumes, and node positions — so you can restore it later. Think of them as save files for your audio setup.

## What Gets Saved

A snapshot captures:

- **Connections** — Which output ports are wired to which input ports
- **Volumes** — All volume levels and mute states per node and channel
- **Positions** — Where nodes are placed on the graph canvas

This is everything needed to recreate your exact setup.

Snapshots are stored locally in your config directory (`~/.config/pipeflow/`) and persist across application restarts.

## Creating Snapshots

1. Open the Snapshots panel (`S`)
2. Click "Save Snapshot"
3. Enter a descriptive name

The snapshot saves immediately with a timestamp.

Good naming helps future-you:
- `streaming-setup` — Your OBS + Discord + music routing
- `recording-vocals` — Mic → DAW with monitoring
- `gaming-night` — Game audio + voice chat + music
- `clean-slate` — Default routing, known-good baseline

## Restoring Snapshots

1. Open the Snapshots panel (`S`)
2. Find the snapshot you want
3. Click "Restore"

Pipeflow recreates the saved connections and applies the saved volume levels.

### What Happens During Restore

- Existing connections that aren't in the snapshot are **left alone** (restore is additive for connections)
- Connections in the snapshot are created if the matching nodes exist
- Volumes are applied to all matched nodes
- Node positions are restored on the canvas

### When Nodes Are Missing

If applications have been closed or devices unplugged since the snapshot was taken, those connections are silently skipped. No errors, no crashes — the restore applies everything it can.

When the missing application starts again later, you can re-restore the snapshot to pick up the remaining connections.

## Smart Node Matching

PipeWire assigns new numeric IDs to nodes every time an application restarts or a device reconnects. Snapshots can't rely on these IDs.

Instead, Pipeflow matches nodes by:

1. **Node name** — The human-readable name (e.g., "Firefox", "Built-in Audio Analog Stereo")
2. **Media class** — The type of node (Audio/Sink, Audio/Source, etc.)

This means snapshots work reliably even after reboots, application restarts, or device reconnections. The only case where matching fails is if you've genuinely changed your hardware or renamed a device.

## Managing Snapshots

The Snapshots panel (`S`) lets you:

- **View** all saved snapshots with timestamps
- **Preview** snapshot contents (connections and volumes)
- **Restore** any snapshot
- **Delete** snapshots you no longer need

Snapshots are lightweight — keep as many as you want.

## Use Cases

### Switching Contexts

Save separate snapshots for different activities:
- Work: headset mic → meeting app, speakers muted
- Music: DAW → audio interface → monitors
- Gaming: game → headphones, mic → Discord

Restore the appropriate snapshot when switching tasks.

### Experimentation Safety Net

Before making significant routing changes, save a snapshot. If the experiment goes wrong, restore and you're back to where you started.

### Live Performance

Save your show routing during soundcheck. If anything goes wrong during the performance, a snapshot restore gets you back to the tested configuration in seconds. Combine with [stage mode](SAFETY.md) for maximum protection.

### Multi-Machine Consistency

If you run similar setups on multiple machines, snapshots help maintain consistency. The smart node matching means the same snapshot works across machines with equivalent hardware, even if the PipeWire IDs differ.
