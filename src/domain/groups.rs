//! Node grouping functionality.
//!
//! Allows users to organize nodes into collapsible groups.

use crate::util::id::{NodeId, NodeIdentifier};
use crate::util::spatial::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Unique identifier for a node group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(pub Uuid);

impl GroupId {
    /// Creates a new random group ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for GroupId {
    fn default() -> Self {
        Self::new()
    }
}

/// A group of nodes that can be manipulated together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGroup {
    /// Unique group ID
    pub id: GroupId,
    /// Group name
    pub name: String,
    /// Member node IDs (runtime, ephemeral)
    #[serde(skip)]
    pub members: HashSet<NodeId>,
    /// Persistent member identifiers (survives restarts)
    #[serde(default)]
    pub persistent_members: HashSet<NodeIdentifier>,
    /// Whether the group is collapsed
    pub collapsed: bool,
    /// Group position (when collapsed)
    pub position: Position,
    /// Group color (for visual distinction)
    pub color: GroupColor,
}

impl NodeGroup {
    /// Creates a new empty group with the given name.
    pub fn new(name: String) -> Self {
        Self {
            id: GroupId::new(),
            name,
            members: HashSet::new(),
            persistent_members: HashSet::new(),
            collapsed: false,
            position: Position::zero(),
            color: GroupColor::default(),
        }
    }

    /// Adds a node to the group.
    #[cfg(test)]
    pub fn add_member(&mut self, node_id: NodeId) {
        self.members.insert(node_id);
    }

    /// Removes a node from the group.
    pub fn remove_member(&mut self, node_id: &NodeId) -> bool {
        self.members.remove(node_id)
    }

    /// Returns true if the group contains the given node.
    #[cfg(test)]
    pub fn contains(&self, node_id: &NodeId) -> bool {
        self.members.contains(node_id)
    }

    /// Reconciles a node: if its identifier is in persistent_members, add its runtime NodeId.
    /// Returns true if the node was added to runtime members.
    pub fn reconcile_node(&mut self, node_id: NodeId, identifier: &NodeIdentifier) -> bool {
        if self.persistent_members.contains(identifier) {
            self.members.insert(node_id);
            true
        } else {
            false
        }
    }

    /// Returns the number of runtime members.
    #[cfg(test)]
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Returns the number of persistent members (survives restarts).
    /// This is the expected count even when runtime members haven't been reconciled yet.
    pub fn persistent_member_count(&self) -> usize {
        self.persistent_members.len()
    }

    /// Returns the effective member count - shows persistent count if runtime is empty
    /// but persistent has members (i.e., before reconciliation).
    pub fn effective_member_count(&self) -> usize {
        if self.members.is_empty() && !self.persistent_members.is_empty() {
            self.persistent_members.len()
        } else {
            self.members.len()
        }
    }

    /// Returns true if the group has no members and no persistent members.
    /// Use this to check if a group is truly empty vs just not yet reconciled.
    pub fn is_truly_empty(&self) -> bool {
        self.members.is_empty() && self.persistent_members.is_empty()
    }

    /// Returns true if the group has persistent members that haven't been reconciled yet.
    pub fn is_pending_reconciliation(&self) -> bool {
        self.members.is_empty() && !self.persistent_members.is_empty()
    }

    /// Toggles the collapsed state.
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }
}

/// Color for a group (for visual distinction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupColor {
    /// Red component (0-255).
    pub r: u8,
    /// Green component (0-255).
    pub g: u8,
    /// Blue component (0-255).
    pub b: u8,
}

impl GroupColor {
    /// Creates a new color.
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Converts to egui Color32.
    pub fn to_color32(self) -> egui::Color32 {
        egui::Color32::from_rgb(self.r, self.g, self.b)
    }

    /// Predefined colors for groups.
    pub fn palette() -> Vec<Self> {
        vec![
            Self::new(99, 155, 255),  // Blue
            Self::new(255, 99, 132),  // Red
            Self::new(75, 192, 192),  // Teal
            Self::new(255, 206, 86),  // Yellow
            Self::new(153, 102, 255), // Purple
            Self::new(255, 159, 64),  // Orange
            Self::new(46, 204, 113),  // Green
            Self::new(231, 76, 60),   // Crimson
            Self::new(52, 152, 219),  // Light Blue
            Self::new(155, 89, 182),  // Violet
        ]
    }

