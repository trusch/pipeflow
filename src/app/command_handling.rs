//! Command handling and keyboard shortcuts.
//!
//! Routes UI commands, app commands, and keyboard shortcuts to appropriate handlers.

use crate::core::commands::{AppCommand, CommandAction, UiCommand};
use crate::core::history::{UndoAction, UndoEntry};
use crate::core::state::GraphState;
use crate::domain::graph::Node;
use crate::domain::rules::{ConnectionSpec, MatchPattern};
use crate::util::id::{NodeId, NodeIdentifier};
use crate::util::layout::{
    force_directed_layout, get_metering_target_id, is_metering_node, LayoutConfig,
};
use crate::util::spatial::Position;

use super::PipeflowApp;

/// Creates a stable NodeIdentifier for any node, including satellites/metering nodes.
///
/// For regular nodes: uses the node's own properties.
/// For satellite/metering nodes: uses the main node's properties with a prefix,
/// ensuring stable identification across restarts (since meter node names contain
/// ephemeral PipeWire IDs that change on restart).
pub(crate) fn create_stable_identifier(node: &Node, graph: &GraphState) -> NodeIdentifier {
    if is_metering_node(&node.name) {
        if let Some(main_node_id) = get_metering_target_id(&node.name) {
            if let Some(main_node) = graph.get_node(&main_node_id) {
                return NodeIdentifier::new(
                    format!("pipeflow-meter-for:{}", main_node.name),
                    main_node.application_name.clone(),
                    main_node
                        .media_class
                        .as_ref()
                        .map(|mc| mc.display_name().to_string()),
                );
            }
        }
    }
    // Regular node or main node not found - use node's own properties
    NodeIdentifier::new(
        node.name.clone(),
        node.application_name.clone(),
        node.media_class
            .as_ref()
            .map(|mc| mc.display_name().to_string()),
    )
}

impl PipeflowApp {
    /// Handles UI commands that modify local state.
    ///
    /// These commands affect the UI state (selection, positions, groups)
    /// but don't directly interact with PipeWire.
    pub(super) fn handle_ui_command(&mut self, command: UiCommand) {
        // Track whether this command should trigger layout save
        let should_save = matches!(
            command,
            UiCommand::SetNodePosition(..)
                | UiCommand::CreateGroupFromSelection(..)
                | UiCommand::ToggleUninteresting(..)
                | UiCommand::SetCustomName(..)
        );

        let mut state = self.state.write();

        match command {
            UiCommand::SelectNode(id) => {
                state.ui.select_node(id);
            }
            UiCommand::AddToSelection(id) => {
                state.ui.add_to_selection(id);
            }
            UiCommand::ToggleSelection(id) => {
                state.ui.toggle_selection(id);
            }
            UiCommand::ClearSelection => {
                state.ui.clear_selection();
            }
            UiCommand::SetNodePosition(id, x, y) => {
                let pos = Position::new(x, y);
                if let Some(node) = state.graph.get_node(&id) {
                    let identifier = create_stable_identifier(node, &state.graph);
                    state.ui.update_position(id, &identifier, pos);
                } else {
                    state.ui.set_node_position(id, pos);
                }
            }
            UiCommand::CreateGroupFromSelection(name) => {
                let members: Vec<_> = state.ui.selected_nodes.iter().copied().collect();
                if !members.is_empty() {
                    // First, build identifiers before creating the group
                    let identifiers: Vec<_> = members
                        .iter()
                        .filter_map(|node_id| {
                            state
                                .graph
                                .get_node(node_id)
                                .map(|node| create_stable_identifier(node, &state.graph))
                        })
                        .collect();
                    // Create the group with members
                    let group_id = state.ui.groups.create_group_with_members(name, members);
                    // Populate persistent_members so it survives restarts
                    if let Some(group) = state.ui.groups.get_group_mut(&group_id) {
                        for identifier in identifiers {
                            group.persistent_members.insert(identifier);
                        }
                    }
                }
            }
            UiCommand::SetSafetyMode(mode) => {
                state.safety.set_mode(mode);
            }
            UiCommand::ToggleUninteresting(node_ids) => {
                self.handle_toggle_uninteresting(&mut state, node_ids);
            }
            UiCommand::SetCustomName(node_id, name) => {
                if let Some(node) = state.graph.get_node(&node_id) {
                    let identifier = create_stable_identifier(node, &state.graph);
                    match name {
                        Some(custom_name) => {
                            state.ui.set_custom_name(node_id, &identifier, custom_name);
                        }
                        None => {
                            state.ui.clear_custom_name(node_id, &identifier);
                        }
                    }
                }
            }
        }

        if should_save {
            self.components.needs_layout_save = true;
        }
    }

