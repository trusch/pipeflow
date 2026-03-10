# Safety Modes

Pipeflow includes a safety system designed to prevent accidental changes during live performance, recording sessions, or everyday use.

## Modes

### Normal Mode

Full control with no restrictions. You can create and remove connections, adjust volumes, reorganize nodes, and modify any aspect of the graph.

Use for: initial setup, experimenting, daily desktop audio management.

### Read-Only Mode

Observe without risk. You can navigate the graph, inspect nodes, and view details, but all modifications are blocked — both routing changes and volume adjustments are prevented.

Use for: monitoring a working setup, showing your configuration to someone, preventing accidental changes during a session.

### Stage Mode

Maximum protection for live performance. Stage mode blocks all routing and volume changes. The UI reflects stage mode clearly so you won't accidentally think you're in normal mode.

Mute toggles remain available in all modes as a safety valve.

Use for: live performance, important recording sessions, any situation where an accidental change could be catastrophic.

## Switching Modes

- **Toolbar dropdown**: The safety mode selector is in the main toolbar
- **Command palette**: Open with `Ctrl+K` and search for "Read-Only"

## Recommendations

### Daily Desktop Use

Normal mode is fine for most workflows.

### Recording Sessions

Read-only mode. You don't want to accidentally disconnect a microphone mid-take. Set up your routing first, switch to read-only, then record.

### Live Performance

Stage mode. Set up and test everything in normal mode during soundcheck, save a snapshot, then switch to stage mode before the show starts.

### Workflow

1. **Setup**: Normal mode — connect everything, adjust levels, position nodes
2. **Test**: Verify audio flows correctly, save a snapshot
3. **Lock**: Switch to appropriate safety mode for the session
4. **Perform/Record**: Work with confidence
5. **Teardown**: Back to normal mode if needed
