//! Central state management.
//!
//! Contains the main application state and state query helpers.

use crate::domain::audio::{LinkMeterData, MeterData, VolumeControl};
use crate::domain::filters::FilterSet;
use crate::domain::graph::{Link, Node, NodeLayer, Port};
use crate::domain::groups::GroupManager;
use crate::domain::rules::RuleManager;
use crate::domain::safety::SafetyController;
use crate::util::id::{LinkId, NodeId, NodeIdentifier, PortId};
use crate::util::spatial::Position;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Visibility settings for PipeWire stack layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerVisibility {
    /// Show hardware layer nodes (ALSA devices, etc.)
    pub hardware: bool,
    /// Show PipeWire layer nodes (splits, adapters, etc.)
    pub pipewire: bool,
    /// Show session layer nodes (WirePlumber-managed app nodes)
    pub session: bool,
}

impl Default for LayerVisibility {
    fn default() -> Self {
        Self {
            hardware: true,
            pipewire: true,
            session: true,
        }
    }
}

impl LayerVisibility {
    /// Returns true if the given layer is visible.
    pub fn is_visible(&self, layer: NodeLayer) -> bool {
        match layer {
            NodeLayer::Hardware => self.hardware,
            NodeLayer::Pipewire => self.pipewire,
            NodeLayer::Session => self.session,
        }
    }

    /// Toggles visibility for the given layer.
    pub fn toggle(&mut self, layer: NodeLayer) {
        match layer {
            NodeLayer::Hardware => self.hardware = !self.hardware,
            NodeLayer::Pipewire => self.pipewire = !self.pipewire,
            NodeLayer::Session => self.session = !self.session,
        }
    }
}

/// Thread-safe shared state wrapper.
pub type SharedState = Arc<RwLock<AppState>>;

/// Creates a new shared state instance.
pub fn create_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::default()))
}

/// Main application state - single source of truth.
#[derive(Debug, Default)]
pub struct AppState {
    /// Graph data (nodes, ports, links)
    pub graph: GraphState,
    /// UI state (selection, positions, filters)
    pub ui: UiState,
    /// Safety state (mode, locks, panic)
    pub safety: SafetyController,
    /// Connection status
    pub connection: ConnectionState,
}

impl AppState {
    /// Clears all graph state (e.g., on disconnect).
    pub fn clear_graph(&mut self) {
        self.graph.clear();
    }
}

/// State of the PipeWire graph.
#[derive(Debug, Default, Clone)]
pub struct GraphState {
    /// All nodes by ID
    pub nodes: HashMap<NodeId, Node>,
    /// All ports by ID
    pub ports: HashMap<PortId, Port>,
    /// All links by ID
    pub links: HashMap<LinkId, Link>,
    /// Signal meter data by node ID
    pub meters: HashMap<NodeId, MeterData>,
    /// Link flow meter data by link ID (for visual flow effects)
    pub link_meters: HashMap<LinkId, LinkMeterData>,
    /// Volume control state by node ID
    pub volumes: HashMap<NodeId, VolumeControl>,
    /// Nodes where volume control failed (node_id -> error message)
    pub volume_control_failed: HashMap<NodeId, String>,
}

