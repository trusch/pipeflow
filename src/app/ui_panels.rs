//! UI panel helper methods.
//!
//! Contains methods for rendering inspector panels (link details, groups).

use crate::core::state::GraphState;
use crate::ui::groups::GroupPanel;
use crate::util::id::LinkId;

use super::PipeflowApp;

/// Response from the link details panel.
#[derive(Debug, Default)]
pub(super) struct LinkPanelResponse {
    /// Toggle link active state (link_id, new_active_state)
    pub toggle_link: Option<(LinkId, bool)>,
    /// Remove link
    pub remove_link: Option<LinkId>,
}

impl PipeflowApp {
    /// Shows the link details panel and returns any actions requested.
    pub(super) fn show_link_panel(
        &self,
        ui: &mut egui::Ui,
        link: &crate::domain::graph::Link,
        graph: &GraphState,
    ) -> LinkPanelResponse {
        let mut response = LinkPanelResponse::default();

        ui.heading("Connection Details");
        ui.separator();

        let output_node = graph.get_node(&link.output_node);
        let input_node = graph.get_node(&link.input_node);
        let output_port = graph.get_port(&link.output_port);
        let input_port = graph.get_port(&link.input_port);

        // Source section
        egui::CollapsingHeader::new("Source")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("link_source")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Node:");
                        ui.add(
                            egui::Label::new(
                                output_node.map(|n| n.display_name()).unwrap_or("Unknown"),
                            )
                            .wrap(),
                        );
                        ui.end_row();

                        ui.label("Port:");
                        ui.add(
                            egui::Label::new(
                                output_port.map(|p| p.display_name()).unwrap_or("Unknown"),
                            )
                            .wrap(),
                        );
                        ui.end_row();

                        if let Some(node) = output_node {
                            if let Some(ref media_class) = node.media_class {
                                ui.label("Type:");
                                ui.label(media_class.display_name());
                                ui.end_row();
                            }
                        }
                    });
            })
            .header_response
            .on_hover_text("Output side of the connection");

        // Destination section
        egui::CollapsingHeader::new("Destination")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("link_dest").num_columns(2).show(ui, |ui| {
                    ui.label("Node:");
                    ui.add(
                        egui::Label::new(input_node.map(|n| n.display_name()).unwrap_or("Unknown"))
                            .wrap(),
                    );
                    ui.end_row();

                    ui.label("Port:");
                    ui.add(
                        egui::Label::new(input_port.map(|p| p.display_name()).unwrap_or("Unknown"))
                            .wrap(),
                    );
                    ui.end_row();

                    if let Some(node) = input_node {
                        if let Some(ref media_class) = node.media_class {
                            ui.label("Type:");
                            ui.label(media_class.display_name());
                            ui.end_row();
                        }
                    }
                });
            })
            .header_response
            .on_hover_text("Input side of the connection");

        ui.separator();

        // Link status
        egui::Grid::new("link_status").show(ui, |ui| {
            ui.label("Link ID:");
            ui.label(format!("{}", link.id.raw()));
            ui.end_row();

            ui.label("Status:");
            if link.is_active {
                ui.colored_label(egui::Color32::GREEN, "Active");
            } else {
                ui.colored_label(egui::Color32::GRAY, "Disabled");
            }
            ui.end_row();

            ui.label("State:");
            ui.label(link.state.display_name());
            ui.end_row();
        });

        ui.separator();

        // Actions
        let link_id = link.id;
        let is_active = link.is_active;

        ui.horizontal(|ui| {
            let toggle_text = if is_active { "Disable" } else { "Enable" };
            if ui.button(toggle_text).clicked() {
                response.toggle_link = Some((link_id, !is_active));
            }

            if ui.button("Remove").clicked() {
                response.remove_link = Some(link_id);
            }
        });

        response
    }

    /// Shows the groups panel and handles internal responses.
    pub(super) fn show_groups_panel(
        &mut self,
        ui: &mut egui::Ui,
    ) -> crate::ui::groups::GroupPanelResponse {
        let group_panel = &mut self.components.group_panel;
        let theme = &self.components.theme;

        let mut state = self.state.write();
        let selected = state.ui.selected_nodes.clone();
        let display_names = GroupPanel::build_display_name_map(state.graph.nodes.values());
        let response = group_panel.show(ui, &mut state.ui.groups, &selected, &display_names, theme);

        // Handle toggle collapsed
        if let Some(group_id) = response.toggle_collapsed {
            if let Some(group) = state.ui.groups.get_group_mut(&group_id) {
                group.toggle_collapsed();
            }
        }

        // Handle remove from group
        if let Some((node_id, group_id)) = response.remove_from_group {
            // Get the identifier before removing (to also remove from persistent_members)
            let identifier = state
                .graph
                .get_node(&node_id)
                .map(|node| super::command_handling::create_stable_identifier(node, &state.graph));
            if let Some(group) = state.ui.groups.get_group_mut(&group_id) {
                group.remove_member(&node_id);
                // Also remove from persistent_members so it stays removed after restart
                if let Some(id) = identifier {
                    group.persistent_members.remove(&id);
                }
            }
        }

        response
    }
}
