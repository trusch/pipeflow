# Pipeflow UX foundation

This rollout reframes Pipeflow around a small set of durable user intents instead of a pile of peer utility panels.

## Primary spaces

1. **Patch**
   - Focus the current graph
   - Group related nodes
   - Keep the live patch readable
2. **Auto Connect**
   - Capture repeatable routing rules
   - Reapply familiar wiring patterns when nodes appear
3. **Saved Setups**
   - Save the current patch state
   - Restore a known-good setup later
4. **Details**
   - Contextual inspector on the right
   - Empty state explains how to move between Patch, Auto Connect, and Saved Setups

## Toolbar model

The toolbar keeps only the actions that matter everywhere:

- connection status
- safety state
- command search
- organize patch
- fit all
- undo / redo

Secondary visibility controls move into the **View** menu:

- layers
- background-node visibility
- meters and meter refresh

## Terminology updates

- **Snapshots** → **Saved Setups**
- **Connection Rules** → **Auto Connect**
- **Uninteresting nodes** → **Background nodes**
- **Reset View** → **Fit All**
- **Auto-Layout** → **Organize Patch**

## Selection behavior

- No selection: the Details panel acts as orientation and next-step guidance.
- Single selection: full node details.
- Multi-selection: bulk actions and per-node accordions.

## Migration note

Existing functionality remains reachable; the UX change is mainly about naming and grouping. Old mental-model terms are still referenced in descriptions where needed so command search stays forgiving during the transition.