impl GraphState {
    /// Clears all graph data.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.ports.clear();
        self.links.clear();
        self.meters.clear();
        self.link_meters.clear();
        self.volumes.clear();
        self.volume_control_failed.clear();
    }

    /// Adds a node to the graph.
    pub fn add_node(&mut self, node: Node) {
        let id = node.id;
        self.nodes.insert(id, node);
        // Initialize meter and volume data
        self.meters.insert(id, MeterData::default());
        self.volumes.insert(id, VolumeControl::default());
    }

    /// Removes a node and its associated data.
    pub fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        // Remove associated ports
        let port_ids: Vec<_> = self
            .ports
            .values()
            .filter(|p| p.node_id == *id)
            .map(|p| p.id)
            .collect();

        for port_id in port_ids {
            self.ports.remove(&port_id);
        }

        // Collect link IDs being removed (for link_meters cleanup)
        let link_ids_to_remove: Vec<_> = self
            .links
            .iter()
            .filter(|(_, link)| link.output_node == *id || link.input_node == *id)
            .map(|(link_id, _)| *link_id)
            .collect();

        // Remove associated links and their meter data
        for link_id in link_ids_to_remove {
            self.links.remove(&link_id);
            self.link_meters.remove(&link_id);
        }

        // Remove meter and volume data
        self.meters.remove(id);
        self.volumes.remove(id);

        self.nodes.remove(id)
    }

    /// Adds a port to the graph.
    pub fn add_port(&mut self, port: Port) {
        let node_id = port.node_id;
        let port_id = port.id;

        // Add to node's port list
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if !node.port_ids.contains(&port_id) {
                node.port_ids.push(port_id);
            }
        }

        self.ports.insert(port_id, port);
    }

    /// Removes a port and associated links.
    pub fn remove_port(&mut self, id: &PortId) -> Option<Port> {
        // Remove from node's port list
        if let Some(port) = self.ports.get(id) {
            if let Some(node) = self.nodes.get_mut(&port.node_id) {
                node.port_ids.retain(|pid| pid != id);
            }
        }

        // Collect link IDs being removed (for link_meters cleanup)
        let link_ids_to_remove: Vec<_> = self
            .links
            .iter()
            .filter(|(_, link)| link.output_port == *id || link.input_port == *id)
            .map(|(link_id, _)| *link_id)
            .collect();

        // Remove associated links and their meter data
        for link_id in link_ids_to_remove {
            self.links.remove(&link_id);
            self.link_meters.remove(&link_id);
        }

        self.ports.remove(id)
    }

    /// Adds a link to the graph.
    pub fn add_link(&mut self, link: Link) {
        let link_id = link.id;
        self.links.insert(link_id, link);
        // Initialize link meter data for flow visualization
        self.link_meters.insert(link_id, LinkMeterData::default());
    }

    /// Removes a link.
    pub fn remove_link(&mut self, id: &LinkId) -> Option<Link> {
        self.link_meters.remove(id);
        self.links.remove(id)
    }

    /// Gets a node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Gets a port by ID.
    pub fn get_port(&self, id: &PortId) -> Option<&Port> {
        self.ports.get(id)
    }

    /// Gets a link by ID.
    pub fn get_link(&self, id: &LinkId) -> Option<&Link> {
        self.links.get(id)
    }

    /// Gets all ports for a node.
    pub fn ports_for_node(&self, node_id: &NodeId) -> Vec<&Port> {
        self.ports
            .values()
            .filter(|p| p.node_id == *node_id)
            .collect()
    }

    /// Gets all links connected to a node.
    pub fn links_for_node(&self, node_id: &NodeId) -> Vec<&Link> {
        self.links
            .values()
            .filter(|l| l.output_node == *node_id || l.input_node == *node_id)
            .collect()
    }

}

/// Serialization helper for HashMap<NodeIdentifier, Position>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
mod persistent_positions_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct PositionEntry {
        identifier: NodeIdentifier,
        position: Position,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, Position>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<PositionEntry> = map
            .iter()
            .map(|(k, v)| PositionEntry {
                identifier: k.clone(),
                position: *v,
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NodeIdentifier, Position>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<PositionEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.position))
            .collect())
    }
}

/// Serialization helper for HashSet<NodeIdentifier>.
/// Serialize as a Vec since HashSet of complex types needs special handling.
mod persistent_identifiers_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(
        set: &HashSet<NodeIdentifier>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<&NodeIdentifier> = set.iter().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashSet<NodeIdentifier>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<NodeIdentifier> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}

/// Serialization helper for HashMap<NodeIdentifier, String>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
mod persistent_names_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct NameEntry {
        identifier: NodeIdentifier,
        custom_name: String,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, String>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<NameEntry> = map
            .iter()
            .map(|(k, v)| NameEntry {
                identifier: k.clone(),
                custom_name: v.clone(),
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NodeIdentifier, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<NameEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.custom_name))
            .collect())
    }
}

