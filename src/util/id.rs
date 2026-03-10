//! Type-safe ID wrappers for PipeWire objects.
//!
//! These wrappers prevent accidental mixing of different ID types
//! (e.g., using a NodeId where a PortId is expected).

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identifier for a PipeWire node that persists across restarts.
///
/// Unlike `NodeId` (which is assigned by PipeWire and changes on restart),
/// this identifier is based on node properties that remain stable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeIdentifier {
    /// Node name (from PipeWire `node.name` property)
    pub name: String,
    /// Application name (from `application.name` property)
    pub app_name: Option<String>,
    /// Media class string (from `media.class` property)
    pub media_class: Option<String>,
}

impl NodeIdentifier {
    /// Creates a new NodeIdentifier.
    pub fn new(name: String, app_name: Option<String>, media_class: Option<String>) -> Self {
        Self {
            name,
            app_name,
            media_class,
        }
    }
}

impl fmt::Display for NodeIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref app) = self.app_name {
            write!(f, " ({})", app)?;
        }
        Ok(())
    }
}

/// Unique identifier for a PipeWire node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Creates a new NodeId from a raw PipeWire ID.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw PipeWire ID.
    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.0)
    }
}

impl From<u32> for NodeId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Unique identifier for a PipeWire port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortId(pub u32);

impl PortId {
    /// Creates a new PortId from a raw PipeWire ID.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw PipeWire ID.
    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Port({})", self.0)
    }
}

impl From<u32> for PortId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Unique identifier for a PipeWire link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkId(pub u32);

impl LinkId {
    /// Creates a new LinkId from a raw PipeWire ID.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw PipeWire ID.
    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for LinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Link({})", self.0)
    }
}

impl From<u32> for LinkId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Unique identifier for a PipeWire device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub u32);

impl DeviceId {
    /// Creates a new DeviceId from a raw PipeWire ID.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Device({})", self.0)
    }
}

impl From<u32> for DeviceId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Unique identifier for a PipeWire client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub u32);

impl ClientId {
    /// Creates a new ClientId from a raw PipeWire ID.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw PipeWire ID.
    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Client({})", self.0)
    }
}

impl From<u32> for ClientId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Unique identifier for a connection rule (UUID-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId(pub Uuid);

impl RuleId {
    /// Creates a new random RuleId.
    #[inline]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the raw UUID.
    #[inline]
    pub fn raw(&self) -> Uuid {
        self.0
    }
}

impl Default for RuleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rule({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_equality() {
        let id1 = NodeId::new(42);
        let id2 = NodeId::new(42);
        let id3 = NodeId::new(43);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_port_id_equality() {
        let id1 = PortId::new(100);
        let id2 = PortId::new(100);
        let id3 = PortId::new(101);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_link_id_equality() {
        let id1 = LinkId::new(200);
        let id2 = LinkId::new(200);
        let id3 = LinkId::new(201);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_id_display() {
        assert_eq!(format!("{}", NodeId::new(42)), "Node(42)");
        assert_eq!(format!("{}", PortId::new(100)), "Port(100)");
        assert_eq!(format!("{}", LinkId::new(200)), "Link(200)");
    }

    #[test]
    fn test_id_from_u32() {
        let node_id: NodeId = 42u32.into();
        let port_id: PortId = 100u32.into();
        let link_id: LinkId = 200u32.into();

        assert_eq!(node_id.raw(), 42);
        assert_eq!(port_id.raw(), 100);
        assert_eq!(link_id.raw(), 200);
    }

    #[test]
    fn test_id_hash() {
        use std::collections::HashSet;

        let mut nodes: HashSet<NodeId> = HashSet::new();
        nodes.insert(NodeId::new(1));
        nodes.insert(NodeId::new(2));
        nodes.insert(NodeId::new(1)); // duplicate

        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_id_serialization() {
        let node_id = NodeId::new(42);
        let json = serde_json::to_string(&node_id).unwrap();
        let deserialized: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(node_id, deserialized);
    }
}
