//! Graph visualization using egui-snarl.
//!
//! Renders the PipeWire graph with nodes, ports, and connections.

use crate::core::state::{GraphState, LayerVisibility};
use crate::domain::explain::explain_node_short;
use crate::domain::filters::FilterSet;
use crate::util::is_metering_node;

/// Truncates a string to fit within a maximum width in pixels using actual font measurement.
/// Uses smart truncation: shows beginning and end of text (e.g., "playback_F..._FL")
/// to keep distinguishing characters visible.
fn truncate_text_measured(
    text: &str,
    max_width: f32,
    font_id: &egui::FontId,
    fonts: &mut egui::epaint::text::FontsView<'_>,
) -> String {
    if max_width <= 0.0 {
        return String::new();
    }

    // Measure full text width
    let full_width = measure_text_width(text, font_id, fonts);
    if full_width <= max_width {
        return text.to_string();
    }

    let text_chars: Vec<char> = text.chars().collect();
    let text_len = text_chars.len();

    if text_len <= 2 {
        return text.to_string();
    }

    // Measure ellipsis width
    let ellipsis = "..";
    let ellipsis_width = measure_text_width(ellipsis, font_id, fonts);

    // If ellipsis alone doesn't fit, return empty or minimal
    if ellipsis_width >= max_width {
        return String::new();
    }

    let available_width = max_width - ellipsis_width;

    // Use binary search to find optimal split
    // We want to maximize (start_chars + end_chars) such that
    // width(start) + width(end) <= available_width
    // Split roughly 60/40 favoring the start for context
    let mut best_start = 0usize;
    let mut best_end = 0usize;

    // Try different start lengths, then find matching end length
    for start_chars in (1..=text_len.saturating_sub(1)).rev() {
        let start: String = text_chars[..start_chars].iter().collect();
        let start_width = measure_text_width(&start, font_id, fonts);

        if start_width > available_width {
            continue;
        }

        let remaining_width = available_width - start_width;

        // Find how many end chars fit in remaining width
        for end_chars in (0..=text_len.saturating_sub(start_chars)).rev() {
            if end_chars == 0 {
                // Just start + ellipsis
                if start_chars > best_start + best_end {
                    best_start = start_chars;
                    best_end = 0;
                }
                break;
            }

            let end: String = text_chars[text_len - end_chars..].iter().collect();
            let end_width = measure_text_width(&end, font_id, fonts);

            if end_width <= remaining_width {
                let total_chars = start_chars + end_chars;
                if total_chars > best_start + best_end {
                    best_start = start_chars;
                    best_end = end_chars;
                }
                break;
            }
        }

        // Early exit if we found a good solution with most chars
        if best_start + best_end >= text_len.saturating_sub(2) {
            break;
        }
    }

    if best_start == 0 && best_end == 0 {
        return ellipsis.to_string();
    }

    let start: String = text_chars[..best_start].iter().collect();
    if best_end == 0 {
        format!("{}{}", start, ellipsis)
    } else {
        let end: String = text_chars[text_len - best_end..].iter().collect();
        format!("{}{}{}", start, ellipsis, end)
    }
}

/// Measures the width of text using egui's font system.
#[inline]
fn measure_text_width(text: &str, font_id: &egui::FontId, fonts: &mut egui::epaint::text::FontsView<'_>) -> f32 {
    let job = egui::text::LayoutJob::simple_singleline(
        text.to_string(),
        font_id.clone(),
        egui::Color32::WHITE,
    );
    fonts.layout_job(job).rect.width()
}

use crate::domain::graph::{Link, Node, Port, PortDirection};
use crate::ui::theme::Theme;
use crate::util::id::{LinkId, NodeId, PortId};
use crate::util::spatial::Position;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::{HashMap, HashSet};

/// Level of detail for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LodLevel {
    /// Minimal rendering - just colored rectangles
    Minimal,
    /// Low detail - ports but no labels
    Low,
    /// Full detail - everything
    Full,
}

/// Target for context menu (locked when menu opens).
#[derive(Debug, Clone, Copy)]
enum ContextMenuTarget {
    None,
    Link(LinkId),
    Background,
}

/// Graph view state.
#[derive(Debug, Clone)]
pub struct GraphView {
    /// Current zoom level
    pub zoom: f32,
    /// Pan offset
    pub pan: Vec2,
    /// Connection being created
    creating_connection: Option<ConnectionDrag>,
    /// Hovered node
    hovered_node: Option<NodeId>,
    /// Hovered port
    hovered_port: Option<PortId>,
    /// Hovered link
    hovered_link: Option<LinkId>,
    /// Box selection state (start position in screen coords)
    box_selection_start: Option<Pos2>,
    /// Context menu target (locked when menu opens)
    context_menu_target: ContextMenuTarget,
}

/// State of a connection being dragged.
#[derive(Debug, Clone)]
struct ConnectionDrag {
    /// Starting port
    from_port: PortId,
    /// Starting port direction
    from_direction: PortDirection,
    /// Current mouse position
    current_pos: Pos2,
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
        }
    }
}

impl GraphView {
    /// Creates a new graph view.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculates the screen position of a port.
    /// This is the single source of truth for port positions.
    fn get_port_screen_position(
        &self,
        port_id: &PortId,
        graph: &GraphState,
        node_positions: &HashMap<NodeId, Position>,
        transform: &GraphTransform,
        theme: &Theme,
    ) -> Option<Pos2> {
        let port = graph.get_port(port_id)?;
        let node = graph.get_node(&port.node_id)?;
        let node_pos = node_positions.get(&port.node_id).copied().unwrap_or(Position::zero());

        // Check if this is a meter node (uses smaller width)
        let is_meter = is_metering_node(&node.name);
        let node_width = if is_meter {
            theme.sizes.node_width * 0.55
        } else {
            theme.sizes.node_width
        };

        // Get all ports for this node and find this port's index among ports of the same direction
        // Ports are sorted alphabetically by name for consistent ordering
        let all_ports = graph.ports_for_node(&port.node_id);
        let mut same_direction_ports: Vec<_> = all_ports
            .iter()
            .filter(|p| p.direction == port.direction)
            .collect();
        same_direction_ports.sort_by(|a, b| a.name.cmp(&b.name));

        let port_index = same_direction_ports
            .iter()
            .position(|p| p.id == port.id)
            .unwrap_or(0);

        // Check if node has meter data (affects port Y offset)
        let meter_data = graph.meters.get(&port.node_id);
        let has_meter = meter_data.map(|m| m.max_peak() > 0.0).unwrap_or(false);
        let meter_height = if has_meter { 8.0 } else { 0.0 };

        // Calculate Y position: header + meter + port offset
        let port_start_y = node_pos.y + theme.sizes.node_header_height + meter_height + 4.0;
        let port_y = port_start_y + (port_index as f32 * theme.sizes.port_height);

        // Calculate X position based on direction
        let is_input = port.direction == PortDirection::Input;
        let port_x = if is_input {
            node_pos.x + theme.sizes.port_radius
        } else {
            node_pos.x + node_width - theme.sizes.port_radius
        };

        // Convert to screen coordinates
        let screen_pos = transform.graph_to_screen(Pos2::new(port_x, port_y + theme.sizes.port_height * 0.5));
        Some(screen_pos)
    }

