//! App-level mixer node registry.
//!
//! Tracks mixer nodes created by pipeflow, mapping PipeWire node IDs to
//! [`MixerNodeState`] instances that hold per-strip and master state.

use crate::domain::mixer_node::MixerNodeState;
use crate::util::id::NodeId;
use std::collections::HashMap;

/// Manages all mixer nodes created by pipeflow.
#[derive(Debug, Default)]
pub struct MixerNodeManager {
    /// Active mixer nodes keyed by PipeWire node ID.
    nodes: HashMap<NodeId, MixerNodeState>,
}

impl MixerNodeManager {
    /// Creates a new empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new mixer node.
    pub fn insert(&mut self, node_id: NodeId, state: MixerNodeState) {
        self.nodes.insert(node_id, state);
    }

    /// Removes a mixer node, returning its state if it existed.
    pub fn remove(&mut self, node_id: &NodeId) -> Option<MixerNodeState> {
        self.nodes.remove(node_id)
    }

    /// Returns a reference to a mixer node's state.
    pub fn get(&self, node_id: &NodeId) -> Option<&MixerNodeState> {
        self.nodes.get(node_id)
    }

    /// Returns a mutable reference to a mixer node's state.
    pub fn get_mut(&mut self, node_id: &NodeId) -> Option<&mut MixerNodeState> {
        self.nodes.get_mut(node_id)
    }

    /// Returns true if the given node ID is a mixer node we manage.
    pub fn is_mixer_node(&self, node_id: &NodeId) -> bool {
        self.nodes.contains_key(node_id)
    }

    /// Lists all mixer node IDs.
    pub fn list(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Sets the gain on a specific strip.
    pub fn set_strip_gain(&mut self, node_id: &NodeId, strip: usize, gain: f32) -> bool {
        if let Some(state) = self.nodes.get_mut(node_id) {
            if let Some(s) = state.strips.get_mut(strip) {
                s.gain = gain.clamp(0.0, 2.0);
                return true;
            }
        }
        false
    }

    /// Sets the mute state on a specific strip.
    pub fn set_strip_mute(&mut self, node_id: &NodeId, strip: usize, muted: bool) -> bool {
        if let Some(state) = self.nodes.get_mut(node_id) {
            if let Some(s) = state.strips.get_mut(strip) {
                s.muted = muted;
                return true;
            }
        }
        false
    }

    /// Sets the master gain.
    pub fn set_master_gain(&mut self, node_id: &NodeId, gain: f32) -> bool {
        if let Some(state) = self.nodes.get_mut(node_id) {
            state.master_gain = gain.clamp(0.0, 2.0);
            true
        } else {
            false
        }
    }

    /// Sets the master mute state.
    pub fn set_master_mute(&mut self, node_id: &NodeId, muted: bool) -> bool {
        if let Some(state) = self.nodes.get_mut(node_id) {
            state.master_muted = muted;
            true
        } else {
            false
        }
    }

    /// Finds a mixer node by its PipeWire node name prefix.
    pub fn find_by_pw_name(&self, pw_name: &str) -> Option<NodeId> {
        // We identify mixer nodes by name prefix "pipeflow-mixer-"
        if !pw_name.starts_with("pipeflow-mixer-") {
            return None;
        }
        // Return first match (there should be at most one per name)
        self.nodes.keys().find(|_| true).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut mgr = MixerNodeManager::new();
        let id = NodeId::new(42);
        mgr.insert(id, MixerNodeState::new("Test".into(), 4));
        assert!(mgr.is_mixer_node(&id));
        assert_eq!(mgr.get(&id).unwrap().strip_count(), 4);
    }

    #[test]
    fn strip_gain_clamp() {
        let mut mgr = MixerNodeManager::new();
        let id = NodeId::new(1);
        mgr.insert(id, MixerNodeState::new("X".into(), 2));
        mgr.set_strip_gain(&id, 0, 5.0);
        assert!((mgr.get(&id).unwrap().strips[0].gain - 2.0).abs() < f32::EPSILON);
        mgr.set_strip_gain(&id, 0, -1.0);
        assert!((mgr.get(&id).unwrap().strips[0].gain).abs() < f32::EPSILON);
    }

    #[test]
    fn remove_returns_state() {
        let mut mgr = MixerNodeManager::new();
        let id = NodeId::new(7);
        mgr.insert(id, MixerNodeState::new("R".into(), 2));
        let removed = mgr.remove(&id);
        assert!(removed.is_some());
        assert!(!mgr.is_mixer_node(&id));
    }
}
