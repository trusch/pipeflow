//! Shared graph-view types and responses.

use crate::util::id::{LinkId, NodeId, PortId};
use egui::{Pos2, Vec2};
use std::collections::HashMap;

/// Level of detail for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LodLevel {
    /// Minimal rendering - just colored rectangles
    Minimal,
    /// Low detail - ports but no labels
    Low,
    /// Full detail - everything
    Full,
}

/// Target for context menu (locked when menu opens).
#[derive(Debug, Clone, Copy)]
pub(super) enum ContextMenuTarget {
    None,
    Link(LinkId),
    Background,
}

/// State of a connection being dragged.
#[derive(Debug, Clone)]
pub(super) struct ConnectionDrag {
    /// Starting port
    pub(super) from_port: PortId,
    /// Starting port direction
    pub(super) from_direction: crate::domain::graph::PortDirection,
    /// Current mouse position
    pub(super) current_pos: Pos2,
}

/// Graph view state.
#[derive(Debug, Clone)]
pub struct GraphView {
    /// Current zoom level
    pub zoom: f32,
    /// Pan offset
    pub pan: Vec2,
    /// Connection being created
    pub(super) creating_connection: Option<ConnectionDrag>,
    /// Hovered node
    pub(super) hovered_node: Option<NodeId>,
    /// Hovered port
    pub(super) hovered_port: Option<PortId>,
    /// Hovered link
    pub(super) hovered_link: Option<LinkId>,
    /// Box selection state (start position in screen coords)
    pub(super) box_selection_start: Option<Pos2>,
    /// Context menu target (locked when menu opens)
    pub(super) context_menu_target: ContextMenuTarget,
    /// Per-node opacity for fade-in animation (0.0 = invisible, 1.0 = fully visible)
    pub(super) node_alphas: HashMap<NodeId, f32>,
    /// Whether the minimap is currently being dragged
    pub(super) minimap_dragging: bool,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            creating_connection: None,
            hovered_node: None,
            hovered_port: None,
            hovered_link: None,
            box_selection_start: None,
            context_menu_target: ContextMenuTarget::None,
            node_alphas: HashMap::new(),
            minimap_dragging: false,
        }
    }
}

impl GraphView {
    /// Creates a new graph view.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true while node fade animations are still settling.
    pub fn is_animating(&self) -> bool {
        self.node_alphas.values().any(|&alpha| alpha < 1.0)
    }
}

/// Response from the graph view.
#[derive(Debug, Default)]
pub struct GraphViewResponse {
    /// Node that was clicked
    pub clicked_node: Option<NodeId>,
    /// Node being dragged and the delta
    pub dragged_node: Option<(NodeId, Vec2)>,
    /// Port where connection started
    pub started_connection: Option<PortId>,
    /// Connection completed (output_port, input_port)
    pub completed_connection: Option<(PortId, PortId)>,
    /// Link that was clicked
    pub clicked_link: Option<LinkId>,
    /// Link to remove (from context menu)
    pub remove_link: Option<LinkId>,
    /// Link to toggle (link_id, new_active_state)
    pub toggle_link: Option<(LinkId, bool)>,
    /// Background was clicked
    pub clicked_background: bool,
    /// Nodes selected by box selection
    pub box_selected_nodes: Vec<NodeId>,
    /// Whether box selection should add to existing selection (shift held)
    pub box_selection_additive: bool,
    /// Snap to grid requested (None = all nodes, Some = specific nodes)
    pub snap_to_grid: Option<Option<Vec<NodeId>>>,
    /// Toggle uninteresting status for nodes
    pub toggle_uninteresting: Option<Vec<NodeId>>,
    /// Save node's connections as a rule
    pub save_connections_as_rule: Option<NodeId>,
    /// Request to rename a node (opens rename dialog)
    pub rename_node: Option<NodeId>,
}
