# Saved Setups & Recall

Saved setups capture your complete audio configuration — connections, volumes, and node positions — so you can bring a known-good patch back later.

## What Gets Saved

A saved setup captures:

- **Connections** — Which output ports are wired to which input ports
- **Volumes** — All volume levels and mute states per node and channel
- **Positions** — Where nodes are placed on the graph canvas

This is everything needed to recreate your exact setup.

Saved setups are stored locally in your config directory (`~/.config/pipeflow/`) and persist across application restarts.

## Saving a Setup

1. Open Saved Setups (`S`)
2. Click "Save Setup"
3. Enter a descriptive name

The setup saves immediately with a timestamp.

Good naming helps future-you:
- `streaming-setup` — Your OBS + Discord + music routing
- `recording-vocals` — Mic → DAW with monitoring
- `gaming-night` — Game audio + voice chat + music
- `clean-slate` — Default routing, known-good baseline

## Restoring a Setup

1. Open Saved Setups (`S`)
2. Find the setup you want
3. Click "Restore"

Pipeflow recreates the saved connections and applies the saved volume levels.

### What Happens During Restore

- Existing connections that aren't in the saved setup are **left alone** (restore is additive for connections)
- Connections in the saved setup are created if the matching nodes exist
- Volumes are applied to all matched nodes
- Node positions are restored on the canvas

### When Nodes Are Missing

If applications have been closed or devices unplugged since the setup was saved, those connections are silently skipped. No errors, no crashes — the restore applies everything it can.

When the missing application starts again later, you can restore the setup again to pick up the remaining connections.

## Smart Node Matching

PipeWire assigns new numeric IDs to nodes every time an application restarts or a device reconnects. Saved setups can't rely on these IDs.

Instead, Pipeflow matches nodes by:

1. **Node name** — The human-readable name (e.g., "Firefox", "Built-in Audio Analog Stereo")
2. **Media class** — The type of node (Audio/Sink, Audio/Source, etc.)

This means saved setups work reliably even after reboots, application restarts, or device reconnections. The only case where matching fails is if you've genuinely changed your hardware or renamed a device.

## Managing Saved Setups

Saved Setups (`S`) lets you:

- **View** all saved setups with timestamps
- **Preview** setup contents (connections and volumes)
- **Restore** any saved setup
- **Delete** setups you no longer need

Saved setups are lightweight — keep as many as you want.

## Use Cases

### Switching Contexts

Save separate setups for different activities:
- Work: headset mic → meeting app, speakers muted
- Music: DAW → audio interface → monitors
- Gaming: game → headphones, mic → Discord

Restore the appropriate setup when switching tasks.

### Experimentation Safety Net

Before making significant routing changes, save a setup. If the experiment goes wrong, restore and you're back to where you started.

### Live Performance

Save your show routing during soundcheck. If anything goes wrong during the performance, restoring a setup gets you back to the tested configuration in seconds. Combine with [Stage mode](SAFETY.md) for maximum protection.

### Multi-Machine Consistency

If you run similar setups on multiple machines, saved setups help maintain consistency. The smart node matching means the same setup works across machines with equivalent hardware, even if the PipeWire IDs differ.
