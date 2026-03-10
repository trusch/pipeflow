# Safety & Stage Modes

Pipeflow includes a layered safety system designed to prevent audio disasters during live performance, recording sessions, or everyday use. The system has three pillars: safety modes, routing lock, and the panic button.

## Safety Modes

### Normal Mode

Full control with no restrictions. You can create and remove connections, adjust volumes, reorganize nodes, and modify any aspect of the graph.

Use for: initial setup, experimenting, daily desktop audio management.

### Read-Only Mode

Observe without risk. You can navigate the graph, inspect nodes, and view details, but all modifications are blocked. Think of it as museum mode.

Use for: monitoring a working setup, showing your configuration to someone, preventing accidental changes during a session.

### Stage Mode

Maximum protection for live performance. Stage mode combines:

- Read-only protection (no accidental graph changes)
- Routing lock (connections cannot be altered)
- Enhanced panic button visibility (larger, more prominent)
- Extra confirmation required before any state change

The UI reflects stage mode clearly — you won't accidentally think you're in normal mode.

Use for: live performance, important recording sessions, any situation where an accidental change could be catastrophic.

## Routing Lock

Routing lock freezes connections independently of the safety mode. When enabled:

- No new connections can be created
- Existing connections cannot be removed
- Volume and mute controls remain fully functional
- Node repositioning still works

This is the sweet spot when you've nailed your routing but need to keep tweaking levels. Toggle it with `Ctrl+L`.

### Combining with Safety Modes

| Mode | Routing Lock Off | Routing Lock On |
|------|-----------------|-----------------|
| **Normal** | Full control | Volumes only, connections frozen |
| **Read-Only** | No changes | No changes |
| **Stage** | Always locked | Always locked |

Stage mode implicitly enables routing lock. In read-only mode, routing lock is redundant but harmless.

## Panic Button

The panic button is your emergency stop. Press `Space` or `F9` to instantly mute all audio outputs. Press again to restore normal operation.

Design principles:
- **Instant**: No confirmation dialog, no animation, no delay
- **Accessible**: `Space` is the largest key on your keyboard — easy to hit when feedback is screaming at you
- **Reversible**: Press again to unmute; no state is lost
- **Always available**: Works in every safety mode, even stage mode

### When Panic Fires

- All output volumes are set to zero immediately
- A clear visual indicator shows panic state (the UI makes it obvious)
- No connections are modified — your routing stays intact
- Audio flow stops at the volume level, not the connection level

### After Panic

Press `Space` or `F9` again to restore all volumes to their pre-panic levels. The graph returns to exactly where it was.

## Recommendations

### Daily Desktop Use

Normal mode is fine. If you have a setup you like, enable routing lock to prevent accidental disconnections while leaving volume control available.

### Recording Sessions

Read-only mode or routing lock. You don't want to accidentally disconnect a microphone mid-take. Set up your routing first, lock it down, then record.

### Live Performance

Stage mode. No question. Set up and test everything in normal mode during soundcheck, save a snapshot, then switch to stage mode before the show starts. The panic button becomes your safety net.

### Shared Machines

Read-only mode prevents other users from altering your carefully tuned setup. Combine with a snapshot so you can always restore if someone bypasses it.

### Workflow

1. **Setup**: Normal mode — connect everything, adjust levels, position nodes
2. **Test**: Verify audio flows correctly, save a snapshot
3. **Lock**: Switch to appropriate safety mode for the session
4. **Perform/Record**: Work with confidence
5. **Teardown**: Back to normal mode if needed