    /// Handles app commands sent to PipeWire.
    pub(super) fn handle_app_command(&mut self, command: AppCommand) {
        let state = self.state.read();
        let safety_mode = state.safety.mode;

        if let Some(ref handler) = self.command_handler {
            if let Err(e) = handler.execute(command.clone(), &state.safety) {
                tracing::error!("Command failed: {}", e);
                let follow_up = match safety_mode {
                    crate::domain::safety::SafetyMode::Normal => "Try again or check the PipeWire state.",
                    crate::domain::safety::SafetyMode::ReadOnly => "Switch Safety back to Normal to make changes.",
                    crate::domain::safety::SafetyMode::Stage => "Stage mode locks routing and volume. Switch Safety to Normal to edit the patch.",
                };
                self.components.status_message = Some((
                    format!("{} {}", e, follow_up),
                    std::time::Instant::now(),
                    true,
                ));
            }
        }
    }

    /// Handles global keyboard shortcuts.
    pub(super) fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        // Skip if command palette is open
        if self.components.command_palette.open {
            return;
        }

        let mut needs_undo = false;
        let mut needs_redo = false;
        let mut needs_open_palette = false;
        let mut needs_auto_layout = false;

        ctx.input(|input| {
            // Ctrl+Z - Undo
            if input.key_pressed(egui::Key::Z) && input.modifiers.command && !input.modifiers.shift
            {
                needs_undo = true;
            }

            // Ctrl+Shift+Z or Ctrl+Y - Redo
            if (input.key_pressed(egui::Key::Z) && input.modifiers.command && input.modifiers.shift)
                || (input.key_pressed(egui::Key::Y) && input.modifiers.command)
            {
                needs_redo = true;
            }

            // Escape - Clear selection
            if input.key_pressed(egui::Key::Escape) {
                self.handle_ui_command(UiCommand::ClearSelection);
            }

            // I - Toggle inspector panel
            if input.key_pressed(egui::Key::I) && !input.modifiers.command {
                self.components.show_inspector = !self.components.show_inspector;
            }

            // H - Toggle help panel
            if input.key_pressed(egui::Key::H) && !input.modifiers.command {
                self.components.show_help = !self.components.show_help;
            }

            // Comma - Toggle settings panel
            if input.key_pressed(egui::Key::Comma) && !input.modifiers.command {
                self.components.show_settings = !self.components.show_settings;
            }

            // [ - Toggle left sidebar
            if input.key_pressed(egui::Key::OpenBracket) && !input.modifiers.command {
                crate::ui::sidebar::SidebarState::clear_egui_state(ctx, "left_panel");
                self.components.left_sidebar.toggle();
            }

            // ] - Toggle right sidebar (inspector)
            if input.key_pressed(egui::Key::CloseBracket) && !input.modifiers.command {
                crate::ui::sidebar::SidebarState::clear_egui_state(ctx, "inspector");
                self.components.right_sidebar.toggle();
            }

            // Delete/Backspace - Delete selected link
            if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
                self.handle_delete_selected_link();
            }

            // Plus/Equals - Zoom in
            if input.key_pressed(egui::Key::Plus)
                || (input.key_pressed(egui::Key::Equals) && input.modifiers.shift)
            {
                self.components.graph_view.zoom = (self.components.graph_view.zoom * 1.2).min(3.0);
            }

            // Minus - Zoom out
            if input.key_pressed(egui::Key::Minus) {
                self.components.graph_view.zoom = (self.components.graph_view.zoom / 1.2).max(0.1);
            }

            // 0 (zero) - Reset zoom
            if input.key_pressed(egui::Key::Num0) && input.modifiers.command {
                self.components.graph_view.reset_view();
            }

            // / (slash) - Open command palette (search)
            if input.key_pressed(egui::Key::Slash) && !input.modifiers.command {
                needs_open_palette = true;
            }

            // Ctrl+L - Auto-layout
            if input.key_pressed(egui::Key::L) && input.modifiers.command {
                needs_auto_layout = true;
            }
        });

        if needs_open_palette {
            self.components.command_palette.open();
        }

        if needs_auto_layout {
            self.perform_auto_layout(false);
        }

        // Execute undo/redo outside the input closure (needs &mut self)
        if needs_undo {
            self.perform_undo();
        }
        if needs_redo {
            self.perform_redo();
        }
    }

    /// Handles command palette action.
    pub(super) fn handle_command_action(&mut self, action: CommandAction) {
        match action {
            CommandAction::Ui(cmd) => self.handle_ui_command(cmd),
            CommandAction::Custom(name) => match name.as_str() {
                "zoom_in" => {
                    self.components.graph_view.zoom =
                        (self.components.graph_view.zoom * 1.2).min(3.0);
                }
                "zoom_out" => {
                    self.components.graph_view.zoom =
                        (self.components.graph_view.zoom / 1.2).max(0.1);
                }
                "reset_view" => {
                    self.components.graph_view.reset_view();
                }
                "increase_spacing" => {
                    self.config.ui.grid_spacing = (self.config.ui.grid_spacing + 5.0).min(50.0);
                }
                "decrease_spacing" => {
                    self.config.ui.grid_spacing = (self.config.ui.grid_spacing - 5.0).max(10.0);
                }
                "toggle_help" => {
                    self.components.show_help = !self.components.show_help;
                }
                "toggle_inspector" => {
                    self.components.show_inspector = !self.components.show_inspector;
                }
                "toggle_settings" => {
                    self.components.show_settings = !self.components.show_settings;
                }
                "toggle_left_sidebar" => {
                    self.components.left_sidebar.toggle();
                }
                "toggle_right_sidebar" => {
                    self.components.right_sidebar.toggle();
                }
                "save_snapshot" => {
                    let state = self.state.read();
                    let result = self
                        .components
                        .snapshot_manager
                        .capture_quick_save(&state.graph, create_stable_identifier);
                    drop(state);
                    match result {
                        Ok(_) => {
                            self.resolve_persistent_issue("saved-setup-save-failed");
                            self.set_status_message("Saved a quick fallback scene", false);
                        }
                        Err(e) => {
                            tracing::error!("Failed to save quick scene: {}", e);
                        }
                    }
                }
                "undo" => {
                    self.perform_undo();
                }
                "redo" => {
                    self.perform_redo();
                }
                "auto_layout" => {
                    self.perform_auto_layout(false);
                }
                "auto_layout_selected" => {
                    self.perform_auto_layout(true);
                }
                _ => {
                    tracing::warn!("Unknown custom command: {}", name);
                }
            },
            CommandAction::GoToNode(node_id) => {
                self.handle_ui_command(UiCommand::SelectNode(node_id));
                let state = self.state.read();
                if let Some(pos) = state.ui.node_positions.get(&node_id) {
                    self.components.graph_view.pan = egui::Vec2::new(
                        -pos.x * self.components.graph_view.zoom,
                        -pos.y * self.components.graph_view.zoom,
                    );
                    self.components.graph_view.zoom = 1.0;
                }
            }
        }
    }

    /// Performs auto-layout using force-directed algorithm.
    pub(super) fn perform_auto_layout(&mut self, selected_only: bool) {
        let state = self.state.read();

        let nodes: Vec<_> = if selected_only {
            state
                .graph
                .nodes
                .values()
                .filter(|n| state.ui.selected_nodes.contains(&n.id))
                .map(|n| (n.id, n.media_class.clone(), n.name.clone()))
                .collect()
        } else {
            state
                .graph
                .nodes
                .values()
                .map(|n| (n.id, n.media_class.clone(), n.name.clone()))
                .collect()
        };

        if nodes.is_empty() {
            return;
        }

        // Capture old positions before layout for undo
        let old_positions: Vec<(NodeId, Position)> = nodes
            .iter()
            .filter_map(|(id, _, _)| state.ui.node_positions.get(id).map(|pos| (*id, *pos)))
            .collect();

        let links: Vec<_> = state
            .graph
            .links
            .values()
            .map(|l| (l.output_node, l.input_node))
            .collect();
        let positions = state.ui.node_positions.clone();
        let config = LayoutConfig::default();
        drop(state);

        let new_positions = force_directed_layout(&nodes, &links, &positions, &config);
        let new_positions_vec: Vec<(NodeId, Position)> =
            new_positions.iter().map(|(id, pos)| (*id, *pos)).collect();

        for (id, pos) in &new_positions {
            self.handle_ui_command(UiCommand::SetNodePosition(*id, pos.x, pos.y));
        }

        // Push batch undo entry
        let reverse_actions: Vec<UndoAction> = old_positions
            .iter()
            .map(|(id, pos)| UndoAction::UiCommand(UiCommand::SetNodePosition(*id, pos.x, pos.y)))
            .collect();
        let forward_actions: Vec<UndoAction> = new_positions_vec
            .iter()
            .map(|(id, pos)| UndoAction::UiCommand(UiCommand::SetNodePosition(*id, pos.x, pos.y)))
            .collect();

        self.components.undo_stack.push(UndoEntry {
            description: "Organize Patch".to_string(),
            forward: UndoAction::Batch(forward_actions),
            reverse: UndoAction::Batch(reverse_actions),
        });
    }

    // --- Undo/Redo ---

    /// Executes an UndoAction (recursing into batches).
    fn execute_undo_action(&mut self, action: UndoAction) {
        match action {
            UndoAction::AppCommand(cmd) => self.handle_app_command(cmd),
            UndoAction::UiCommand(cmd) => self.handle_ui_command(cmd),
            UndoAction::RemoveLinkBetweenPorts {
                output_port,
                input_port,
            } => {
                // Find the link between these ports and remove it
                let link_id = {
                    let state = self.state.read();
                    state
                        .graph
                        .links
                        .values()
                        .find(|l| l.output_port == output_port && l.input_port == input_port)
                        .map(|l| l.id)
                };
                if let Some(link_id) = link_id {
                    {
                        let mut state = self.state.write();
                        state.graph.remove_link(&link_id);
                    }
                    self.handle_app_command(AppCommand::RemoveLink(link_id));
                } else {
                    tracing::warn!(
                        "Undo: could not find link between ports {:?} -> {:?}",
                        output_port,
                        input_port
                    );
                }
            }
            UndoAction::Batch(actions) => {
                for a in actions {
                    self.execute_undo_action(a);
                }
            }
        }
    }

    /// Performs an undo operation.
    pub(super) fn perform_undo(&mut self) {
        if let Some(action) = self.components.undo_stack.undo() {
            self.execute_undo_action(action);
        }
    }

    /// Performs a redo operation.
    pub(super) fn perform_redo(&mut self) {
        if let Some(action) = self.components.undo_stack.redo() {
            self.execute_undo_action(action);
        }
    }

    // --- Private helpers ---

    fn handle_toggle_uninteresting(
        &self,
        state: &mut crate::core::state::AppState,
        node_ids: Vec<crate::util::id::NodeId>,
    ) {
        for node_id in node_ids {
            let is_currently_uninteresting = state.ui.is_uninteresting(&node_id);
            if let Some(node) = state.graph.get_node(&node_id) {
                let identifier = create_stable_identifier(node, &state.graph);
                state
                    .ui
                    .update_uninteresting(node_id, &identifier, !is_currently_uninteresting);
            } else {
                state.ui.toggle_uninteresting(node_id);
            }
        }
    }

    fn handle_delete_selected_link(&mut self) {
        let link_info = {
            let state = self.state.read();
            state.ui.selected_link.and_then(|link_id| {
                state
                    .graph
                    .get_link(&link_id)
                    .map(|l| (link_id, l.output_port, l.input_port))
            })
        };

        if let Some((link_id, output_port, input_port)) = link_info {
            {
                let mut state = self.state.write();
                state.graph.remove_link(&link_id);
                state.ui.selected_link = None;
            }
            self.handle_app_command(AppCommand::RemoveLink(link_id));
            self.components.undo_stack.push(UndoEntry {
                description: "Remove link".to_string(),
                forward: UndoAction::AppCommand(AppCommand::RemoveLink(link_id)),
                reverse: UndoAction::AppCommand(AppCommand::CreateLink {
                    output_port,
                    input_port,
                }),
            });
        }
    }

    /// Processes pending connections queued by connection rules.
    pub(super) fn process_pending_rule_connections(&mut self) {
        // First, process disconnections (for exclusive rules)
        let disconnections = {
            let mut state = self.state.write();
            state.ui.rules.take_pending_disconnections()
        };

        for link_id in disconnections {
            self.handle_app_command(AppCommand::RemoveLink(link_id));
        }

        // Then, process new connections
        let connections = {
            let mut state = self.state.write();
            state.ui.rules.take_pending_connections()
        };

        for pending in connections {
            self.handle_app_command(AppCommand::CreateLink {
                output_port: pending.output_port,
                input_port: pending.input_port,
            });
        }
    }

    /// Creates a connection rule from a node's current connections.
    pub(super) fn create_rule_from_node_connections(&mut self, node_id: NodeId) {
        let (connections, primary_node_name) = {
            let state = self.state.read();

            // Get the primary node's display name
            let primary_node = state.graph.get_node(&node_id);
            let primary_name = primary_node.map(|n| n.display_name().to_string());

            // Collect all connections involving this node
            let mut specs = Vec::new();

            for link in state.graph.links.values() {
                // Check if this link involves our node
                if link.output_node != node_id && link.input_node != node_id {
                    continue;
                }

                let out_port = match state.graph.get_port(&link.output_port) {
                    Some(p) => p,
                    None => continue,
                };
                let in_port = match state.graph.get_port(&link.input_port) {
                    Some(p) => p,
                    None => continue,
                };

                let out_node = match state.graph.get_node(&link.output_node) {
                    Some(n) => n,
                    None => continue,
                };
                let in_node = match state.graph.get_node(&link.input_node) {
                    Some(n) => n,
                    None => continue,
                };

                let output_pattern = MatchPattern::exact(
                    out_node.application_name.as_deref(),
                    &out_node.name,
                    &out_port.name,
                );
                let input_pattern = MatchPattern::exact(
                    in_node.application_name.as_deref(),
                    &in_node.name,
                    &in_port.name,
                );
                specs.push(ConnectionSpec::new(output_pattern, input_pattern));
            }

            (specs, primary_name)
        };

        if connections.is_empty() {
            tracing::info!("No connections to save as rule for node {:?}", node_id);
            return;
        }

        let connection_count = connections.len();
        // Use the node name as the default rule name
        let rule_name = primary_node_name
            .clone()
            .map(|n| format!("{} connections", n));
        let mut state = self.state.write();
        let rule_id =
            state
                .ui
                .rules
                .create_from_snapshot(rule_name, connections, primary_node_name);
        tracing::info!(
            "Created rule {:?} with {} connections",
            rule_id,
            connection_count
        );

        // Mark that we need to save layout
        self.components.needs_layout_save = true;
    }
}
