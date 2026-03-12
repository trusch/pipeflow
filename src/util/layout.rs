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
    /// Uses a spatial grid for O(1) amortized collision detection on large graphs.
    fn find_free_spot_near(
        &self,
        target: Position,
        media_class: Option<&MediaClass>,
        current_positions: &HashMap<NodeId, Position>,
    ) -> Position {
        let min_distance = self.min_distance();

        // Build spatial grid for fast proximity queries
        let grid = crate::util::spatial::SpatialGrid::from_positions(
            min_distance,
            current_positions.values().copied(),
        );

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
        if !grid.has_neighbor_within(target, min_distance) {
            return target;
        }

        // Try positions in expanding circles
        for radius in 1..20 {
            let offset = radius as f32 * self.config.node_spacing_y;

            // Try below first (most natural for new nodes)
            let below = Position::new(target.x, target.y + offset);
            if !grid.has_neighbor_within(below, min_distance) {
                return below;
            }

            // Then above
            let above = Position::new(target.x, target.y - offset);
            if !grid.has_neighbor_within(above, min_distance) {
                return above;
            }

            // Then to the sides
            let right = Position::new(target.x + offset, target.y);
            if !grid.has_neighbor_within(right, min_distance) {
                return right;
            }

            let left = Position::new(target.x - offset, target.y);
            if !grid.has_neighbor_within(left, min_distance) {
                return left;
            }
        }

        // Fallback: just place it below the target
        Position::new(target.x, target.y + self.config.node_spacing_y)
    }

    /// Returns the minimum distance between nodes for layout.
    fn min_distance(&self) -> f32 {
        (self.config.node_width.powi(2) + self.config.node_height.powi(2)).sqrt() * 0.6
    }

    /// Checks if a position is free (no overlapping nodes).
    /// Uses O(n) scan — for batch operations, prefer building a SpatialGrid.
    #[cfg(test)]
    fn is_position_free(&self, pos: Position, current_positions: &HashMap<NodeId, Position>) -> bool {
        let min_distance = self.min_distance();

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

/// Performs a force-directed layout on nodes.
///
/// Uses a simple spring-electric model:
/// - Repulsion between all node pairs (Coulomb's law)
/// - Attraction along links (Hooke's law)
/// - Layer bias to create left-to-right flow (sources → sinks)
pub fn force_directed_layout(
    nodes: &[(NodeId, Option<MediaClass>)],
    links: &[(NodeId, NodeId)],
    existing_positions: &HashMap<NodeId, Position>,
    _config: &LayoutConfig,
) -> HashMap<NodeId, Position> {
    if nodes.is_empty() {
        return HashMap::new();
    }

    const ITERATIONS: usize = 200;
    const REPULSION: f32 = 5000.0;
    const ATTRACTION: f32 = 0.01;
    const LAYER_BIAS: f32 = 0.5;
    const DAMPING: f32 = 0.9;
    const MAX_VELOCITY: f32 = 50.0;
    const MIN_DIST: f32 = 1.0;

    // Build index for fast lookup
    let node_indices: HashMap<NodeId, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();

    // Initialize positions
    let mut positions: Vec<[f32; 2]> = nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| {
            if let Some(pos) = existing_positions.get(id) {
                [pos.x, pos.y]
            } else {
                // Spread unpositioned nodes in a grid pattern
                let row = i / 5;
                let col = i % 5;
                [col as f32 * 250.0, row as f32 * 150.0]
            }
        })
        .collect();

    let mut velocities: Vec<[f32; 2]> = vec![[0.0, 0.0]; nodes.len()];
    let n = nodes.len();

    for _ in 0..ITERATIONS {
        let mut forces: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];

        // Repulsion: every pair
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = positions[i][0] - positions[j][0];
                let dy = positions[i][1] - positions[j][1];
                let dist_sq = (dx * dx + dy * dy).max(MIN_DIST);
                let dist = dist_sq.sqrt();
                let force = REPULSION / dist_sq;
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                forces[i][0] += fx;
                forces[i][1] += fy;
                forces[j][0] -= fx;
                forces[j][1] -= fy;
            }
        }

        // Attraction: along links
        for (out_id, in_id) in links {
            let Some(&i) = node_indices.get(out_id) else {
                continue;
            };
            let Some(&j) = node_indices.get(in_id) else {
                continue;
            };
            let dx = positions[j][0] - positions[i][0];
            let dy = positions[j][1] - positions[i][1];
            let dist = (dx * dx + dy * dy).sqrt().max(MIN_DIST);
            let force = ATTRACTION * dist;
            let fx = force * dx / dist;
            let fy = force * dy / dist;
            forces[i][0] += fx;
            forces[i][1] += fy;
            forces[j][0] -= fx;
            forces[j][1] -= fy;
        }

        // Layer bias: sources left, sinks right
        for (i, (_id, media_class)) in nodes.iter().enumerate() {
            if let Some(mc) = media_class {
                let col = mc.layout_column();
                if col != 0 {
                    forces[i][0] += col as f32 * LAYER_BIAS;
                }
            }
        }

        // Apply forces with damping
        for i in 0..n {
            velocities[i][0] = (velocities[i][0] + forces[i][0]) * DAMPING;
            velocities[i][1] = (velocities[i][1] + forces[i][1]) * DAMPING;

            // Clamp velocity
            let speed = (velocities[i][0].powi(2) + velocities[i][1].powi(2)).sqrt();
            if speed > MAX_VELOCITY {
                let scale = MAX_VELOCITY / speed;
                velocities[i][0] *= scale;
                velocities[i][1] *= scale;
            }

            positions[i][0] += velocities[i][0];
            positions[i][1] += velocities[i][1];
        }
    }

    // Center around (0, 0)
    let cx: f32 = positions.iter().map(|p| p[0]).sum::<f32>() / n as f32;
    let cy: f32 = positions.iter().map(|p| p[1]).sum::<f32>() / n as f32;

    nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| {
            (*id, Position::new(positions[i][0] - cx, positions[i][1] - cy))
        })
        .collect()
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
