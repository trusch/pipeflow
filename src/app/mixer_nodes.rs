//! App-level mixer node registry.
//!
//! Tracks mixer nodes created by pipeflow, mapping PipeWire node IDs to
//! [`MixerNodeState`] instances that hold per-strip and master state.

use crate::core::state::GraphState;
use crate::domain::graph::PortDirection;
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn find_by_pw_name(&self, pw_name: &str) -> Option<NodeId> {
        // We identify mixer nodes by name prefix "pipeflow-mixer-"
        let display_name = pw_name.strip_prefix("pipeflow-mixer-")?;
        self.nodes
            .iter()
            .find(|(_, state)| state.name == display_name)
            .map(|(id, _)| *id)
    }

    /// Returns an iterator over all (node_id, state) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &MixerNodeState)> {
        self.nodes.iter()
    }
}

/// Finds the source node feeding a specific strip of a mixer node.
///
/// A mixer node created by pw-loopback has N stereo input port pairs.
/// Strip index `i` corresponds to input ports with channel indices `2*i` and
/// `2*i+1`.  We look for links whose `input_node == mixer_node_id` and whose
/// input port belongs to the strip's channel range, then return the
/// `output_node` from that link (the upstream source).
///
/// Returns `None` if no source is linked to that strip.
pub fn find_source_node_for_strip(
    graph: &GraphState,
    mixer_node_id: NodeId,
    strip_index: usize,
) -> Option<NodeId> {
    // Collect input ports for this mixer node, sorted by channel index
    let mut input_ports: Vec<_> = graph
        .ports
        .values()
        .filter(|p| p.node_id == mixer_node_id && p.direction == PortDirection::Input)
        .collect();
    input_ports.sort_by_key(|p| p.channel.unwrap_or(u32::MAX));

    // Strip i uses ports at indices [2*i, 2*i+1] (stereo pair)
    let port_start = strip_index * 2;
    let strip_port_ids: Vec<_> = input_ports
        .iter()
        .skip(port_start)
        .take(2)
        .map(|p| p.id)
        .collect();

    if strip_port_ids.is_empty() {
        return None;
    }

    // Find a link whose input port is one of this strip's ports
    graph
        .links
        .values()
        .find(|link| link.input_node == mixer_node_id && strip_port_ids.contains(&link.input_port))
        .map(|link| link.output_node)
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