/// Serialization helper for HashMap<NodeIdentifier, VolumeControl>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
mod persistent_volumes_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct VolumeEntry {
        identifier: NodeIdentifier,
        volume: VolumeControl,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, VolumeControl>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<VolumeEntry> = map
            .iter()
            .map(|(k, v)| VolumeEntry {
                identifier: k.clone(),
                volume: v.clone(),
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NodeIdentifier, VolumeControl>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<VolumeEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.volume))
            .collect())
    }
}

/// Animation state for a node position.
#[derive(Debug, Clone, Copy)]
pub struct PositionAnimation {
    /// Starting position
    pub from: Position,
    /// Target position
    pub to: Position,
    /// Animation progress (0.0 to 1.0)
    pub progress: f32,
    /// Animation speed (progress per second)
    pub speed: f32,
}

impl PositionAnimation {
    /// Creates a new animation.
    pub fn new(from: Position, to: Position, speed: f32) -> Self {
        Self {
            from,
            to,
            progress: 0.0,
            speed,
        }
    }

    /// Fast animation for short-lived nodes (like notification sounds).
    pub fn fast(from: Position, to: Position) -> Self {
        Self::new(from, to, 8.0) // Complete in ~125ms
    }

    /// Normal animation speed.
    pub fn normal(from: Position, to: Position) -> Self {
        Self::new(from, to, 5.0) // Complete in ~200ms
    }

    /// Returns the current interpolated position.
    pub fn current_position(&self) -> Position {
        // Use smooth ease-out interpolation
        let t = self.ease_out(self.progress);
        Position::new(
            self.from.x + (self.to.x - self.from.x) * t,
            self.from.y + (self.to.y - self.from.y) * t,
        )
    }

    /// Updates the animation progress. Returns true if animation is complete.
    pub fn update(&mut self, dt: f32) -> bool {
        self.progress = (self.progress + self.speed * dt).min(1.0);
        self.progress >= 1.0
    }

