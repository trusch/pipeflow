//! Layered layout engine for audio flow graphs.
//!
//! Uses a simplified Sugiyama-style approach to produce clean left-to-right
//! layouts: sources on the left, processing in the middle, sinks on the right.
//! No overlaps, tight spacing, clean routing lines.

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
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Node width for layout calculations
    pub node_width: f32,
    /// Node height estimate for layout calculations
    pub node_height: f32,
    /// Horizontal spacing between layers
    pub horizontal_spacing: f32,
    /// Vertical spacing between nodes within a layer
    pub vertical_spacing: f32,
    /// Gap between main node and satellite/metering node
    pub satellite_gap: f32,
    /// Vertical offset for satellite nodes
    pub satellite_offset_y: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            node_width: 200.0,
            node_height: 80.0,
            horizontal_spacing: 300.0,
            vertical_spacing: 100.0,
            satellite_gap: 15.0,
            satellite_offset_y: 10.0,
        }
    }
}

/// Calculates positions for all nodes using layered layout.
///
/// Algorithm (simplified Sugiyama):
/// 1. Separate satellites (metering nodes) from regular nodes
/// 2. Build adjacency from links
/// 3. Assign layers via topological ordering
/// 4. Order nodes within layers using barycenter heuristic
/// 5. Assign (x, y) positions
/// 6. Place satellites next to their parents
pub fn layered_layout(
    nodes: &[(NodeId, Option<MediaClass>, String)],
    links: &[(NodeId, NodeId)],
    config: &LayoutConfig,
) -> HashMap<NodeId, Position> {
    if nodes.is_empty() {
        return HashMap::new();
    }

    // 1. Separate satellites from regular nodes
    let mut satellite_to_parent: HashMap<NodeId, NodeId> = HashMap::new();
    let mut regular_nodes: Vec<(NodeId, Option<&MediaClass>)> = Vec::new();

    for (id, mc, name) in nodes {
        if is_metering_node(name) {
            if let Some(parent_id) = get_metering_target_id(name) {
                satellite_to_parent.insert(*id, parent_id);
                continue;
            }
        }
        regular_nodes.push((*id, mc.as_ref()));
    }

    if regular_nodes.is_empty() {
        return HashMap::new();
    }

    let node_set: std::collections::HashSet<NodeId> =
        regular_nodes.iter().map(|(id, _)| *id).collect();

    // 2. Build adjacency (only for regular nodes present in the set)
    let mut predecessors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut successors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for &(from, to) in links {
        if node_set.contains(&from) && node_set.contains(&to) {
            successors.entry(from).or_default().push(to);
            predecessors.entry(to).or_default().push(from);
        }
    }

    // 3. Assign layers via topological walk
    //    layer[n] = max(layer[pred] + 1) for all predecessors
    //    Nodes with no predecessors start at layer 0.
    let mut layers: HashMap<NodeId, usize> = HashMap::new();

    // Topological sort using Kahn's algorithm
    let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
    for &(id, _) in &regular_nodes {
        in_degree.insert(id, predecessors.get(&id).map_or(0, |v| v.len()));
    }

    let mut queue: std::collections::VecDeque<NodeId> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    // Sort queue for determinism
    let mut queue_vec: Vec<NodeId> = queue.drain(..).collect();
    queue_vec.sort_by_key(|id| id.raw());
    queue = queue_vec.into_iter().collect();

    while let Some(node) = queue.pop_front() {
        let my_layer = predecessors
            .get(&node)
            .map(|preds| {
                preds
                    .iter()
                    .filter_map(|p| layers.get(p))
                    .max()
                    .map_or(0, |m| m + 1)
            })
            .unwrap_or(0);
        layers.insert(node, my_layer);

        if let Some(succs) = successors.get(&node) {
            for &s in succs {
                if let Some(deg) = in_degree.get_mut(&s) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(s);
                    }
                }
            }
        }
    }

    // Handle cycles: any node not yet layered gets assigned based on media class
    for &(id, mc) in &regular_nodes {
        if !layers.contains_key(&id) {
            let layer = match mc.map(|m| m.layout_column()) {
                Some(-1) => 0,
                Some(1) => 2,
                _ => 1,
            };
            layers.insert(id, layer);
        }
    }

    // Apply media class overrides:
    // Sources should be in the earliest layers, sinks in the latest
    let max_layer = layers.values().copied().max().unwrap_or(0);
    // Ensure sinks are at least at max_layer (or max_layer if max_layer is 0)
    let sink_layer = max_layer.max(2);

    for &(id, mc) in &regular_nodes {
        if let Some(media_class) = mc {
            match media_class.layout_column() {
                -1 => {
                    // Sources: force to layer 0
                    layers.insert(id, 0);
                }
                1 => {
                    // Sinks: force to sink_layer
                    layers.insert(id, sink_layer);
                }
                _ => {}
            }
        }
    }

    // Recalculate max_layer after overrides
    let max_layer = layers.values().copied().max().unwrap_or(0);

    // 4. Order nodes within layers using barycenter heuristic
    // Build layer -> ordered list of nodes
    let mut layer_nodes: Vec<Vec<NodeId>> = vec![Vec::new(); max_layer + 1];
    for &(id, _) in &regular_nodes {
        if let Some(&layer) = layers.get(&id) {
            layer_nodes[layer].push(id);
        }
    }

    // Initial ordering: sort by node ID for determinism
    for layer in &mut layer_nodes {
        layer.sort_by_key(|id| id.raw());
    }

    // Barycenter ordering: 3 passes (forward, backward, forward)
    for pass in 0..3 {
        if pass % 2 == 0 {
            // Forward sweep: order layer L based on positions in layer L-1
            for l in 1..=max_layer {
                let prev_layer = &layer_nodes[l - 1];
                let prev_positions: HashMap<NodeId, f32> = prev_layer
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| (id, i as f32))
                    .collect();

                let mut barycenters: Vec<(NodeId, f32)> = layer_nodes[l]
                    .iter()
                    .map(|&id| {
                        let connected: Vec<f32> = predecessors
                            .get(&id)
                            .map(|preds| {
                                preds
                                    .iter()
                                    .filter_map(|p| prev_positions.get(p))
                                    .copied()
                                    .collect()
                            })
                            .unwrap_or_default();

                        let bc = if connected.is_empty() {
                            f32::MAX // no connections: keep current order
                        } else {
                            connected.iter().sum::<f32>() / connected.len() as f32
                        };
                        (id, bc)
                    })
                    .collect();

                barycenters
                    .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                layer_nodes[l] = barycenters.into_iter().map(|(id, _)| id).collect();
            }
        } else {
            // Backward sweep: order layer L based on positions in layer L+1
            for l in (0..max_layer).rev() {
                let next_layer = &layer_nodes[l + 1];
                let next_positions: HashMap<NodeId, f32> = next_layer
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| (id, i as f32))
                    .collect();

                let mut barycenters: Vec<(NodeId, f32)> = layer_nodes[l]
                    .iter()
                    .map(|&id| {
                        let connected: Vec<f32> = successors
                            .get(&id)
                            .map(|succs| {
                                succs
                                    .iter()
                                    .filter_map(|s| next_positions.get(s))
                                    .copied()
                                    .collect()
                            })
                            .unwrap_or_default();

                        let bc = if connected.is_empty() {
                            f32::MAX
                        } else {
                            connected.iter().sum::<f32>() / connected.len() as f32
                        };
                        (id, bc)
                    })
                    .collect();

                barycenters
                    .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                layer_nodes[l] = barycenters.into_iter().map(|(id, _)| id).collect();
            }
        }
    }

    // 5. Assign positions
    let mut result: HashMap<NodeId, Position> = HashMap::new();

    for (layer_idx, layer) in layer_nodes.iter().enumerate() {
        let x = layer_idx as f32 * config.horizontal_spacing;
        let layer_height = layer.len() as f32 * config.vertical_spacing;
        let y_start = -layer_height / 2.0 + config.vertical_spacing / 2.0;

        for (node_idx, &id) in layer.iter().enumerate() {
            let y = y_start + node_idx as f32 * config.vertical_spacing;
            result.insert(id, Position::new(x, y));
        }
    }

    // Center the whole graph around (0, 0)
    if !result.is_empty() {
        let cx: f32 = result.values().map(|p| p.x).sum::<f32>() / result.len() as f32;
        let cy: f32 = result.values().map(|p| p.y).sum::<f32>() / result.len() as f32;
        for pos in result.values_mut() {
            pos.x -= cx;
            pos.y -= cy;
        }
    }

    // 6. Place satellites next to their parents
    for (&satellite_id, &parent_id) in &satellite_to_parent {
        if let Some(&parent_pos) = result.get(&parent_id) {
            result.insert(
                satellite_id,
                Position::new(
                    parent_pos.x + config.node_width + config.satellite_gap,
                    parent_pos.y + config.satellite_offset_y,
                ),
            );
        }
    }

    result
}