    /// Renders the graph view.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        graph: &GraphState,
        node_positions: &HashMap<NodeId, Position>,
        selected_nodes: &HashSet<NodeId>,
        selected_link: Option<LinkId>,
        uninteresting_nodes: &HashSet<NodeId>,
        custom_names: &HashMap<NodeId, String>,
        hide_uninteresting: bool,
        layer_visibility: &LayerVisibility,
        filters: &FilterSet,
        ports: &HashMap<PortId, Port>,
        theme: &Theme,
    ) -> GraphViewResponse {
        let mut response = GraphViewResponse::default();

        // Get available space
        let available_rect = ui.available_rect_before_wrap();

        // Handle zoom with scroll wheel
        let scroll_delta = ui.input(|i| i.raw_scroll_delta);
        if scroll_delta.y != 0.0 {
            let zoom_delta = scroll_delta.y * 0.001;
            self.zoom = (self.zoom + zoom_delta).clamp(0.1, 3.0);
        }

        // Allocate the full area
        let (rect, canvas_response) =
            ui.allocate_exact_size(available_rect.size(), Sense::click_and_drag());

        // Track mouse position
        let mouse_pos = ui.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO));

        // Handle panning with middle mouse, right-click drag, or shift+left
        if canvas_response.dragged_by(egui::PointerButton::Middle)
            || canvas_response.dragged_by(egui::PointerButton::Secondary)
            || (canvas_response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|i| i.modifiers.shift))
        {
            self.pan += canvas_response.drag_delta();
        }

        // Handle box selection with left mouse (when not over a node)
        if canvas_response.drag_started_by(egui::PointerButton::Primary)
            && self.hovered_node.is_none()
            && self.hovered_port.is_none()
            && !ui.input(|i| i.modifiers.shift)
        {
            self.box_selection_start = Some(mouse_pos);
        }

        // Update or cancel box selection
        if !canvas_response.dragged_by(egui::PointerButton::Primary) {
            if let Some(start) = self.box_selection_start.take() {
                // Selection completed - calculate which nodes are inside
                let selection_rect = Rect::from_two_pos(start, mouse_pos);
                // We'll compute selected nodes after drawing
                response.box_selected_nodes = self.compute_box_selection(
                    selection_rect,
                    graph,
                    node_positions,
                    &GraphTransform::new(rect.center(), self.zoom, self.pan),
                    theme,
                );
            }
        }

        // Update creating connection with current mouse position
        if let Some(ref mut drag) = self.creating_connection {
            drag.current_pos = mouse_pos;
        }

        // Draw background
        ui.painter()
            .rect_filled(rect, 0.0, theme.background.primary);

        // Draw grid
        self.draw_grid(ui, rect, theme);

        // Transform for zoom and pan
        let transform = GraphTransform::new(rect.center(), self.zoom, self.pan);

        // Reset hovered link state before drawing links
        self.hovered_link = None;

        // Draw links first (below nodes)
        for link in graph.links.values() {
            // Check if either endpoint is uninteresting
            let output_uninteresting = uninteresting_nodes.contains(&link.output_node);
            let input_uninteresting = uninteresting_nodes.contains(&link.input_node);
            let is_link_uninteresting = output_uninteresting || input_uninteresting;

            // Skip links connected to hidden uninteresting nodes
            if hide_uninteresting && is_link_uninteresting {
                continue;
            }

            // Skip links where either endpoint's layer is hidden
            {
                let output_layer_hidden = graph
                    .nodes
                    .get(&link.output_node)
                    .map(|n| !layer_visibility.is_visible(n.layer))
                    .unwrap_or(false);
                let input_layer_hidden = graph
                    .nodes
                    .get(&link.input_node)
                    .map(|n| !layer_visibility.is_visible(n.layer))
                    .unwrap_or(false);

                if output_layer_hidden || input_layer_hidden {
                    continue;
                }
            }

            // Skip links where endpoints don't match active filters
            if !filters.is_empty() {
                let output_node = graph.nodes.get(&link.output_node);
                let input_node = graph.nodes.get(&link.input_node);

                let output_filtered = output_node
                    .map(|n| !filters.matches_with_ports(n, ports))
                    .unwrap_or(true);
                let input_filtered = input_node
                    .map(|n| !filters.matches_with_ports(n, ports))
                    .unwrap_or(true);

                if output_filtered || input_filtered {
                    continue;
                }
            }

            let is_selected = selected_link == Some(link.id);
            self.draw_link(ui, link, graph, node_positions, &transform, theme, mouse_pos, is_selected, is_link_uninteresting);
        }

        // Reset hovered state before drawing nodes
        self.hovered_node = None;
        self.hovered_port = None;

        // Calculate expanded viewport for culling (include margin for partially visible nodes)
        let viewport = rect.expand(theme.sizes.node_width * self.zoom + 50.0);

        // Draw nodes with viewport culling
        // First pass: regular nodes, Second pass: meter nodes (drawn on top)
        let mut meter_nodes = Vec::new();

        for node in graph.nodes.values() {
            let is_uninteresting = uninteresting_nodes.contains(&node.id);

            // Skip uninteresting nodes if hiding is enabled
            if hide_uninteresting && is_uninteresting {
                continue;
            }

            // Skip nodes whose layer is hidden
            if !layer_visibility.is_visible(node.layer) {
                continue;
            }

            // Skip nodes that don't match active filters
            if !filters.is_empty() && !filters.matches_with_ports(node, ports) {
                continue;
            }

            // Get node position and check if visible
            let pos = node_positions
                .get(&node.id)
                .copied()
                .unwrap_or(Position::zero());
            let screen_pos = transform.graph_to_screen(Pos2::new(pos.x, pos.y));

            // Skip nodes outside viewport
            if !viewport.contains(screen_pos) {
                continue;
            }

            let is_meter = is_metering_node(&node.name);
            if is_meter {
                // Defer meter nodes to second pass
                meter_nodes.push(node);
                continue;
            }

            let is_selected = selected_nodes.contains(&node.id);
            self.draw_node(
                ui,
                node,
                graph,
                node_positions,
                is_selected,
                is_uninteresting,
                false, // is_meter
                &transform,
                theme,
                &mut response,
                uninteresting_nodes,
                custom_names,
            );
        }

        // Second pass: draw meter nodes on top
        for node in meter_nodes {
            let is_selected = selected_nodes.contains(&node.id);
            let is_uninteresting = uninteresting_nodes.contains(&node.id);
            self.draw_node(
                ui,
                node,
                graph,
                node_positions,
                is_selected,
                is_uninteresting,
                true, // is_meter
                &transform,
                theme,
                &mut response,
                uninteresting_nodes,
                custom_names,
            );
        }

        // Handle connection started from a port
        if let Some(port_id) = response.started_connection.take() {
            if let Some(port) = graph.get_port(&port_id) {
                self.creating_connection = Some(ConnectionDrag {
                    from_port: port_id,
                    from_direction: port.direction,
                    current_pos: mouse_pos,
                });
            }
        }

        // Draw connection being created (AFTER nodes, so it appears on top)
        if let Some(ref drag) = self.creating_connection {
            // Snap to hovered port if compatible
            let snap_target = if let Some(port_id) = self.hovered_port {
                let compatible = graph.get_port(&port_id)
                    .and_then(|target_port| {
                        let from_port = graph.get_port(&drag.from_port)?;
                        if from_port.can_connect_to(target_port) {
                            Some(())
                        } else {
                            None
                        }
                    })
                    .is_some();

                if compatible {
                    // Use unified port position calculation
                    self.get_port_screen_position(&port_id, graph, node_positions, &transform, theme)
                } else {
                    None
                }
            } else {
                None
            };

            self.draw_creating_connection_with_snap(
                ui,
                drag,
                snap_target,
                graph,
                node_positions,
                &transform,
                theme,
            );
        }

        // Draw box selection rectangle
        if let Some(start) = self.box_selection_start {
            let selection_rect = Rect::from_two_pos(start, mouse_pos);
            ui.painter().rect(
                selection_rect,
                0.0,
                Color32::from_rgba_unmultiplied(100, 150, 255, 40),
                Stroke::new(1.0, Color32::from_rgb(100, 150, 255)),
                egui::StrokeKind::Outside,
            );
        }

        // Handle connection completion when mouse is released
        let pointer_released = ui.input(|i| i.pointer.any_released());
        if pointer_released {
            if let Some(drag) = self.creating_connection.take() {
                // Check if we're over a compatible port
                if let Some(target_port_id) = self.hovered_port {
                    if let Some(target_port) = graph.get_port(&target_port_id) {
                        if let Some(from_port) = graph.get_port(&drag.from_port) {
                            // Check compatibility: opposite directions, different nodes
                            if from_port.can_connect_to(target_port) {
                                // Determine output -> input order
                                let (output, input) = if drag.from_direction == PortDirection::Output {
                                    (drag.from_port, target_port_id)
                                } else {
                                    (target_port_id, drag.from_port)
                                };
                                response.completed_connection = Some((output, input));
                            }
                        }
                    }
                }
            }
        }

        // Handle click to deselect
        if canvas_response.clicked() && self.hovered_node.is_none() && self.hovered_link.is_none() {
            response.clicked_background = true;
        }

        // Handle left-click on link to select it
        if canvas_response.clicked() && self.hovered_link.is_some() {
            response.clicked_link = self.hovered_link;
        }

        // Lock context menu target on right-click for links and background
        // (Node context menus are handled directly in draw_node)
        if canvas_response.secondary_clicked() {
            self.context_menu_target = if let Some(link_id) = self.hovered_link {
                ContextMenuTarget::Link(link_id)
            } else {
                ContextMenuTarget::Background
            };
        }

        // Handle right-click context menu for background and links
        canvas_response.context_menu(|ui| {
            self.show_context_menu(ui, graph, uninteresting_nodes, &mut response);
        });

        response
    }

    /// Shows the context menu for the graph (links and background only - nodes have their own menu).
    fn show_context_menu(&self, ui: &mut Ui, graph: &GraphState, _uninteresting_nodes: &HashSet<NodeId>, response: &mut GraphViewResponse) {
        match self.context_menu_target {
            ContextMenuTarget::Link(link_id) => {
                // Get link info to show current state
                let link = graph.get_link(&link_id);
                let is_active = link.map(|l| l.is_active).unwrap_or(true);

                // Get link endpoint names for better display
                let link_desc = if let Some(link) = link {
                    let out_name = graph.get_node(&link.output_node)
                        .map(|n| n.display_name().to_string())
                        .unwrap_or_else(|| "?".to_string());
                    let in_name = graph.get_node(&link.input_node)
                        .map(|n| n.display_name().to_string())
                        .unwrap_or_else(|| "?".to_string());
                    format!("{} -> {}", out_name, in_name)
                } else {
                    format!("Link {:?}", link_id)
                };

                ui.heading("Connection");
                ui.label(&link_desc);
                ui.separator();

                // Toggle active state
                let toggle_label = if is_active { "Disable Link" } else { "Enable Link" };
                if ui.button(toggle_label).clicked() {
                    response.toggle_link = Some((link_id, !is_active));
                    ui.close();
                }

                if ui.button("Remove Link").clicked() {
                    response.remove_link = Some(link_id);
                    ui.close();
                }
            }
            ContextMenuTarget::Background | ContextMenuTarget::None => {
                // Node context menus are handled directly in draw_node
                ui.label("Graph");
                ui.separator();
                if ui.button("Reset View").clicked() {
                    // This would need to be handled by caller
                    ui.close();
                }
                if ui.button("Snap All to Grid").clicked() {
                    response.snap_to_grid = Some(None); // None = all nodes
                    ui.close();
                }
            }
        }
    }

    /// Draws the background grid.
    fn draw_grid(&self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        let spacing = theme.sizes.grid_spacing * self.zoom;
        if spacing < 5.0 {
            return; // Too zoomed out to show grid
        }

        let painter = ui.painter();
        let stroke = Stroke::new(1.0, theme.background.grid);

        // Offset based on pan
        let offset_x = self.pan.x % spacing;
        let offset_y = self.pan.y % spacing;

        // Vertical lines
        let mut x = rect.left() + offset_x;
        while x < rect.right() {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], stroke);
            x += spacing;
        }

        // Horizontal lines
        let mut y = rect.top() + offset_y;
        while y < rect.bottom() {
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
            y += spacing;
        }
    }

    /// Draws a node.
    #[allow(clippy::too_many_arguments)]
    fn draw_node(
        &mut self,
        ui: &mut Ui,
        node: &Node,
        graph: &GraphState,
        positions: &HashMap<NodeId, Position>,
        is_selected: bool,
        is_uninteresting: bool,
        is_meter: bool,
        transform: &GraphTransform,
        theme: &Theme,
        response: &mut GraphViewResponse,
        uninteresting_nodes: &HashSet<NodeId>,
        custom_names: &HashMap<NodeId, String>,
    ) {
        // Use smaller width for meter nodes
        let node_width = if is_meter {
            theme.sizes.node_width * 0.55  // Meter nodes are 55% of normal width
        } else {
            theme.sizes.node_width
        };
        let pos = positions
            .get(&node.id)
            .copied()
            .unwrap_or(Position::zero());
        let screen_pos = transform.graph_to_screen(Pos2::new(pos.x, pos.y));

        let ports = graph.ports_for_node(&node.id);
        let mut input_ports: Vec<_> = ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
            .collect();
        input_ports.sort_by(|a, b| a.name.cmp(&b.name));
        let mut output_ports: Vec<_> = ports
            .iter()
            .filter(|p| p.direction == PortDirection::Output)
            .collect();
        output_ports.sort_by(|a, b| a.name.cmp(&b.name));

        // Check if we have meter data for this node
        let meter_data = graph.meters.get(&node.id);
        let has_meter = meter_data.map(|m| m.max_peak() > 0.0).unwrap_or(false);
        let meter_height = if has_meter { 8.0 } else { 0.0 };

        let max_ports = input_ports.len().max(output_ports.len());
        let node_height = theme.sizes.node_header_height
            + meter_height
            + (max_ports as f32 * theme.sizes.port_height)
            + 8.0;

        let node_rect = Rect::from_min_size(
            screen_pos,
            Vec2::new(
                node_width * self.zoom,
                node_height * self.zoom,
            ),
        );

        let painter = ui.painter();

        // LOD: Determine detail level based on zoom
        let lod = if self.zoom < 0.25 {
            LodLevel::Minimal // Just colored rectangle
        } else if self.zoom < 0.5 {
            LodLevel::Low // No port labels
        } else {
            LodLevel::Full // Everything
        };

        // Helper to dim colors for uninteresting nodes
        let dim_color = |color: Color32| -> Color32 {
            if is_uninteresting {
                // Reduce saturation and brightness for greyed out appearance
                let r = (color.r() as f32 * 0.4 + 40.0) as u8;
                let g = (color.g() as f32 * 0.4 + 40.0) as u8;
                let b = (color.b() as f32 * 0.4 + 40.0) as u8;
                let a = (color.a() as f32 * 0.6) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, a)
            } else {
                color
            }
        };

        // Draw subtle drop shadow (only at higher LOD levels and for non-uninteresting nodes)
        if lod != LodLevel::Minimal && !is_uninteresting {
            let shadow_offset = 3.0 * self.zoom;
            let shadow_rect = Rect::from_min_size(
                Pos2::new(screen_pos.x + shadow_offset, screen_pos.y + shadow_offset),
                node_rect.size(),
            );
            // Soft shadow with multiple layers for smooth appearance
            let shadow_alpha = if is_selected { 60 } else { 40 };
            let outer_rounding = theme.sizes.node_rounding + 2.0;
            painter.rect_filled(
                shadow_rect.expand(2.0 * self.zoom),
                egui::CornerRadius::same(outer_rounding as u8),
                Color32::from_rgba_unmultiplied(0, 0, 0, shadow_alpha / 2),
            );
            painter.rect_filled(
                shadow_rect,
                theme.sizes.node_rounding(),
                Color32::from_rgba_unmultiplied(0, 0, 0, shadow_alpha),
            );
        }

        // Draw selection glow effect (animated pulse)
        if is_selected && !is_uninteresting {
            let time = ui.input(|i| i.time);
            let pulse = ((time * 2.0).sin() * 0.3 + 0.7) as f32; // Gentle pulse between 0.4 and 1.0
            let glow_alpha = (80.0 * pulse) as u8;
            let glow_expand = 4.0 * self.zoom * pulse;
            let glow_rounding = theme.sizes.node_rounding + glow_expand;

            painter.rect_filled(
                node_rect.expand(glow_expand),
                egui::CornerRadius::same(glow_rounding as u8),
                Color32::from_rgba_unmultiplied(100, 150, 255, glow_alpha / 2),
            );
        }

        // Draw subtle hover highlight (using previous frame's hover state)
        let is_hovered = self.hovered_node == Some(node.id);
        if is_hovered && !is_selected && !is_uninteresting {
            let hover_expand = 2.0 * self.zoom;
            let hover_rounding = theme.sizes.node_rounding + hover_expand;
            painter.rect_filled(
                node_rect.expand(hover_expand),
                egui::CornerRadius::same(hover_rounding as u8),
                Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            );
        }

        // Node background
        let bg_color = if is_selected {
            dim_color(theme.node.background_selected)
        } else {
            dim_color(theme.node.background)
        };
        painter.rect_filled(node_rect, theme.sizes.node_rounding(), bg_color);

        // Node border
        let border_color = if is_selected {
            dim_color(theme.node.border_selected)
        } else {
            dim_color(theme.node.border)
        };
        let border_width = if is_selected { 2.0 } else { 1.5 };
        painter.rect_stroke(
            node_rect,
            theme.sizes.node_rounding(),
            Stroke::new(border_width, border_color),
            egui::StrokeKind::Outside,
        );

        // Minimal LOD: just show colored rectangle with header color strip
        if lod == LodLevel::Minimal {
            let header_rect = Rect::from_min_size(
                screen_pos,
                Vec2::new(
                    node_width * self.zoom,
                    theme.sizes.node_header_height * self.zoom,
                ),
            );
            let header_color = dim_color(theme.header_color_for_media_class(node.media_class.as_ref()));
            painter.rect_filled(header_rect, theme.sizes.node_rounding(), header_color);

            // Handle node interaction (always needed)
            let node_response = ui.interact(node_rect, ui.id().with(node.id.raw()), Sense::click_and_drag());
            if node_response.hovered() {
                self.hovered_node = Some(node.id);
            }
            if node_response.clicked() {
                response.clicked_node = Some(node.id);
            }
            // Allow dragging any node for a natural feel (at minimal LOD, no ports visible anyway)
            if node_response.dragged() {
                response.dragged_node = Some((node.id, node_response.drag_delta() / self.zoom));
            }

            // Handle right-click context menu on node
            let node_id = node.id;
            node_response.context_menu(|ui| {
                Self::show_node_context_menu(ui, node_id, graph, uninteresting_nodes, custom_names, response);
            });

            return;
        }

        // Header
        let header_rect = Rect::from_min_size(
            screen_pos,
            Vec2::new(
                node_width * self.zoom,
                theme.sizes.node_header_height * self.zoom,
            ),
        );
        let header_color = dim_color(theme.header_color_for_media_class(node.media_class.as_ref()));
        painter.rect_filled(
            header_rect,
            egui::CornerRadius {
                nw: theme.sizes.node_rounding as u8,
                ne: theme.sizes.node_rounding as u8,
                sw: 0,
                se: 0,
            },
            header_color,
        );

        // Node name (truncated to fit), prioritizing custom name if set
        let name = custom_names
            .get(&node.id)
            .map(|s| s.as_str())
            .unwrap_or_else(|| node.display_name());
        // Use smaller font for meter nodes
        let base_font_size = if is_meter { 9.0 } else { 12.0 };
        let font_size = base_font_size * self.zoom;
        let font_id = egui::FontId::proportional(font_size);
        let max_text_width = (node_width - 8.0) * self.zoom; // Leave padding
        let truncated_name = ui.fonts_mut(|fonts| {
            truncate_text_measured(name, max_text_width, &font_id, fonts)
        });
        let text_color = dim_color(theme.text.primary);
        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            truncated_name,
            font_id,
            text_color,
        );

        // Draw compact meter below header if we have meter data
        let mut current_y = screen_pos.y + theme.sizes.node_header_height * self.zoom;
        if has_meter {
            if let Some(meter) = meter_data {
                let meter_rect = Rect::from_min_size(
                    Pos2::new(screen_pos.x + 4.0 * self.zoom, current_y + 2.0 * self.zoom),
                    Vec2::new(
                        (node_width - 8.0) * self.zoom,
                        4.0 * self.zoom,
                    ),
                );
                let stale_threshold = std::time::Duration::from_millis(100);
                self.draw_compact_meter(ui, meter_rect, meter.get_decayed_max_peak(stale_threshold), theme);
                current_y += meter_height * self.zoom;
            }
        }

        // Draw ports (skip labels at low LOD)
        let port_start_y = current_y + 4.0;
        let draw_port_labels = lod == LodLevel::Full;

        for (i, port) in input_ports.iter().enumerate() {
            let port_y = port_start_y + (i as f32 * theme.sizes.port_height * self.zoom);
            self.draw_port_with_lod(
                ui,
                port,
                Pos2::new(screen_pos.x, port_y),
                true,
                draw_port_labels,
                node_width,
                theme,
                response,
            );
        }

        for (i, port) in output_ports.iter().enumerate() {
            let port_y = port_start_y + (i as f32 * theme.sizes.port_height * self.zoom);
            self.draw_port_with_lod(
                ui,
                port,
                Pos2::new(
                    screen_pos.x + node_width * self.zoom,
                    port_y,
                ),
                false,
                draw_port_labels,
                node_width,
                theme,
                response,
            );
        }

        // Handle node interaction for hover and selection (excluding ports area on edges)
        // Create a body rect that excludes the port areas on left and right edges
        let port_margin = theme.sizes.port_radius * 3.0 * self.zoom;
        let body_rect = egui::Rect::from_min_max(
            egui::Pos2::new(node_rect.min.x + port_margin, node_rect.min.y),
            egui::Pos2::new(node_rect.max.x - port_margin, node_rect.max.y),
        );

        let node_response = ui.interact(body_rect, ui.id().with(node.id.raw()), Sense::click_and_drag());

        if node_response.hovered() && self.hovered_port.is_none() {
            self.hovered_node = Some(node.id);
        }

        // Click to select (only if not on a port and no connection being created)
        if node_response.clicked() && self.hovered_port.is_none() && response.started_connection.is_none() {
            response.clicked_node = Some(node.id);
        }

        // Allow dragging any node for a natural feel
        // But only if we're not on a port and no connection is being started
        if node_response.dragged() && self.hovered_port.is_none() && response.started_connection.is_none() {
            response.dragged_node = Some((node.id, node_response.drag_delta() / self.zoom));
        }

        // Show tooltip with node explanation on hover
        let tooltip_text = explain_node_short(node, graph);
        node_response.clone().on_hover_text(&tooltip_text);

        // Handle right-click context menu on node
        let node_id = node.id;
        node_response.context_menu(|ui| {
            Self::show_node_context_menu(ui, node_id, graph, uninteresting_nodes, custom_names, response);
        });
    }

    /// Shows the context menu for a specific node.
    fn show_node_context_menu(
        ui: &mut Ui,
        node_id: NodeId,
        graph: &GraphState,
        uninteresting_nodes: &HashSet<NodeId>,
        custom_names: &HashMap<NodeId, String>,
        response: &mut GraphViewResponse,
    ) {
        // Get node info for display, prioritizing custom name
        let node_name = custom_names
            .get(&node_id)
            .cloned()
            .or_else(|| graph.get_node(&node_id).map(|n| n.display_name().to_string()))
            .unwrap_or_else(|| format!("Node {:?}", node_id));

        let is_uninteresting = uninteresting_nodes.contains(&node_id);

        ui.heading(&node_name);
        ui.separator();

        if ui.button("Select").clicked() {
            response.clicked_node = Some(node_id);
            ui.close();
        }

        if ui.button("Inspect").clicked() {
            response.clicked_node = Some(node_id);
            ui.close();
        }

        if ui.button("Rename...").clicked() {
            response.rename_node = Some(node_id);
            ui.close();
        }

        ui.separator();

        // Toggle uninteresting status
        let toggle_label = if is_uninteresting {
            "Mark as Interesting"
        } else {
            "Mark as Uninteresting"
        };
        if ui.button(toggle_label).clicked() {
            response.toggle_uninteresting = Some(vec![node_id]);
            ui.close();
        }

        // Snap to grid - will snap only this node (or selected nodes if this is selected)
        if ui.button("Snap to Grid").clicked() {
            response.snap_to_grid = Some(Some(vec![node_id]));
            ui.close();
        }

        ui.separator();

        // Show connections
        let input_links: Vec<_> = graph.links.values()
            .filter(|l| l.input_node == node_id)
            .collect();
        let output_links: Vec<_> = graph.links.values()
            .filter(|l| l.output_node == node_id)
            .collect();

        if !input_links.is_empty() || !output_links.is_empty() {
            ui.label(format!("{} input, {} output connections",
                input_links.len(), output_links.len()));
            ui.separator();
        }

        if ui.button("Disconnect All").clicked() {
            // Queue all links for removal
            for link in input_links.iter().chain(output_links.iter()) {
                response.remove_link = Some(link.id);
            }
            ui.close();
        }

        // Only show "Save Connections as Rule" if there are connections
        if !input_links.is_empty() || !output_links.is_empty() {
            ui.separator();
            if ui.button("Save Connections as Rule...").clicked() {
                response.save_connections_as_rule = Some(node_id);
                ui.close();
            }
        }
    }

    /// Draws a port with LOD support (optionally skips labels).
    #[allow(clippy::too_many_arguments)]
    fn draw_port_with_lod(
        &mut self,
        ui: &mut Ui,
        port: &Port,
        pos: Pos2,
        is_input: bool,
        draw_labels: bool,
        node_width: f32,
        theme: &Theme,
        response: &mut GraphViewResponse,
    ) {
        let painter = ui.painter();

        let circle_x = if is_input {
            pos.x + theme.sizes.port_radius * self.zoom
        } else {
            pos.x - theme.sizes.port_radius * self.zoom
        };
        let circle_center = Pos2::new(circle_x, pos.y + theme.sizes.port_height * self.zoom * 0.5);

        // Port color based on type
        let color = theme.port_color(
            port.direction,
            true, // Assume audio for now
            false,
            false,
            port.is_control,
            port.is_monitor,
        );

        // Draw port circle
        let radius = theme.sizes.port_radius * self.zoom;
        painter.circle_filled(circle_center, radius, color);

        // Port name (only at full LOD, truncated to fit)
        if draw_labels {
            let text_x = if is_input {
                pos.x + theme.sizes.port_radius * 2.5 * self.zoom
            } else {
                pos.x - theme.sizes.port_radius * 2.5 * self.zoom
            };
            let text_align = if is_input {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::RIGHT_CENTER
            };

            let font_size = 10.0 * self.zoom;
            let font_id = egui::FontId::proportional(font_size);

            // Port names get half the node width minus:
            // - port circle area (port_radius * 2.5 for the circle and gap)
            // - center margin (to prevent left/right labels from touching)
            let port_circle_space = theme.sizes.port_radius * 2.5;
            let center_margin = 4.0; // Small gap in the middle between left and right labels
            let max_port_text_width =
                (node_width * 0.5 - port_circle_space - center_margin) * self.zoom;

            let truncated_port_name = ui.fonts_mut(|fonts| {
                truncate_text_measured(port.display_name(), max_port_text_width, &font_id, fonts)
            });

            painter.text(
                Pos2::new(text_x, circle_center.y),
                text_align,
                truncated_port_name,
                font_id,
                theme.text.secondary,
            );
        }

        // Handle port interaction - expand rect to include label area for tooltip
        let label_width = if draw_labels {
            (node_width * 0.5 - theme.sizes.port_radius * 4.0) * self.zoom
        } else {
            0.0
        };
        let port_rect = if is_input {
            Rect::from_min_size(
                Pos2::new(circle_center.x - radius, circle_center.y - radius * 1.25),
                Vec2::new(radius * 2.5 + label_width, radius * 2.5),
            )
        } else {
            Rect::from_min_size(
                Pos2::new(circle_center.x - radius * 1.5 - label_width, circle_center.y - radius * 1.25),
                Vec2::new(radius * 2.5 + label_width, radius * 2.5),
            )
        };
        let port_response = ui.interact(port_rect, ui.id().with(port.id.raw()), Sense::click_and_drag());

        // Check hover and drag states before consuming the response
        let is_hovered = port_response.hovered();
        let drag_started = port_response.drag_started();

        if is_hovered {
            self.hovered_port = Some(port.id);
        }

        if drag_started {
            response.started_connection = Some(port.id);
        }

        // Show tooltip with full port name on hover (consumes response)
        port_response.on_hover_text(port.display_name());
    }

    /// Draws a compact LED-style segmented meter directly on the painter.
    fn draw_compact_meter(&self, ui: &Ui, rect: Rect, peak: f32, theme: &Theme) {
        let painter = ui.painter();

        // Background
        painter.rect_filled(rect, 1.0, theme.meter.background);

        let level = peak.min(1.5);
        let num_segments = 10;
        let segment_width = rect.width() / num_segments as f32;
        let gap = 1.0 * self.zoom.max(0.5); // Scale gap with zoom

        for i in 0..num_segments {
            let segment_start = i as f32 / num_segments as f32 * 1.5;
            let segment_end = (i + 1) as f32 / num_segments as f32 * 1.5;

            // Determine segment color based on position
            // Thresholds: red >= 0.9 (~-1dB), yellow >= 0.7 (~-3dB), green < 0.7
            let (base_color, dim_color) = if segment_start >= 1.0 {
                // Clip zone (bright red) - signal exceeds 0dB
                (Color32::from_rgb(255, 60, 60), Color32::from_rgba_unmultiplied(60, 15, 15, 80))
            } else if segment_start >= 0.9 {
                // Red zone - approaching clipping (~-1dB to 0dB)
                (theme.meter.high, Color32::from_rgba_unmultiplied(theme.meter.high.r() / 5, theme.meter.high.g() / 5, theme.meter.high.b() / 5, 60))
            } else if segment_start >= 0.7 {
                // Yellow zone - elevated levels (~-3dB to -1dB)
                (theme.meter.mid, Color32::from_rgba_unmultiplied(theme.meter.mid.r() / 5, theme.meter.mid.g() / 5, theme.meter.mid.b() / 5, 60))
            } else {
                // Green zone - normal levels (< -3dB)
                (theme.meter.low, Color32::from_rgba_unmultiplied(theme.meter.low.r() / 5, theme.meter.low.g() / 5, theme.meter.low.b() / 5, 60))
            };

            let segment_rect = Rect::from_min_size(
                Pos2::new(rect.min.x + i as f32 * segment_width + gap / 2.0, rect.min.y),
                Vec2::new((segment_width - gap).max(1.0), rect.height()),
            );

            if level >= segment_end {
                // Fully lit segment
                painter.rect_filled(segment_rect, 0.0, base_color);
            } else if level > segment_start {
                // Partially lit - show as dimmed transitioning to lit
                let partial = (level - segment_start) / (segment_end - segment_start);
                let color = Color32::from_rgba_unmultiplied(
                    (dim_color.r() as f32 + (base_color.r() as f32 - dim_color.r() as f32) * partial) as u8,
                    (dim_color.g() as f32 + (base_color.g() as f32 - dim_color.g() as f32) * partial) as u8,
                    (dim_color.b() as f32 + (base_color.b() as f32 - dim_color.b() as f32) * partial) as u8,
                    (dim_color.a() as f32 + (base_color.a() as f32 - dim_color.a() as f32) * partial) as u8,
                );
                painter.rect_filled(segment_rect, 0.0, color);
            } else {
                // Unlit segment (very dim)
                painter.rect_filled(segment_rect, 0.0, dim_color);
            }
        }

        // Pulsing clip indicator when clipping
        if peak > 1.0 {
            let pulse = ((ui.input(|i| i.time) * 10.0).sin() * 0.4 + 0.6) as f32;
            let clip_alpha = (200.0 * pulse) as u8;
            let clip_rect = Rect::from_min_size(
                Pos2::new(rect.max.x - segment_width * 2.0, rect.min.y),
                Vec2::new(segment_width * 2.0, rect.height()),
            );
            painter.rect_filled(clip_rect, 0.0, Color32::from_rgba_unmultiplied(255, 50, 50, clip_alpha));
        }
    }

    /// Draws a link between ports.
    #[allow(clippy::too_many_arguments)]
    fn draw_link(
        &mut self,
        ui: &mut Ui,
        link: &Link,
        graph: &GraphState,
        positions: &HashMap<NodeId, Position>,
        transform: &GraphTransform,
        theme: &Theme,
        mouse_pos: Pos2,
        is_selected: bool,
        is_uninteresting: bool,
    ) {
        // Get port positions using the unified calculation
        let from_pos = match self.get_port_screen_position(&link.output_port, graph, positions, transform, theme) {
            Some(pos) => pos,
            None => return,
        };
        let to_pos = match self.get_port_screen_position(&link.input_port, graph, positions, transform, theme) {
            Some(pos) => pos,
            None => return,
        };

        // Check if this is a self-link (same node connects to itself)
        let is_self_link = link.output_node == link.input_node;

        // For self-links, determine if the loop should go up or down
        // based on whether the ports are in the upper or lower half of the node
        let self_link_goes_up = if is_self_link {
            // Get the node's screen position to find its center
            if let Some(node_pos) = positions.get(&link.output_node) {
                let node_screen_pos = transform.graph_to_screen(Pos2::new(node_pos.x, node_pos.y));
                // Estimate node height (ports are distributed vertically)
                let node_height = graph.get_node(&link.output_node)
                    .map(|n| n.port_ids.len() as f32 * 20.0 * self.zoom + 40.0 * self.zoom)
                    .unwrap_or(100.0 * self.zoom);
                let node_center_y = node_screen_pos.y + node_height / 2.0;

                // If ports are above center, loop goes up; if below, loop goes down
                let avg_port_y = (from_pos.y + to_pos.y) / 2.0;
                avg_port_y < node_center_y
            } else {
                true // Default to going up
            }
        } else {
            true // Not used for non-self-links
        };

        // Hit testing for link hover (use larger tolerance for easier selection)
        // Note: We collect candidates during hit testing, then the caller determines
        // which link is actually hovered. Check against self.hovered_link for styling.
        let is_near_mouse = if is_self_link {
            self.point_near_self_link_bezier(mouse_pos, from_pos, to_pos, 12.0, self_link_goes_up)
        } else {
            self.point_near_bezier(mouse_pos, from_pos, to_pos, 12.0)
        };
        if is_near_mouse && self.hovered_link.is_none() {
            // Only set hovered_link if not already set (first link wins)
            self.hovered_link = Some(link.id);
        }

        // Use the stored hovered_link state for consistent styling across all links
        let is_hovered = self.hovered_link == Some(link.id);

        // Get link meter data for flow visualization
        let link_meter = graph.link_meters.get(&link.id);
        let flow_intensity = link_meter.map(|m| m.glow_intensity()).unwrap_or(0.0);
        let color_hint = link_meter.map(|m| m.color_hint()).unwrap_or(0);
        let pulse_phase = link_meter.map(|m| m.pulse_phase).unwrap_or(0.0);

        // Determine link appearance based on state
        // Priority: selected > hovered > disabled > uninteresting > flow > normal
        let (color, thickness) = if !link.is_active {
            // Disabled links: greyed out (semi-transparent), same structure as active
            let c = if is_selected {
                Color32::from_rgba_unmultiplied(255, 180, 100, 128) // Semi-transparent orange for selected disabled
            } else if is_hovered {
                Color32::from_rgba_unmultiplied(128, 128, 128, 160) // Semi-transparent grey for hovered disabled
            } else {
                // Use the normal wire color but at 40% opacity
                let base = theme.wire.audio;
                Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 100)
            };
            (c, theme.sizes.wire_thickness)
        } else if is_selected {
            // Selected active link: distinct selection color (orange/gold) and thicker
            let selection_color = Color32::from_rgb(255, 180, 100);
            (selection_color, theme.sizes.wire_thickness * 1.8)
        } else if is_hovered {
            // Hovered active link: highlighted color and thicker
            let highlight_color = Color32::from_rgb(100, 200, 255);
            (highlight_color, theme.sizes.wire_thickness * 1.5)
        } else if is_uninteresting {
            // Links to/from uninteresting nodes: muted/faded appearance (distinct from disabled)
            // Uses a desaturated, lower contrast color with slight transparency
            let base = theme.wire.audio;
            let faded = Color32::from_rgba_unmultiplied(
                ((base.r() as u16 + 80) / 2) as u8,  // Shift toward grey
                ((base.g() as u16 + 80) / 2) as u8,
                ((base.b() as u16 + 80) / 2) as u8,
                160, // Slightly transparent but more visible than disabled
            );
            (faded, theme.sizes.wire_thickness * 0.8) // Slightly thinner
        } else {
            // Normal active link - color based on flow level
            let base_color = match color_hint {
                2 => Color32::from_rgb(255, 80, 80),   // Red (clipping)
                1 => Color32::from_rgb(255, 220, 100), // Yellow (elevated)
                _ => theme.meter.low,                   // Green (normal)
            };
            // Interpolate between base wire color and flow color based on intensity
            let wire_color = if flow_intensity > 0.01 {
                interpolate_color(theme.wire.audio, base_color, flow_intensity.min(1.0))
            } else {
                theme.wire.audio
            };
            (wire_color, theme.sizes.wire_thickness)
        };

        // Choose the appropriate drawing function based on whether this is a self-link
        let draw_wire = |s: &Self, ui: &mut Ui, from: Pos2, to: Pos2, color: Color32, thickness: f32| {
            if is_self_link {
                s.draw_self_link_bezier(ui, from, to, color, thickness, self_link_goes_up);
            } else {
                s.draw_bezier_wire(ui, from, to, color, thickness);
            }
        };

        // Draw flow glow layer first (underneath) for active links with activity
        // Skip glow for uninteresting links to keep them visually subdued
        if link.is_active && flow_intensity > 0.01 && !is_selected && !is_hovered && !is_uninteresting {
            let glow_alpha = (flow_intensity * 80.0) as u8;
            let glow_color = match color_hint {
                2 => Color32::from_rgba_unmultiplied(255, 80, 80, glow_alpha),   // Red glow
                1 => Color32::from_rgba_unmultiplied(255, 220, 100, glow_alpha), // Yellow glow
                _ => Color32::from_rgba_unmultiplied(100, 255, 100, glow_alpha), // Green glow
            };
            draw_wire(self, ui, from_pos, to_pos, glow_color, thickness * 3.0);
        }

        // Draw the main link wire
        draw_wire(self, ui, from_pos, to_pos, color, thickness);

        // Draw a glow effect for selected or hovered links (both active and disabled)
        if is_selected || is_hovered {
            let glow_alpha = if link.is_active { 60 } else { 30 }; // Dimmer glow for disabled
            let glow_color = if is_selected {
                Color32::from_rgba_unmultiplied(255, 180, 100, glow_alpha) // Orange glow for selected
            } else {
                Color32::from_rgba_unmultiplied(100, 200, 255, glow_alpha / 2) // Blue glow for hover
            };
            draw_wire(self, ui, from_pos, to_pos, glow_color, thickness * 3.0);
        }

        // Draw traveling pulse for high-activity links (only at Full LOD and high activity)
        // Skip for self-links as the pulse animation doesn't work well with the looping curve
        if link.is_active && flow_intensity > 0.2 && self.zoom > 0.5 && !is_selected && !is_hovered && !is_self_link {
            self.draw_flow_pulse(ui, from_pos, to_pos, pulse_phase, flow_intensity, color_hint);
        }
    }

    /// Draws a traveling pulse effect along a link to indicate audio flow.
    fn draw_flow_pulse(
        &self,
        ui: &mut Ui,
        from: Pos2,
        to: Pos2,
        pulse_phase: f32,
        intensity: f32,
        color_hint: u8,
    ) {
        let painter = ui.painter();

        // Calculate control points for the bezier curve
        let dx = (to.x - from.x).abs() * 0.5;
        let ctrl1 = Pos2::new(from.x + dx, from.y);
        let ctrl2 = Pos2::new(to.x - dx, to.y);

        // Draw multiple pulse dots along the curve
        let num_pulses = 3;
        let pulse_spacing = 1.0 / num_pulses as f32;

        for i in 0..num_pulses {
            let base_t = (pulse_phase + i as f32 * pulse_spacing) % 1.0;
            let pos = cubic_bezier(from, ctrl1, ctrl2, to, base_t);

            // Pulse size and opacity based on intensity
            let pulse_size = (3.0 + intensity * 4.0) * self.zoom;
            let alpha = (intensity * 200.0).min(200.0) as u8;

            let pulse_color = match color_hint {
                2 => Color32::from_rgba_unmultiplied(255, 120, 120, alpha), // Red
                1 => Color32::from_rgba_unmultiplied(255, 230, 150, alpha), // Yellow
                _ => Color32::from_rgba_unmultiplied(150, 255, 150, alpha), // Green
            };

            painter.circle_filled(pos, pulse_size, pulse_color);

            // Add a small glow around the pulse
            let glow_color = Color32::from_rgba_unmultiplied(
                pulse_color.r(),
                pulse_color.g(),
                pulse_color.b(),
                alpha / 3,
            );
            painter.circle_filled(pos, pulse_size * 2.0, glow_color);
        }
    }

    /// Tests if a point is near a bezier curve.
    fn point_near_bezier(&self, point: Pos2, from: Pos2, to: Pos2, tolerance: f32) -> bool {
        let dx = (to.x - from.x).abs() * 0.5;
        let ctrl1 = Pos2::new(from.x + dx, from.y);
        let ctrl2 = Pos2::new(to.x - dx, to.y);

        // Sample points along the curve and check distance
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let p = cubic_bezier(from, ctrl1, ctrl2, to, t);
            let dist = (point - p).length();
            if dist < tolerance {
                return true;
            }
        }
        false
    }

    /// Tests if a point is near a self-link bezier curve.
    /// `go_up`: if true, loops above the ports; if false, loops below.
    fn point_near_self_link_bezier(&self, point: Pos2, from: Pos2, to: Pos2, tolerance: f32, go_up: bool) -> bool {
        // Use the same bezier calculation as draw_self_link_bezier
        let horizontal_dist = (from.x - to.x).abs();
        let loop_height = (horizontal_dist * 0.4).max(40.0 * self.zoom);

        let ctrl_offset_x = horizontal_dist * 0.3 + 20.0 * self.zoom;
        let ctrl_offset_y = loop_height;

        let (ctrl1, ctrl2) = if go_up {
            (
                Pos2::new(from.x + ctrl_offset_x, from.y - ctrl_offset_y),
                Pos2::new(to.x - ctrl_offset_x, to.y - ctrl_offset_y),
            )
        } else {
            (
                Pos2::new(from.x + ctrl_offset_x, from.y + ctrl_offset_y),
                Pos2::new(to.x - ctrl_offset_x, to.y + ctrl_offset_y),
            )
        };

        // Sample points along the bezier and check distance
        for i in 0..=30 {
            let t = i as f32 / 30.0;
            let p = cubic_bezier(from, ctrl1, ctrl2, to, t);

            let dist = (point - p).length();
            if dist < tolerance {
                return true;
            }
        }
        false
    }

    /// Draws a bezier wire between two points.
    fn draw_bezier_wire(&self, ui: &mut Ui, from: Pos2, to: Pos2, color: Color32, thickness: f32) {
        let painter = ui.painter();

        // Calculate control points for a nice curve
        let dx = (to.x - from.x).abs() * 0.5;
        let ctrl1 = Pos2::new(from.x + dx, from.y);
        let ctrl2 = Pos2::new(to.x - dx, to.y);

        // Draw as a series of line segments (egui doesn't have native bezier)
        let segments = 20;
        let mut points = Vec::with_capacity(segments + 1);

        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let p = cubic_bezier(from, ctrl1, ctrl2, to, t);
            points.push(p);
        }

        let stroke = Stroke::new(thickness, color);
        for i in 0..points.len() - 1 {
            painter.line_segment([points[i], points[i + 1]], stroke);
        }
    }

    /// Draws a self-link as a bezier curve that loops around.
    /// `go_up`: if true, loops above the ports; if false, loops below.
    fn draw_self_link_bezier(&self, ui: &mut Ui, from: Pos2, to: Pos2, color: Color32, thickness: f32, go_up: bool) {
        let painter = ui.painter();

        // from = output port (right side), to = input port (left side)
        // Use a cubic bezier curve that starts and ends exactly at the port positions
        // and loops around above or below

        // Calculate the loop height - proportional to horizontal distance but with minimum
        let horizontal_dist = (from.x - to.x).abs();
        let loop_height = (horizontal_dist * 0.4).max(40.0 * self.zoom);

        // Control points extend outward horizontally and up/down vertically
        let ctrl_offset_x = horizontal_dist * 0.3 + 20.0 * self.zoom;
        let ctrl_offset_y = loop_height;

        let (ctrl1, ctrl2) = if go_up {
            // Loop above: control points go up and outward
            (
                Pos2::new(from.x + ctrl_offset_x, from.y - ctrl_offset_y),
                Pos2::new(to.x - ctrl_offset_x, to.y - ctrl_offset_y),
            )
        } else {
            // Loop below: control points go down and outward
            (
                Pos2::new(from.x + ctrl_offset_x, from.y + ctrl_offset_y),
                Pos2::new(to.x - ctrl_offset_x, to.y + ctrl_offset_y),
            )
        };

        // Draw as a series of line segments
        let segments = 30;
        let mut points = Vec::with_capacity(segments + 1);

        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let p = cubic_bezier(from, ctrl1, ctrl2, to, t);
            points.push(p);
        }

        let stroke = Stroke::new(thickness, color);
        for i in 0..points.len() - 1 {
            painter.line_segment([points[i], points[i + 1]], stroke);
        }
    }

    /// Draws the connection currently being created, with optional snap to target.
    #[allow(clippy::too_many_arguments)]
    fn draw_creating_connection_with_snap(
        &self,
        ui: &mut Ui,
        drag: &ConnectionDrag,
        snap_target: Option<Pos2>,
        graph: &GraphState,
        positions: &HashMap<NodeId, Position>,
        transform: &GraphTransform,
        theme: &Theme,
    ) {
        // Get starting port position using the unified calculation
        let from_pos = match self.get_port_screen_position(&drag.from_port, graph, positions, transform, theme) {
            Some(pos) => pos,
            None => return,
        };

        // Use snap target if available, otherwise use mouse position
        let to_pos = snap_target.unwrap_or(drag.current_pos);

        // Use different color when snapped (showing valid connection)
        let (color, thickness) = if snap_target.is_some() {
            (theme.wire.audio, theme.sizes.wire_thickness * 1.5) // Thicker when snapped
        } else {
            (theme.wire.creating, theme.sizes.wire_thickness)
        };

        self.draw_bezier_wire(ui, from_pos, to_pos, color, thickness);
    }

    /// Resets zoom and pan to defaults.
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    /// Computes which nodes are inside a selection rectangle.
    fn compute_box_selection(
        &self,
        selection_rect: Rect,
        graph: &GraphState,
        positions: &HashMap<NodeId, Position>,
        transform: &GraphTransform,
        theme: &Theme,
    ) -> Vec<NodeId> {
        let mut selected = Vec::new();

        for node in graph.nodes.values() {
            let pos = positions
                .get(&node.id)
                .copied()
                .unwrap_or(Position::zero());
            let screen_pos = transform.graph_to_screen(Pos2::new(pos.x, pos.y));

            // Calculate node rectangle
            let node_rect = Rect::from_min_size(
                screen_pos,
                Vec2::new(
                    theme.sizes.node_width * self.zoom,
                    100.0 * self.zoom, // Approximate height
                ),
            );

            // Check if node intersects with selection rectangle
            if selection_rect.intersects(node_rect) {
                selected.push(node.id);
            }
        }

        selected
    }
}