    /// Returns a color from the palette by index.
    pub fn from_palette(index: usize) -> Self {
        let palette = Self::palette();
        palette[index % palette.len()]
    }
}

impl Default for GroupColor {
    fn default() -> Self {
        Self::new(99, 155, 255) // Blue
    }
}

/// Manager for node groups.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupManager {
    /// All groups
    pub groups: Vec<NodeGroup>,
    /// Counter for generating unique group names
    next_group_number: usize,
}

impl GroupManager {
    /// Creates a new empty group manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new group and returns its ID.
    pub fn create_group(&mut self, name: Option<String>) -> GroupId {
        self.next_group_number += 1;
        let name = name.unwrap_or_else(|| format!("Group {}", self.next_group_number));
        let mut group = NodeGroup::new(name);
        group.color = GroupColor::from_palette(self.groups.len());
        let id = group.id;
        self.groups.push(group);
        id
    }

    /// Creates a group with the given members.
    pub fn create_group_with_members(
        &mut self,
        name: Option<String>,
        members: impl IntoIterator<Item = NodeId>,
    ) -> GroupId {
        let id = self.create_group(name);
        if let Some(group) = self.get_group_mut(&id) {
            group.members = members.into_iter().collect();
        }
        id
    }

    /// Removes a group by ID.
    pub fn remove_group(&mut self, id: &GroupId) -> Option<NodeGroup> {
        if let Some(pos) = self.groups.iter().position(|g| g.id == *id) {
            Some(self.groups.remove(pos))
        } else {
            None
        }
    }

    /// Gets a group by ID.
    pub fn get_group(&self, id: &GroupId) -> Option<&NodeGroup> {
        self.groups.iter().find(|g| g.id == *id)
    }

    /// Gets a mutable reference to a group by ID.
    pub fn get_group_mut(&mut self, id: &GroupId) -> Option<&mut NodeGroup> {
        self.groups.iter_mut().find(|g| g.id == *id)
    }

    /// Reconciles a node that just appeared: checks all groups for its identifier
    /// and adds the node's runtime ID to matching groups.
    pub fn reconcile_node(&mut self, node_id: NodeId, identifier: &NodeIdentifier) {
        for group in &mut self.groups {
            group.reconcile_node(node_id, identifier);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_creation() {
        let group = NodeGroup::new("Test Group".to_string());

        assert!(!group.id.0.is_nil());
        assert_eq!(group.name, "Test Group");
        assert!(group.members.is_empty());
        assert!(!group.collapsed);
    }

    #[test]
    fn test_group_membership() {
        let mut group = NodeGroup::new("Test".to_string());
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        group.add_member(node1);
        assert!(group.contains(&node1));
        assert!(!group.contains(&node2));
        assert_eq!(group.member_count(), 1);

        group.add_member(node2);
        assert_eq!(group.member_count(), 2);

        group.remove_member(&node1);
        assert!(!group.contains(&node1));
        assert!(group.contains(&node2));
    }

    #[test]
    fn test_group_collapse() {
        let mut group = NodeGroup::new("Test".to_string());

        assert!(!group.collapsed);

        group.toggle_collapsed();
        assert!(group.collapsed);

        group.toggle_collapsed();
        assert!(!group.collapsed);

        // Can also set directly
        group.collapsed = true;
        assert!(group.collapsed);

        group.collapsed = false;
        assert!(!group.collapsed);
    }

    #[test]
    fn test_group_manager() {
        let mut manager = GroupManager::new();

        let id1 = manager.create_group(Some("Group A".to_string()));
        let id2 = manager.create_group(None);

        assert!(manager.get_group(&id1).is_some());
        assert_eq!(manager.get_group(&id1).unwrap().name, "Group A");
        assert!(manager.get_group(&id2).unwrap().name.starts_with("Group"));
    }

    #[test]
    fn test_group_color_palette() {
        let palette = GroupColor::palette();
        assert!(!palette.is_empty());

        let color = GroupColor::from_palette(0);
        assert_eq!(color, palette[0]);

        // Wraps around
        let color = GroupColor::from_palette(palette.len());
        assert_eq!(color, palette[0]);
    }
}
