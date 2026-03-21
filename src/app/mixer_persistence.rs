//! Persistence for mixer node state.
//!
//! Saves and loads [`MixerNodeManager`] state to/from a JSON file so that
//! strip gain, mute, and label settings survive app restarts.  On startup the
//! persisted state is keyed by the mixer's PipeWire node name
//! (`pipeflow-mixer-<display_name>`) so it can be re-associated when the
//! pw-loopback process reappears with a new node ID.

use crate::domain::mixer_node::MixerNodeState;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// On-disk representation of all mixer node states, keyed by display name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedMixerNodes {
    /// Mixer states keyed by the display name (the part after `pipeflow-mixer-`).
    pub nodes: HashMap<String, MixerNodeState>,
}

/// Returns the path to the mixer-nodes persistence file.
fn persistence_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "pipeflow", "pipeflow")
        .context("Failed to determine config directory")?;
    Ok(dirs.config_dir().join("mixer_nodes.json"))
}

/// Loads persisted mixer node states from disk.
///
/// Returns an empty map on any IO/parse error (non-fatal).
pub fn load_persisted_mixer_nodes() -> PersistedMixerNodes {
    let path = match persistence_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("Cannot determine mixer persistence path: {}", e);
            return PersistedMixerNodes::default();
        }
    };

    if !path.exists() {
        return PersistedMixerNodes::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!("Failed to parse mixer_nodes.json: {}", e);
                PersistedMixerNodes::default()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read mixer_nodes.json: {}", e);
            PersistedMixerNodes::default()
        }
    }
}

/// Saves the current mixer node states to disk.
///
/// The [`super::mixer_nodes::MixerNodeManager`] is converted to a name-keyed
/// map so that node IDs (which change across sessions) are not stored.
pub fn save_mixer_node_states(manager: &super::mixer_nodes::MixerNodeManager) -> Result<()> {
    let path = persistence_path()?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
    }

    let mut persisted = PersistedMixerNodes::default();
    for node_id in manager.list() {
        if let Some(state) = manager.get(&node_id) {
            persisted.nodes.insert(state.name.clone(), state.clone());
        }
    }

    let contents = serde_json::to_string_pretty(&persisted)
        .context("Failed to serialize mixer node states")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write mixer_nodes.json to {:?}", path))?;

    tracing::debug!(
        "Saved {} mixer node state(s) to {:?}",
        persisted.nodes.len(),
        path
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_serialization() {
        let state = MixerNodeState::new("Test Mixer".into(), 4);
        let mut persisted = PersistedMixerNodes::default();
        persisted.nodes.insert("Test Mixer".into(), state.clone());

        let json = serde_json::to_string(&persisted).unwrap();
        let restored: PersistedMixerNodes = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.nodes.len(), 1);
        let restored_state = restored.nodes.get("Test Mixer").unwrap();
        assert_eq!(restored_state.strip_count(), 4);
        assert_eq!(restored_state.name, "Test Mixer");
    }

    #[test]
    fn empty_on_missing_file() {
        // load_persisted_mixer_nodes should return empty default, never panic
        let result = load_persisted_mixer_nodes();
        // Just verify it doesn't panic and returns something
        let _ = result.nodes.len();
    }
}
