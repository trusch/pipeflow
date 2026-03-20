//! UI-specific application state.

use super::animation::PositionAnimation;
use super::layer_visibility::LayerVisibility;
use super::serde_helpers::{
    persistent_identifiers_serde, persistent_names_serde, persistent_positions_serde,
    persistent_volumes_serde,
};
use crate::domain::audio::VolumeControl;
use crate::domain::filters::FilterSet;
use crate::domain::graph::Node;
use crate::domain::groups::GroupManager;
use crate::domain::rules::RuleManager;
use crate::util::id::{LinkId, NodeId, NodeIdentifier};
use crate::util::spatial::Position;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// UI-specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    /// Currently selected node IDs
    #[serde(skip)]
    pub selected_nodes: HashSet<NodeId>,
    /// Currently selected link (for inspection)
    #[serde(skip)]
    pub selected_link: Option<LinkId>,
    /// Node positions in the graph view (runtime, keyed by ephemeral NodeId)
    #[serde(skip)]
    pub node_positions: HashMap<NodeId, Position>,
    /// Animated position transitions (node_id -> animation state)
    #[serde(skip)]
    pub position_animations: HashMap<NodeId, PositionAnimation>,
    /// Persistent node positions (keyed by stable NodeIdentifier, survives restarts)
    #[serde(default, with = "persistent_positions_serde")]
    pub persistent_positions: HashMap<NodeIdentifier, Position>,
    /// Group manager
    pub groups: GroupManager,
    /// Connection rules manager
    pub rules: RuleManager,
    /// Active filters
    pub filters: FilterSet,
    /// Nodes marked as uninteresting by the user (runtime, keyed by ephemeral NodeId)
    #[serde(skip)]
    pub uninteresting_nodes: HashSet<NodeId>,
    /// Persistent uninteresting nodes (keyed by stable NodeIdentifier, survives restarts)
    #[serde(default, with = "persistent_identifiers_serde")]
    pub persistent_uninteresting: HashSet<NodeIdentifier>,
    /// Custom display names (runtime, keyed by ephemeral NodeId)
    #[serde(skip)]
    pub custom_names: HashMap<NodeId, String>,
    /// Persistent custom display names (keyed by stable NodeIdentifier, survives restarts)
    #[serde(default, with = "persistent_names_serde")]
    pub persistent_custom_names: HashMap<NodeIdentifier, String>,
    /// Persistent volume state (keyed by stable NodeIdentifier, survives node restarts)
    #[serde(default, with = "persistent_volumes_serde")]
    pub persistent_volumes: HashMap<NodeIdentifier, VolumeControl>,
    /// Whether to hide uninteresting nodes from the graph view
    #[serde(default)]
    pub hide_uninteresting: bool,
    /// Whether to show Pipeflow's internal meter helper nodes in the graph
    #[serde(default)]
    pub show_internal_meter_nodes: bool,
    /// Visibility settings for PipeWire stack layers
    #[serde(default)]
    pub layer_visibility: LayerVisibility,
    /// Whether the initial layout has been completed (used for first-start detection)
    #[serde(default)]
    pub initial_layout_done: bool,
    /// Current zoom level
    pub zoom: f32,
    /// Current pan offset
    pub pan: Position,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_nodes: HashSet::new(),
            selected_link: None,
            node_positions: HashMap::new(),
            position_animations: HashMap::new(),
            persistent_positions: HashMap::new(),
            groups: GroupManager::new(),
            rules: RuleManager::new(),
            filters: FilterSet::new(),
            uninteresting_nodes: HashSet::new(),
            persistent_uninteresting: HashSet::new(),
            custom_names: HashMap::new(),
            persistent_custom_names: HashMap::new(),
            persistent_volumes: HashMap::new(),
            hide_uninteresting: false,
            show_internal_meter_nodes: false,
            layer_visibility: LayerVisibility::default(),
            initial_layout_done: false,
            zoom: 1.0,
            pan: Position::zero(),
        }
    }
}

impl UiState {
    /// Selects a single node, deselecting others.
    pub fn select_node(&mut self, id: NodeId) {
        self.selected_nodes.clear();
        self.selected_nodes.insert(id);
    }

    /// Adds a node to the selection.
    pub fn add_to_selection(&mut self, id: NodeId) {
        self.selected_nodes.insert(id);
    }

    /// Toggles node selection.
    pub fn toggle_selection(&mut self, id: NodeId) {
        if self.selected_nodes.contains(&id) {
            self.selected_nodes.remove(&id);
        } else {
            self.selected_nodes.insert(id);
        }
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selected_nodes.clear();
    }

    /// Sets a node's position.
    pub fn set_node_position(&mut self, id: NodeId, pos: Position) {
        self.node_positions.insert(id, pos);
    }

    /// Gets a node's position (or default).
    pub fn get_node_position(&self, id: &NodeId) -> Position {
        self.node_positions
            .get(id)
            .copied()
            .unwrap_or(Position::zero())
    }

    /// Toggles a node's uninteresting status.
    pub fn toggle_uninteresting(&mut self, id: NodeId) {
        if self.uninteresting_nodes.contains(&id) {
            self.uninteresting_nodes.remove(&id);
        } else {
            self.uninteresting_nodes.insert(id);
        }
    }

    /// Returns true if the node is marked as uninteresting.
    pub fn is_uninteresting(&self, id: &NodeId) -> bool {
        self.uninteresting_nodes.contains(id)
    }

    /// Toggles whether to hide uninteresting nodes.
    pub fn toggle_hide_uninteresting(&mut self) {
        self.hide_uninteresting = !self.hide_uninteresting;
    }