/// Places a single new node near its connections in an existing layout.
///
/// Strategy:
/// 1. Find connected nodes that are already positioned
/// 2. If connected: place downstream (right) of upstream nodes or upstream (left) of downstream
/// 3. If not connected: use media class to pick column, center vertically
/// 4. Find non-overlapping spot
pub fn place_new_node(
    node_id: NodeId,
    media_class: Option<&MediaClass>,
    graph: &GraphState,
    current_positions: &HashMap<NodeId, Position>,
    viewport_center: Position,
    config: &LayoutConfig,
) -> Position {
    // Find upstream and downstream connections
    let mut upstream_positions: Vec<Position> = Vec::new();
    let mut downstream_positions: Vec<Position> = Vec::new();

    for link in graph.links.values() {
        if link.input_node == node_id {
            // This node receives from output_node — output_node is upstream
            if let Some(&pos) = current_positions.get(&link.output_node) {
                upstream_positions.push(pos);
            }
        } else if link.output_node == node_id {
            // This node sends to input_node — input_node is downstream
            if let Some(&pos) = current_positions.get(&link.input_node) {
                downstream_positions.push(pos);
            }
        }
    }

    let has_connections = !upstream_positions.is_empty() || !downstream_positions.is_empty();

    let base_position = if has_connections {
        // Place based on connection topology
        let all_connected: Vec<Position> = upstream_positions
            .iter()
            .chain(downstream_positions.iter())
            .copied()
            .collect();

        let avg_y = all_connected.iter().map(|p| p.y).sum::<f32>() / all_connected.len() as f32;

        let x = if !upstream_positions.is_empty() {
            // Place to the right of upstream nodes
            let max_upstream_x = upstream_positions
                .iter()
                .map(|p| p.x)
                .fold(f32::NEG_INFINITY, f32::max);
            max_upstream_x + config.horizontal_spacing
        } else {
            // Place to the left of downstream nodes
            let min_downstream_x = downstream_positions
                .iter()
                .map(|p| p.x)
                .fold(f32::INFINITY, f32::min);
            min_downstream_x - config.horizontal_spacing
        };

        Position::new(x, avg_y)
    } else {
        // No connections — use media class for column placement
        let column = media_class.map(|mc| mc.layout_column()).unwrap_or(0);

        // Find the X range of existing nodes to place in appropriate column
        let (min_x, max_x) = if current_positions.is_empty() {
            (0.0, 0.0)
        } else {
            let xs: Vec<f32> = current_positions.values().map(|p| p.x).collect();
            (
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        };

        let x = match column {
            -1 => min_x - config.horizontal_spacing / 2.0,
            1 => max_x + config.horizontal_spacing / 2.0,
            _ => viewport_center.x,
        };

        Position::new(x, viewport_center.y)
    };

    // Find non-overlapping spot
    find_free_spot(base_position, current_positions, config)
}

/// Finds a non-overlapping position near the target.
fn find_free_spot(
    target: Position,
    current_positions: &HashMap<NodeId, Position>,
    config: &LayoutConfig,
) -> Position {
    let min_distance = (config.node_width.powi(2) + config.node_height.powi(2)).sqrt() * 0.6;

    let grid = crate::util::spatial::SpatialGrid::from_positions(
        min_distance,
        current_positions.values().copied(),
    );

    if !grid.has_neighbor_within(target, min_distance) {
        return target;
    }

    // Try positions in expanding vertical offsets (most natural for column layout)
    for radius in 1..30 {
        let offset = radius as f32 * config.vertical_spacing;

        let below = Position::new(target.x, target.y + offset);
        if !grid.has_neighbor_within(below, min_distance) {
            return below;
        }

        let above = Position::new(target.x, target.y - offset);
        if !grid.has_neighbor_within(above, min_distance) {
            return above;
        }
    }

    // Fallback
    Position::new(target.x, target.y + config.vertical_spacing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.horizontal_spacing, 300.0);
        assert_eq!(config.vertical_spacing, 100.0);
        assert_eq!(config.node_width, 200.0);
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
    fn test_layered_layout_empty() {
        let result = layered_layout(&[], &[], &LayoutConfig::default());
        assert!(result.is_empty());
    }

    #[test]
    fn test_layered_layout_single_node() {
        let nodes = vec![(NodeId::new(1), None, "test".to_string())];
        let result = layered_layout(&nodes, &[], &LayoutConfig::default());
        assert_eq!(result.len(), 1);
        // Single node should be centered at origin
        let pos = result.get(&NodeId::new(1)).unwrap();
        assert!((pos.x).abs() < 1.0);
        assert!((pos.y).abs() < 1.0);
    }

    #[test]
    fn test_layered_layout_source_sink_ordering() {
        let nodes = vec![
            (
                NodeId::new(1),
                Some(MediaClass::AudioSource),
                "source".to_string(),
            ),
            (
                NodeId::new(2),
                Some(MediaClass::AudioSink),
                "sink".to_string(),
            ),
        ];
        let links = vec![(NodeId::new(1), NodeId::new(2))];
        let result = layered_layout(&nodes, &links, &LayoutConfig::default());

        let source_pos = result.get(&NodeId::new(1)).unwrap();
        let sink_pos = result.get(&NodeId::new(2)).unwrap();
        // Source should be to the left of sink
        assert!(source_pos.x < sink_pos.x);
    }

    #[test]
    fn test_layered_layout_three_layer_chain() {
        let nodes = vec![
            (
                NodeId::new(1),
                Some(MediaClass::AudioSource),
                "source".to_string(),
            ),
            (NodeId::new(2), None, "filter".to_string()),
            (
                NodeId::new(3),
                Some(MediaClass::AudioSink),
                "sink".to_string(),
            ),
        ];
        let links = vec![
            (NodeId::new(1), NodeId::new(2)),
            (NodeId::new(2), NodeId::new(3)),
        ];
        let result = layered_layout(&nodes, &links, &LayoutConfig::default());

        let p1 = result.get(&NodeId::new(1)).unwrap();
        let p2 = result.get(&NodeId::new(2)).unwrap();
        let p3 = result.get(&NodeId::new(3)).unwrap();

        // Left to right ordering
        assert!(p1.x < p2.x);
        assert!(p2.x < p3.x);
    }

    #[test]
    fn test_layered_layout_no_overlap() {
        let nodes: Vec<_> = (0..10)
            .map(|i| (NodeId::new(i), None, format!("node-{}", i)))
            .collect();
        let links: Vec<_> = (0..9)
            .map(|i| (NodeId::new(i), NodeId::new(i + 1)))
            .collect();
        let config = LayoutConfig::default();
        let result = layered_layout(&nodes, &links, &config);

        let min_distance = (config.node_width.powi(2) + config.node_height.powi(2)).sqrt() * 0.5;

        let positions: Vec<Position> = result.values().copied().collect();
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let dist = positions[i].distance_to(&positions[j]);
                assert!(
                    dist >= min_distance,
                    "Nodes overlap: distance {} < min {}",
                    dist,
                    min_distance
                );
            }
        }
    }

    #[test]
    fn test_layered_layout_satellites() {
        let nodes = vec![
            (
                NodeId::new(1),
                Some(MediaClass::AudioSource),
                "source".to_string(),
            ),
            (NodeId::new(2), None, "pipeflow-meter-1".to_string()),
        ];
        let config = LayoutConfig::default();
        let result = layered_layout(&nodes, &[], &config);

        assert_eq!(result.len(), 2);
        let parent = result.get(&NodeId::new(1)).unwrap();
        let satellite = result.get(&NodeId::new(2)).unwrap();

        // Satellite should be to the right of parent
        assert!(satellite.x > parent.x);
        let expected_x = parent.x + config.node_width + config.satellite_gap;
        assert!((satellite.x - expected_x).abs() < 1.0);
    }

    #[test]
    fn test_layered_layout_centered() {
        let nodes: Vec<_> = (0..5)
            .map(|i| (NodeId::new(i), None, format!("node-{}", i)))
            .collect();
        let links: Vec<_> = (0..4)
            .map(|i| (NodeId::new(i), NodeId::new(i + 1)))
            .collect();
        let result = layered_layout(&nodes, &links, &LayoutConfig::default());

        // Graph should be roughly centered around (0, 0)
        let cx: f32 = result.values().map(|p| p.x).sum::<f32>() / result.len() as f32;
        let cy: f32 = result.values().map(|p| p.y).sum::<f32>() / result.len() as f32;
        assert!(cx.abs() < 1.0, "Center X {} not near 0", cx);
        assert!(cy.abs() < 1.0, "Center Y {} not near 0", cy);
    }

    #[test]
    fn test_place_new_node_no_connections() {
        let graph = GraphState::default();
        let positions = HashMap::new();
        let viewport_center = Position::new(100.0, 200.0);
        let config = LayoutConfig::default();

        let pos = place_new_node(
            NodeId::new(1),
            None,
            &graph,
            &positions,
            viewport_center,
            &config,
        );

        // Should be near viewport center
        assert!((pos.x - viewport_center.x).abs() < 300.0);
        assert!((pos.y - viewport_center.y).abs() < 300.0);
    }
}