    /// Ease-out cubic function for smooth deceleration.
    fn ease_out(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }
}

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
    pub fn restore_uninteresting_for_node(&mut self, node_id: NodeId, identifier: &NodeIdentifier) -> bool {
        if self.persistent_uninteresting.contains(identifier) {
            self.uninteresting_nodes.insert(node_id);
            true
        } else {
            false
        }
    }

    /// Updates both runtime and persistent uninteresting status for a node.
    pub fn update_uninteresting(&mut self, node_id: NodeId, identifier: &NodeIdentifier, uninteresting: bool) {
        if uninteresting {
            self.uninteresting_nodes.insert(node_id);
            self.persistent_uninteresting.insert(identifier.clone());
        } else {
            self.uninteresting_nodes.remove(&node_id);
            self.persistent_uninteresting.remove(identifier);
        }
    }

    /// Returns the custom display name for a node, if set.
    pub fn get_custom_name(&self, id: &NodeId) -> Option<&str> {
        self.custom_names.get(id).map(|s| s.as_str())
    }

    /// Sets a custom display name for a node.
    pub fn set_custom_name(&mut self, node_id: NodeId, identifier: &NodeIdentifier, name: String) {
        self.custom_names.insert(node_id, name.clone());
        self.persistent_custom_names.insert(identifier.clone(), name);
    }

    /// Clears the custom display name for a node.
    pub fn clear_custom_name(&mut self, node_id: NodeId, identifier: &NodeIdentifier) {
        self.custom_names.remove(&node_id);
        self.persistent_custom_names.remove(identifier);
    }

    /// Restores a node's custom display name from persistent storage if available.
    /// Returns true if a custom name was restored.
    pub fn restore_custom_name_for_node(&mut self, node_id: NodeId, identifier: &NodeIdentifier) -> bool {
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
    pub fn restore_position_for_node(&mut self, node_id: NodeId, identifier: &NodeIdentifier) -> bool {
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

/// PipeWire connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and receiving updates
    Connected,
    /// Connection error
    Error,
}

impl ConnectionState {
    /// Returns true if connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::PortDirection;

    #[test]
    fn test_graph_state_node_operations() {
        let mut state = GraphState::default();

        let node = Node::new(NodeId::new(1), "Test Node".to_string());
        state.add_node(node);

        assert!(state.get_node(&NodeId::new(1)).is_some());
        assert!(state.meters.contains_key(&NodeId::new(1)));
        assert!(state.volumes.contains_key(&NodeId::new(1)));

        state.remove_node(&NodeId::new(1));
        assert!(state.get_node(&NodeId::new(1)).is_none());
        assert!(!state.meters.contains_key(&NodeId::new(1)));
    }

    #[test]
    fn test_graph_state_port_operations() {
        let mut state = GraphState::default();

        let node = Node::new(NodeId::new(1), "Test Node".to_string());
        state.add_node(node);

        let port = Port::new(
            PortId::new(10),
            NodeId::new(1),
            "Test Port".to_string(),
            PortDirection::Output,
        );
        state.add_port(port);

        assert!(state.get_port(&PortId::new(10)).is_some());
        assert!(state
            .get_node(&NodeId::new(1))
            .unwrap()
            .port_ids
            .contains(&PortId::new(10)));

        state.remove_port(&PortId::new(10));
        assert!(state.get_port(&PortId::new(10)).is_none());
    }

    #[test]
    fn test_graph_state_link_removal_on_node_removal() {
        let mut state = GraphState::default();

        state.add_node(Node::new(NodeId::new(1), "Node 1".to_string()));
        state.add_node(Node::new(NodeId::new(2), "Node 2".to_string()));

        state.add_port(Port::new(
            PortId::new(10),
            NodeId::new(1),
            "Out".to_string(),
            PortDirection::Output,
        ));
        state.add_port(Port::new(
            PortId::new(20),
            NodeId::new(2),
            "In".to_string(),
            PortDirection::Input,
        ));

        let link = Link::new(
            LinkId::new(100),
            PortId::new(10),
            PortId::new(20),
            NodeId::new(1),
            NodeId::new(2),
        );
        state.add_link(link);

        assert_eq!(state.links.len(), 1);

        state.remove_node(&NodeId::new(1));
        assert_eq!(state.links.len(), 0);
    }

    #[test]
    fn test_ui_state_selection() {
        let mut state = UiState::default();

        state.select_node(NodeId::new(1));
        assert!(state.selected_nodes.contains(&NodeId::new(1)));
        assert_eq!(state.selected_nodes.len(), 1);

        state.add_to_selection(NodeId::new(2));
        assert!(state.selected_nodes.contains(&NodeId::new(2)));
        assert_eq!(state.selected_nodes.len(), 2);

        state.toggle_selection(NodeId::new(1));
        assert!(!state.selected_nodes.contains(&NodeId::new(1)));
        assert_eq!(state.selected_nodes.len(), 1);

        state.clear_selection();
        assert_eq!(state.selected_nodes.len(), 0);
    }

    #[test]
    fn test_ui_state_positions() {
        let mut state = UiState::default();

        state.set_node_position(NodeId::new(1), Position::new(100.0, 200.0));
        let pos = state.get_node_position(&NodeId::new(1));
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 200.0);

        let default_pos = state.get_node_position(&NodeId::new(99));
        assert_eq!(default_pos.x, 0.0);
        assert_eq!(default_pos.y, 0.0);
    }

    #[test]
    fn test_connection_state() {
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Error.is_connected());
    }
}

    #[test]
    fn test_persistent_positions_serialization() {
        use crate::util::id::NodeIdentifier;
        use crate::util::spatial::Position;

        let mut ui_state = UiState::default();
        ui_state.persistent_positions.insert(
            NodeIdentifier::new(
                "test-node".to_string(),
                Some("TestApp".to_string()),
                Some("Audio/Sink".to_string()),
            ),
            Position::new(100.0, 200.0),
        );

        // Try to serialize
        let result = serde_json::to_string_pretty(&ui_state);
        match result {
            Ok(json) => {
                println!("Serialized JSON:\n{}", json);
                // Try to deserialize back
                let deserialized: UiState = serde_json::from_str(&json).unwrap();
                assert_eq!(deserialized.persistent_positions.len(), 1);
            }
            Err(e) => panic!("Serialization failed: {}", e),
        }
    }