/// Coordinate transform for graph view.
struct GraphTransform {
    center: Pos2,
    zoom: f32,
    pan: Vec2,
}

impl GraphTransform {
    fn new(center: Pos2, zoom: f32, pan: Vec2) -> Self {
        Self { center, zoom, pan }
    }

    fn graph_to_screen(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            self.center.x + (pos.x * self.zoom) + self.pan.x,
            self.center.y + (pos.y * self.zoom) + self.pan.y,
        )
    }
}

/// Cubic bezier interpolation.
fn cubic_bezier(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;

    Pos2::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

/// Interpolates between two colors.
fn interpolate_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
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
    /// Snap to grid requested (None = all nodes, Some = specific nodes)
    pub snap_to_grid: Option<Option<Vec<NodeId>>>,
    /// Toggle uninteresting status for nodes
    pub toggle_uninteresting: Option<Vec<NodeId>>,
    /// Save node's connections as a rule
    pub save_connections_as_rule: Option<NodeId>,
    /// Request to rename a node (opens rename dialog)
    pub rename_node: Option<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_view_default() {
        let view = GraphView::new();
        assert_eq!(view.zoom, 1.0);
        assert_eq!(view.pan, Vec2::ZERO);
    }

    #[test]
    fn test_graph_view_zoom() {
        let mut view = GraphView::new();

        view.zoom = 2.0;
        assert_eq!(view.zoom, 2.0);

        view.zoom = 0.1;
        assert_eq!(view.zoom, 0.1);

        view.zoom = 3.0;
        assert_eq!(view.zoom, 3.0);
    }

    #[test]
    fn test_graph_view_reset() {
        let mut view = GraphView::new();
        view.zoom = 2.0;
        view.pan = Vec2::new(100.0, 50.0);

        view.reset_view();
        assert_eq!(view.zoom, 1.0);
        assert_eq!(view.pan, Vec2::ZERO);
    }

    #[test]
    fn test_cubic_bezier() {
        let p0 = Pos2::new(0.0, 0.0);
        let p1 = Pos2::new(0.0, 1.0);
        let p2 = Pos2::new(1.0, 1.0);
        let p3 = Pos2::new(1.0, 0.0);

        let start = cubic_bezier(p0, p1, p2, p3, 0.0);
        let end = cubic_bezier(p0, p1, p2, p3, 1.0);

        assert!((start.x - p0.x).abs() < 0.001);
        assert!((start.y - p0.y).abs() < 0.001);
        assert!((end.x - p3.x).abs() < 0.001);
        assert!((end.y - p3.y).abs() < 0.001);
    }
}