    /// Restores a node's uninteresting status from persistent storage if available.
    /// Returns true if the node was marked as uninteresting.
    pub fn restore_uninteresting_for_node(
        &mut self,
        node_id: NodeId,
        identifier: &NodeIdentifier,
    ) -> bool {
        if self.persistent_uninteresting.contains(identifier) {
            self.uninteresting_nodes.insert(node_id);
            true
        } else {
            false
        }
    }

    /// Updates both runtime and persistent uninteresting status for a node.
    pub fn update_uninteresting(
        &mut self,
        node_id: NodeId,
        identifier: &NodeIdentifier,
        uninteresting: bool,
    ) {
        if uninteresting {
            self.uninteresting_nodes.insert(node_id);
            self.persistent_uninteresting.insert(identifier.clone());
        } else {
            self.uninteresting_nodes.remove(&node_id);
            self.persistent_uninteresting.remove(identifier);
        }
    }

    /// Returns the custom display name for a node, if set.
    #[cfg(test)]
    pub fn get_custom_name(&self, id: &NodeId) -> Option<&str> {
        self.custom_names.get(id).map(|s| s.as_str())
    }

    /// Sets a custom display name for a node.
    pub fn set_custom_name(&mut self, node_id: NodeId, identifier: &NodeIdentifier, name: String) {
        self.custom_names.insert(node_id, name.clone());
        self.persistent_custom_names
            .insert(identifier.clone(), name);
    }

    /// Clears the custom display name for a node.
    pub fn clear_custom_name(&mut self, node_id: NodeId, identifier: &NodeIdentifier) {
        self.custom_names.remove(&node_id);
        self.persistent_custom_names.remove(identifier);
    }

    /// Restores a node's custom display name from persistent storage if available.
    /// Returns true if a custom name was restored.
    pub fn restore_custom_name_for_node(
        &mut self,
        node_id: NodeId,
        identifier: &NodeIdentifier,
    ) -> bool {
        if let Some(name) = self.persistent_custom_names.get(identifier).cloned() {
            self.custom_names.insert(node_id, name);
            true
        } else {
            false
        }
    }

    /// Returns the display name for a node, prioritizing custom name if set.
    /// Falls back to node's default display name (description or name).
    pub fn resolved_display_name<'a>(&'a self, node: &'a Node) -> &'a str {
        self.custom_names
            .get(&node.id)
            .map(|s| s.as_str())
            .unwrap_or_else(|| node.display_name())
    }

    /// Restores a node's position from persistent storage if available.
    /// Returns true if a position was restored.
    pub fn restore_position_for_node(
        &mut self,
        node_id: NodeId,
        identifier: &NodeIdentifier,
    ) -> bool {
        if let Some(&pos) = self.persistent_positions.get(identifier) {
            self.node_positions.insert(node_id, pos);
            true
        } else {
            false
        }
    }

    /// Updates both runtime and persistent position for a node.
    pub fn update_position(&mut self, node_id: NodeId, identifier: &NodeIdentifier, pos: Position) {
        self.node_positions.insert(node_id, pos);
        self.persistent_positions.insert(identifier.clone(), pos);
    }

    /// Persists the volume state for a node.
    pub fn persist_volume(&mut self, identifier: &NodeIdentifier, volume: VolumeControl) {
        self.persistent_volumes.insert(identifier.clone(), volume);
    }

    /// Restores a node's volume from persistent storage if available.
    /// Returns the restored volume if found.
    pub fn restore_volume_for_node(&self, identifier: &NodeIdentifier) -> Option<&VolumeControl> {
        self.persistent_volumes.get(identifier)
    }

    /// Cleans up runtime state for a removed node (positions, selections, etc.).
    /// Persistent state is preserved for when the node reappears.
    pub fn cleanup_removed_node(&mut self, node_id: &NodeId) {
        self.node_positions.remove(node_id);
        self.position_animations.remove(node_id);
        self.selected_nodes.remove(node_id);
        self.custom_names.remove(node_id);
        self.uninteresting_nodes.remove(node_id);
    }

    /// Animates a node to a target position.
    /// If fast is true, uses quick animation suitable for short-lived nodes.
    pub fn animate_to_position(&mut self, node_id: NodeId, target: Position, fast: bool) {
        let current = self.node_positions.get(&node_id).copied().unwrap_or(target);

        // Don't animate if already at target (within small tolerance)
        if (current.x - target.x).abs() < 1.0 && (current.y - target.y).abs() < 1.0 {
            self.node_positions.insert(node_id, target);
            return;
        }

        let animation = if fast {
            PositionAnimation::fast(current, target)
        } else {
            PositionAnimation::normal(current, target)
        };
        self.position_animations.insert(node_id, animation);
    }

    /// Updates all position animations. Call this every frame.
    /// Returns true if any animations are still in progress.
    pub fn update_animations(&mut self, dt: f32) -> bool {
        let mut completed = Vec::new();

        for (node_id, animation) in self.position_animations.iter_mut() {
            // Update animation progress
            let done = animation.update(dt);

            // Update the node's current position
            let current_pos = animation.current_position();
            self.node_positions.insert(*node_id, current_pos);

            if done {
                completed.push(*node_id);
            }
        }

        // Remove completed animations and set final positions
        for node_id in &completed {
            if let Some(animation) = self.position_animations.remove(node_id) {
                // Ensure we end exactly at the target
                self.node_positions.insert(*node_id, animation.to);
            }
        }

        !self.position_animations.is_empty()
    }
}
