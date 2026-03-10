//! Simple layout utilities for node positioning.
//!
//! Provides basic node placement that:
//! - Places new nodes near their connected nodes (minimize line lengths)
//! - Avoids overlapping boxes
//! - Positions metering nodes to the right of their main nodes

use crate::core::state::GraphState;
use crate::domain::graph::MediaClass;
use crate::util::id::NodeId;
use crate::util::spatial::Position;
use std::collections::HashMap;

/// Detects if a node is a pipeflow metering node by its name.
/// Metering nodes are named "pipeflow-meter-{node_id}" where node_id is the
/// numeric ID of the node being monitored.
pub fn is_metering_node(node_name: &str) -> bool {
    node_name.starts_with("pipeflow-meter-")
}

/// Extracts the target node ID from a metering node name.
/// Returns None if this is not a metering node or the ID cannot be parsed.
pub fn get_metering_target_id(node_name: &str) -> Option<NodeId> {
    if !is_metering_node(node_name) {
        return None;
    }
    let suffix = node_name.strip_prefix("pipeflow-meter-")?;
    let id: u32 = suffix.parse().ok()?;
    Some(NodeId::new(id))
}

/// Configuration for layout calculations.
/// This is the single source of truth for all placement-related constants.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Node width for layout calculations
    pub node_width: f32,
    /// Node height estimate for layout calculations
    pub node_height: f32,
    /// Horizontal spacing between nodes
    pub node_spacing_x: f32,
    /// Vertical spacing between nodes
    pub node_spacing_y: f32,
    /// Gap between main node and satellite/metering node
    pub satellite_gap: f32,
    /// Vertical offset for satellite nodes (slight stagger)
    pub satellite_offset_y: f32,
    /// Horizontal offset for source/sink column placement
    pub column_offset: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            node_width: 200.0,
            node_height: 80.0,
            node_spacing_x: 60.0,
            node_spacing_y: 80.0,
            satellite_gap: 15.0,
            satellite_offset_y: 10.0,
            column_offset: 250.0,
        }
    }
}

/// Simple layout calculator for node positioning.
pub struct SmartLayout {
    config: LayoutConfig,
}

impl SmartLayout {
    /// Creates a new layout calculator with default config.
    pub fn new() -> Self {
        Self {
            config: LayoutConfig::default(),
        }
    }

    /// Returns a reference to the layout configuration.
    pub fn config(&self) -> &LayoutConfig {
        &self.config
    }

    /// Calculates a position for a newly appearing node.
    ///
    /// Strategy:
    /// 1. Find all nodes this new node is connected to (via links)
    /// 2. If connected to existing nodes: place near the centroid of connected nodes
    /// 3. If no connections: place at viewport center with column hint
    /// 4. Find first non-overlapping position
    pub fn calculate_new_node_position(
        &self,
        node_id: NodeId,
        graph: &GraphState,
        current_positions: &HashMap<NodeId, Position>,
        viewport_center: Position,
    ) -> Position {
        let node = match graph.get_node(&node_id) {
            Some(n) => n,
            None => return viewport_center,
        };

        // Find all nodes this node is connected to via links
        let connected_positions: Vec<Position> = graph
            .links
            .values()
            .filter_map(|link| {
                if link.output_node == node_id {
                    current_positions.get(&link.input_node).copied()
                } else if link.input_node == node_id {
                    current_positions.get(&link.output_node).copied()
                } else {
                    None
                }
            })
            .collect();

        let base_position = if !connected_positions.is_empty() {
            // Place near the centroid of connected nodes
            let sum_x: f32 = connected_positions.iter().map(|p| p.x).sum();
            let sum_y: f32 = connected_positions.iter().map(|p| p.y).sum();
            let count = connected_positions.len() as f32;
            let centroid = Position::new(sum_x / count, sum_y / count);

            // Adjust X based on media class (sources slightly left, sinks slightly right)
            let x_adjust = node
                .media_class
                .as_ref()
                .map(|mc| match mc.layout_column() {
                    -1 => -self.config.node_spacing_x,
                    1 => self.config.node_spacing_x,
                    _ => 0.0,
                })
                .unwrap_or(0.0);

            Position::new(centroid.x + x_adjust, centroid.y)
        } else {
            // No connections - place based on media class column
            let column = node
                .media_class
                .as_ref()
                .map(|mc| mc.layout_column())
                .unwrap_or(0);

            let base_x = match column {
                -1 => viewport_center.x - self.config.column_offset,
                1 => viewport_center.x + self.config.column_offset,
                _ => viewport_center.x,
            };

            Position::new(base_x, viewport_center.y)
        };

        // Find a non-overlapping position near the base
        self.find_free_spot_near(base_position, node.media_class.as_ref(), current_positions)
    }

