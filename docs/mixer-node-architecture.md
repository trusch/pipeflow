# Mixer node architecture

## Status

Planned only. Pipeflow does **not** currently create a real mixer/bus node from the patch context menu.
The existing group mixer stays a UI-only member balancer, and the node mixer is a detailed view of a single existing node.

## Why this exists

The previous group master strip implied a real bus that does not exist in the graph. That was misleading.
A future mixer should instead be an explicit graph object with real ports, real routing, and backend-backed state.

## UX split

- **Group mixer**: convenience UI for balancing several existing nodes together. No synthetic master, no fake bus semantics.
- **Node mixer**: detailed single-node view for master/channel/routing inspection and control on an existing node.
- **Mixer node**: future graph-native node with its own inputs, outputs, routing state, and dedicated mixer UI.

## Entry point

Future creation flow:

1. User right-clicks empty patch space.
2. Context menu offers `Insert Mixer Node…`.
3. Dialog requests:
   - display name
   - input count / stereo pair count
   - output topology (stereo main, optional aux buses later)
   - initial placement in the patch
4. App asks backend/session layer to create the mixer node.
5. Graph updates with a real node and ports.
6. Opening that node uses the mixer-node UI, not the lightweight group balancer.

Until backend support exists, the menu entry should remain absent or explicitly disabled/planned.

## Required graph/backend capabilities

A real mixer node needs backend support for all of the following:

### 1. Real node lifecycle

- create mixer node
- destroy mixer node
- persist and restore mixer-node configuration
- expose stable identifiers so snapshots/rules can refer to it

### 2. Real ports

- N audio input ports
- one or more audio output ports
- stable channel ordering and labels
- optional control ports later for automation

### 3. Real mixer state

At minimum per input strip:

- mute
- gain
- per-channel gain/linking information
- meter feed
- display label

At minimum for master/output:

- output gain
- mute
- meter feed

Nice-to-have later:

- solo
- dim
- aux sends
- pan/balance
- insert points

### 4. Transport/API surface

The command layer will likely need operations equivalent to:

- `CreateMixerNode { name, inputs, outputs, position }`
- `RemoveMixerNode { node_id }`
- `SetMixerStripGain { node_id, strip, gain }`
- `SetMixerStripMute { node_id, strip, muted }`
- `SetMixerMasterGain { node_id, gain }`
- `SetMixerMasterMute { node_id, muted }`
- `RenameMixerStrip { node_id, strip, label }`

Network/proto state must publish mixer-node metadata instead of forcing the UI to infer it from unrelated PipeWire nodes.

## UI architecture

Introduce a distinct center-view route, separate from group and node mixers:

- `CenterViewMode::Graph`
- `CenterViewMode::GroupMixer(GroupId)`
- `CenterViewMode::NodeMixer(NodeId)`
- `CenterViewMode::MixerNode(NodeId)` ← future

The future mixer-node view should render:

- input strips sourced from mixer-node strip state
- a real master/output strip sourced from mixer-node output state
- routing affordances for patching external nodes into mixer inputs and outputs
- clear labeling that this is a graph object, not a grouping helper

## Snapshot/rules implications

Snapshots should persist mixer-node strip state as first-class mixer state, not as ad-hoc per-node volume overrides.
Auto-connect rules may later target mixer-node input ports, but that should be additive and explicit.

## Suggested implementation order

1. backend primitive for creating/removing a graph-native mixer node
2. proto/state model for mixer-node metadata and strip state
3. command handlers and undo support
4. patch context-menu creation flow
5. dedicated `MixerNode` center view
6. snapshot persistence for mixer nodes
7. follow-on features like solo/dim/auxes

## Non-goals for the current change

- do not synthesize a fake master from grouped nodes
- do not pretend the backend can create a mixer today
- do not overload the node mixer to stand in for a future mixer node
