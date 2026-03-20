//! Group management UI.
//!
//! Provides controls for creating and managing node groups.

use crate::domain::graph::Node;
use crate::domain::groups::{GroupId, GroupManager, NodeGroup};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;
use egui::Ui;
use std::collections::{HashMap, HashSet};

/// Group management panel.
pub struct GroupPanel {
    /// Which group is currently being edited (if any)
    editing_group: Option<GroupId>,
    /// Text buffer for editing
    edit_buffer: String,
}

impl Default for GroupPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupPanel {
    /// Creates a new group panel.
    pub fn new() -> Self {
        Self {
            editing_group: None,
            edit_buffer: String::new(),
        }
    }

    /// Shows the group management panel.
    ///
    /// `node_display_names` should be a pre-computed map of NodeId -> display name with
    /// collision handling (use `build_display_name_map` to create this).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        groups: &mut GroupManager,
        selected_nodes: &HashSet<NodeId>,
        node_display_names: &HashMap<NodeId, String>,
        _theme: &Theme,
    ) -> GroupPanelResponse {
        let mut response = GroupPanelResponse::default();

        // Create group button
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!selected_nodes.is_empty(), egui::Button::new("+ New Group"))
                .clicked()
            {
                let members: Vec<NodeId> = selected_nodes.iter().copied().collect();
                let id = groups.create_group_with_members(None, members);
                response.created_group = Some(id);
            }

            if selected_nodes.is_empty() {
                ui.label("(Select nodes first)");
            } else {
                ui.label(format!("({} nodes selected)", selected_nodes.len()));
            }
            help_button(ui, "groups", "creating_groups");
        });

        ui.separator();

        // List of groups
        if groups.groups.is_empty() {
            ui.label("No groups");
        } else {
            let mut groups_to_remove = Vec::new();

            for group in &mut groups.groups {
                ui.push_id(group.id.0, |ui| {
                    self.show_group_entry(
                        ui,
                        group,
                        &mut response,
                        &mut groups_to_remove,
                        node_display_names,
                    );
                });
            }

            // Remove groups marked for deletion
            for id in groups_to_remove {
                groups.remove_group(&id);
            }
        }

        response
    }

    /// Builds a map of NodeId -> display name with collision handling.
    /// If multiple nodes have the same base display name, they get numbered suffixes.
    ///
    /// This should be called once per frame (or when nodes change) and passed to `show()`.
    pub fn build_display_name_map<'a>(
        nodes: impl Iterator<Item = &'a Node>,
    ) -> HashMap<NodeId, String> {
        // First pass: count occurrences of each base display name
        let mut name_counts: HashMap<String, Vec<NodeId>> = HashMap::new();
        for node in nodes {
            let base_name = node.display_name().to_string();
            name_counts.entry(base_name).or_default().push(node.id);
        }

        // Second pass: assign final names with numbers for collisions
        let mut result = HashMap::new();
        for (base_name, mut node_ids) in name_counts {
            if node_ids.len() == 1 {
                // No collision, use base name
                result.insert(node_ids[0], base_name);
            } else {
                // Sort node IDs for consistent numbering
                node_ids.sort_by_key(|id| id.raw());
                // Collision: add numbers
                for (i, node_id) in node_ids.iter().enumerate() {
                    result.insert(*node_id, format!("{} {}", base_name, i + 1));
                }
            }
        }

        result
    }

    /// Shows a single group entry.
    fn show_group_entry(
        &mut self,
        ui: &mut Ui,
        group: &mut NodeGroup,
        panel_response: &mut GroupPanelResponse,
        groups_to_remove: &mut Vec<GroupId>,
        display_names: &HashMap<NodeId, String>,
    ) {
        let modifiers = ui.input(|i| i.modifiers);
        let is_editing = self.editing_group == Some(group.id);
        let group_id = group.id;

        // Header row: [color] [▼] [name] [count] ... [✎] [×]
        ui.horizontal(|ui| {
            // Color indicator
            let color = group.color.to_color32();
            let (color_rect, painter) =
                ui.allocate_painter(egui::vec2(12.0, 12.0), egui::Sense::hover());
            painter.rect_filled(color_rect.rect, 2.0, color);
            ui.add_space(4.0);

            // Collapse toggle
            let collapse_text = if group.collapsed {
                egui_phosphor::regular::CARET_RIGHT
            } else {
                egui_phosphor::regular::CARET_DOWN
            };
            if ui
                .small_button(collapse_text)
                .on_hover_text(if group.collapsed {
                    "Expand"
                } else {
                    "Collapse"
                })
                .clicked()
            {
                panel_response.toggle_collapsed = Some(group_id);
            }

            if is_editing {
                // Inline text edit
                let text_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .desired_width(ui.available_width() - 50.0),
                );

                if text_edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !self.edit_buffer.is_empty() {
                        group.name = self.edit_buffer.clone();
                        panel_response.renamed_group = Some((group_id, self.edit_buffer.clone()));
                    }
                    self.editing_group = None;
                    self.edit_buffer.clear();
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.editing_group = None;
                    self.edit_buffer.clear();
                }

                text_edit.request_focus();
            } else {
                // Group name - clickable to select all members, truncated
                let name_response = ui.add(
                    egui::Label::new(&group.name)
                        .sense(egui::Sense::click())
                        .truncate(),
                );
                if name_response.clicked() {
                    panel_response.select_group_members = Some(group_id);
                }
                name_response
                    .on_hover_text(format!("{}\n\nClick to select all members", &group.name));

                // Member count badge
                let count = group.effective_member_count();
                let count_label = if group.is_pending_reconciliation() {
                    format!("({} pending)", count)
                } else {
                    format!("({})", count)
                };
                ui.weak(count_label);
            }

            // Action buttons - right aligned, consistent order: edit, delete
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Delete (rightmost)
                if ui
                    .small_button(egui_phosphor::regular::X)
                    .on_hover_text("Delete group")
                    .clicked()
                {
                    groups_to_remove.push(group_id);
                }

                if ui
                    .small_button(egui_phosphor::regular::FADERS)
                    .on_hover_text("Open mixer view")
                    .clicked()
                {
                    panel_response.open_mixer = Some(group_id);
                }

                // Edit/Rename
                if ui
                    .small_button(egui_phosphor::regular::PENCIL_SIMPLE)
                    .on_hover_text("Rename group")
                    .clicked()
                {
                    self.editing_group = Some(group_id);
                    self.edit_buffer = group.name.clone();
                }
            });
        });

        // Show members if not collapsed
        if !group.collapsed {
            ui.indent(group.id.0, |ui| {
                let mut members: Vec<_> = group.members.iter().copied().collect();
                members.sort_by_key(|id| id.raw());

                for node_id in members {
                    ui.horizontal(|ui| {
                        let name = display_names
                            .get(&node_id)
                            .cloned()
                            .unwrap_or_else(|| format!("Node {}", node_id.raw()));

                        // Node name - clickable to toggle selection
                        let node_label = ui.add(
                            egui::Label::new(&name)
                                .sense(egui::Sense::click())
                                .truncate(),
                        );
                        if node_label.clicked() {
                            panel_response.toggle_node_selection = Some((node_id, modifiers.shift));
                        }
                        node_label.on_hover_text(format!(
                            "{}\n\nClick to select, Shift+click to add",
                            &name
                        ));

                        // Remove button
                        if ui
                            .small_button(egui_phosphor::regular::X)
                            .on_hover_text("Remove from group")
                            .clicked()
                        {
                            panel_response.remove_from_group = Some((node_id, group_id));
                        }
                    });
                }

                if group.is_truly_empty() {
                    ui.weak("(empty)");
                } else if group.is_pending_reconciliation() {
                    ui.weak(format!(
                        "(waiting for {} nodes)",
                        group.persistent_member_count()
                    ));
                }
            });
        }

        ui.add_space(2.0);
        ui.separator();
    }
}

/// Response from the group panel.
#[derive(Debug, Default)]
pub struct GroupPanelResponse {
    /// A group was created
    pub created_group: Option<GroupId>,
    /// Toggle collapsed state for a group
    pub toggle_collapsed: Option<GroupId>,
    /// Remove a node from a group
    pub remove_from_group: Option<(NodeId, GroupId)>,
    /// Select all members of a group (replace selection)
    pub select_group_members: Option<GroupId>,
    /// Toggle a node's selection state (with shift = extend, without = replace)
    pub toggle_node_selection: Option<(NodeId, bool)>,
    /// Open the dedicated mixer view for a group
    pub open_mixer: Option<GroupId>,
    /// A group was renamed (group_id, new_name)
    pub renamed_group: Option<(GroupId, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_panel_response_default() {
        let response = GroupPanelResponse::default();
        assert!(response.created_group.is_none());
        assert!(response.toggle_collapsed.is_none());
    }
}