    /// Finds a free spot near a target position.
    fn find_free_spot_near(
        &self,
        target: Position,
        media_class: Option<&MediaClass>,
        current_positions: &HashMap<NodeId, Position>,
    ) -> Position {
        // Adjust x based on media class
        let adjusted_x = if let Some(mc) = media_class {
            match mc.layout_column() {
                -1 => target.x - self.config.node_spacing_x / 2.0,
                1 => target.x + self.config.node_spacing_x / 2.0,
                _ => target.x,
            }
        } else {
            target.x
        };

        let target = Position::new(adjusted_x, target.y);

        // Try the target position first
        if self.is_position_free(target, current_positions) {
            return target;
        }

        // Try positions in expanding circles
        for radius in 1..20 {
            let offset = radius as f32 * self.config.node_spacing_y;

            // Try below first (most natural for new nodes)
            let below = Position::new(target.x, target.y + offset);
            if self.is_position_free(below, current_positions) {
                return below;
            }

            // Then above
            let above = Position::new(target.x, target.y - offset);
            if self.is_position_free(above, current_positions) {
                return above;
            }

            // Then to the sides
            let right = Position::new(target.x + offset, target.y);
            if self.is_position_free(right, current_positions) {
                return right;
            }

            let left = Position::new(target.x - offset, target.y);
            if self.is_position_free(left, current_positions) {
                return left;
            }
        }

        // Fallback: just place it below the target
        Position::new(target.x, target.y + self.config.node_spacing_y)
    }

    /// Checks if a position is free (no overlapping nodes).
    fn is_position_free(&self, pos: Position, current_positions: &HashMap<NodeId, Position>) -> bool {
        let min_distance =
            (self.config.node_width.powi(2) + self.config.node_height.powi(2)).sqrt() * 0.6;

        for existing in current_positions.values() {
            if pos.distance_to(existing) < min_distance {
                return false;
            }
        }

        true
    }
}

impl Default for SmartLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert!(config.node_spacing_x > 0.0);
        assert!(config.node_width > 0.0);
    }

    #[test]
    fn test_smart_layout_new() {
        let layout = SmartLayout::new();
        assert_eq!(layout.config.node_spacing_x, 60.0);
    }

    #[test]
    fn test_is_metering_node() {
        assert!(is_metering_node("pipeflow-meter-123"));
        assert!(is_metering_node("pipeflow-meter-1"));
        assert!(!is_metering_node("some-other-node"));
        assert!(!is_metering_node("Brave"));
        assert!(!is_metering_node(""));
    }

    #[test]
    fn test_get_metering_target_id() {
        assert_eq!(
            get_metering_target_id("pipeflow-meter-123"),
            Some(NodeId::new(123))
        );
        assert_eq!(
            get_metering_target_id("pipeflow-meter-1"),
            Some(NodeId::new(1))
        );
        assert_eq!(get_metering_target_id("pipeflow-meter-abc"), None);
        assert_eq!(get_metering_target_id("some-other-node"), None);
    }

    #[test]
    fn test_satellite_config_defaults() {
        let config = LayoutConfig::default();
        assert!(config.satellite_gap > 0.0);
    }

    #[test]
    fn test_is_position_free() {
        let layout = SmartLayout::new();
        let mut positions = HashMap::new();
        positions.insert(NodeId::new(1), Position::new(0.0, 0.0));

        // Position far away should be free
        assert!(layout.is_position_free(Position::new(500.0, 500.0), &positions));

        // Position on top of existing node should not be free
        assert!(!layout.is_position_free(Position::new(0.0, 0.0), &positions));

        // Position very close should not be free
        assert!(!layout.is_position_free(Position::new(10.0, 10.0), &positions));
    }

    #[test]
    fn test_calculate_new_node_position_no_connections() {
        let layout = SmartLayout::new();
        let graph = GraphState::default();
        let positions = HashMap::new();
        let viewport_center = Position::new(100.0, 100.0);

        let pos = layout.calculate_new_node_position(
            NodeId::new(1),
            &graph,
            &positions,
            viewport_center,
        );

        // Should place near viewport center
        assert!((pos.x - viewport_center.x).abs() < 300.0);
        assert!((pos.y - viewport_center.y).abs() < 300.0);
    }
}
