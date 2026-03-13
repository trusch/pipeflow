# Safety Modes

Pipeflow includes a safety system designed to prevent accidental changes during live performance, recording sessions, or everyday use.

## Modes

### Normal Mode

Full control with no restrictions. You can create and remove connections, adjust volumes, organize the patch, and modify any aspect of the graph.

Use for: initial setup, experimenting, daily desktop audio management.

### Read-Only Mode

Observe without risk. You can navigate the graph, inspect nodes, and view details, but all modifications are blocked — both routing changes and volume adjustments are prevented.

Use for: monitoring a working setup, showing your configuration to someone, preventing accidental changes during a session.

### Stage Mode

Maximum protection for live performance. Stage mode blocks routing and volume changes. The top bar and safety card make stage mode obvious so you won't accidentally think you're back in Normal.

Mute toggles remain available in all modes as a safety valve.

Use for: live performance, important recording sessions, any situation where an accidental change could be catastrophic.

## Switching Modes

- **Toolbar safety card**: Shows the current mode, what it blocks, and lets you switch quickly
- **Command palette**: Open with `Ctrl+K`, `Ctrl+P`, or `/` and search for `Normal`, `Read-Only`, or `Stage`

If an action is blocked, Pipeflow explains what was prevented and reminds you to switch back to Normal if you want to edit.

## Recommendations

### Daily Desktop Use

Normal mode is fine for most workflows.

### Recording Sessions

Read-Only mode. You don't want to accidentally disconnect a microphone mid-take. Set up your routing first, switch to Read-Only, then record.

### Live Performance

Stage mode. Set up and test everything in Normal during soundcheck, save a setup, then switch to Stage before the show starts.

### Workflow

1. **Setup**: Normal mode — connect everything, adjust levels, position nodes
2. **Test**: Verify audio flows correctly, save a setup
3. **Protect**: Switch to the appropriate safety mode for the session
4. **Perform / Record**: Work with confidence
5. **Edit again**: Return to Normal when you need to change routing or volume
