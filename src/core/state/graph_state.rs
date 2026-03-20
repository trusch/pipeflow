//! State of the PipeWire graph.

use crate::domain::audio::{LinkMeterData, MeterData, VolumeControl};
use crate::domain::graph::{Link, Node, Port};
use crate::util::id::{LinkId, NodeId, PortId};
use std::collections::HashMap;

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
