//! Node inspection panel.
//!
//! Shows detailed information about selected nodes.

use crate::core::state::GraphState;
use crate::domain::audio::VolumeControl;
use crate::domain::explain::explain_node;
use crate::domain::graph::{Node, Port, PortDirection};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;
use egui::Ui;
use std::collections::HashSet;

/// Node inspection panel.
pub struct NodePanel;

impl NodePanel {
    /// Shows the node panel for multiple selected nodes.
    pub fn show_multi(
        ui: &mut Ui,
        node_ids: &[NodeId],
        graph: &GraphState,
        uninteresting_nodes: &HashSet<NodeId>,
        theme: &Theme,
    ) -> NodePanelResponse {
        let mut response = NodePanelResponse::default();

        if node_ids.is_empty() {
            Self::show_empty_state(ui, theme);
        } else if node_ids.len() == 1 {
            // Single node - show full details
            if let Some(node) = graph.get_node(&node_ids[0]) {
                let is_uninteresting = uninteresting_nodes.contains(&node.id);
                Self::show_node_details(ui, node, graph, is_uninteresting, theme, &mut response);
            } else {
                Self::show_empty_state(ui, theme);
            }
        } else {
            // Multiple nodes - show as accordion
            ui.heading(format!("{} Nodes Selected", node_ids.len()));
            ui.separator();

            // Add bulk action for marking all as uninteresting
            ui.horizontal(|ui| {
                if ui.button("Mark All as Background").clicked() {
                    response.toggle_uninteresting = Some(node_ids.to_vec());
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for node_id in node_ids {
                    if let Some(node) = graph.get_node(node_id) {
                        let header = node.display_name().to_string();
                        let is_uninteresting = uninteresting_nodes.contains(node_id);
                        ui.collapsing(header, |ui| {
                            Self::show_node_details(
                                ui,
                                node,
                                graph,
                                is_uninteresting,
                                theme,
                                &mut response,
                            );
                        });
                    }
                }
            });
        }

        response
    }

    /// Shows details for a specific node.
    fn show_node_details(
        ui: &mut Ui,
        node: &Node,
        graph: &GraphState,
        is_uninteresting: bool,
        theme: &Theme,
        response: &mut NodePanelResponse,
    ) {
        // Header
        ui.heading(node.display_name());

        // Toggle uninteresting button
        ui.horizontal(|ui| {
            let button_text = if is_uninteresting {
                "Bring Into Focus"
            } else {
                "Mark as Background"
            };
            let hover_text = if is_uninteresting {
                "Return this node to the main patch view"
            } else {
                "Keep this node in the background so the main patch stays readable"
            };
            if ui.button(button_text).on_hover_text(hover_text).clicked() {
                response.toggle_uninteresting = Some(vec![node.id]);
            }
        });

        ui.separator();

        // Node explanation section (expanded by default)
        egui::CollapsingHeader::new("What is this?")
            .default_open(true)
            .show(ui, |ui| {
                let explanation = explain_node(node, graph);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&explanation)
                            .size(12.0)
                            .color(ui.visuals().text_color()),
                    )
                    .wrap(),
                );
            });

        // Metadata section
        ui.collapsing("Metadata", |ui| {
            egui::Grid::new("node_metadata")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("ID:");
                    ui.label(format!("{}", node.id));
                    ui.end_row();

                    ui.label("Name:");
                    ui.add(egui::Label::new(&node.name).wrap());
                    ui.end_row();

                    if let Some(ref nick) = node.nick {
                        ui.label("Nick:");
                        ui.add(egui::Label::new(nick).wrap());
                        ui.end_row();
                    }

                    if let Some(ref desc) = node.description {
                        ui.label("Description:");
                        ui.add(egui::Label::new(desc).wrap());
                        ui.end_row();
                    }

                    if let Some(ref app_name) = node.application_name {
                        ui.label("Application:");
                        ui.add(egui::Label::new(app_name).wrap());
                        ui.end_row();
                    }

                    if let Some(ref media_class) = node.media_class {
                        ui.label("Media Class:");
                        ui.label(media_class.display_name());
                        ui.end_row();
                    }

                    if let Some(client_id) = node.client_id {
                        ui.label("Client ID:");
                        ui.label(format!("{}", client_id));
                        ui.end_row();
                    }

                    ui.label("Active:");
                    ui.label(if node.is_active { "Yes" } else { "No" });
                    ui.end_row();
                });
        });

        // Format section
        if let Some(ref format) = node.format {
            egui::CollapsingHeader::new("Audio Format").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("");
                    help_button(ui, "audio", "audio_basics");
                });
                egui::Grid::new("node_format").show(ui, |ui| {
                    ui.label("Sample Rate:");
                    ui.label(format!("{} Hz", format.sample_rate));
                    ui.end_row();

                    ui.label("Channels:");
                    ui.label(format!("{}", format.channels));
                    ui.end_row();

                    ui.label("Format:");
                    ui.label(&format.format);
                    ui.end_row();
                });
            });
        }

        // Volume control
        if let Some(volume) = graph.volumes.get(&node.id) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("Volume");
                help_button(ui, "audio", "volume_control");
            });

            // Show warning if volume control failed for this node
            if let Some(error_msg) = graph.volume_control_failed.get(&node.id) {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::WARNING)
                            .color(egui::Color32::YELLOW),
                    );
                    ui.label(
                        egui::RichText::new(error_msg)
                            .color(egui::Color32::YELLOW)
                            .small(),
                    );
                });
            }

            if Self::show_volume_control(ui, node.id, volume, response) {
                // Volume was changed
            }
        }

        // Ports section
        ui.separator();
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

        if !input_ports.is_empty() {
            ui.collapsing(format!("Input Ports ({})", input_ports.len()), |ui| {
                for port in input_ports {
                    Self::show_port(ui, port, theme);
                }
            });
        }

        if !output_ports.is_empty() {
            ui.collapsing(format!("Output Ports ({})", output_ports.len()), |ui| {
                for port in output_ports {
                    Self::show_port(ui, port, theme);
                }
            });
        }

        // Links section
        let links = graph.links_for_node(&node.id);
        if !links.is_empty() {
            ui.separator();
            egui::CollapsingHeader::new(format!("Connections ({})", links.len()))
                .default_open(true)
                .show(ui, |ui| {
                    let modifiers = ui.input(|i| i.modifiers);

                    for link in links {
                        let direction = if link.output_node == node.id {
                            egui_phosphor::regular::ARROW_RIGHT
                        } else {
                            egui_phosphor::regular::ARROW_LEFT
                        };
                        let other_node_id = if link.output_node == node.id {
                            link.input_node
                        } else {
                            link.output_node
                        };

                        let other_name = graph
                            .get_node(&other_node_id)
                            .map(|n| n.display_name().to_string())
                            .unwrap_or_else(|| "Unknown".to_string());

                        // Connection info and action buttons on same line
                        ui.horizontal(|ui| {
                            // Toggle button
                            let toggle_text = if link.is_active {
                                egui_phosphor::regular::PAUSE
                            } else {
                                egui_phosphor::regular::PLAY
                            };
                            let toggle_hint = if link.is_active { "Disable" } else { "Enable" };
                            if ui
                                .small_button(toggle_text)
                                .on_hover_text(toggle_hint)
                                .clicked()
                            {
                                response.toggle_link = Some((link.id, !link.is_active));
                            }

                            // Delete button
                            if ui
                                .small_button(egui_phosphor::regular::X)
                                .on_hover_text("Remove connection")
                                .clicked()
                            {
                                response.remove_link = Some(link.id);
                            }

                            // Direction indicator
                            ui.label(direction);

                            // Node name - clickable to select
                            let name_response = ui.add(
                                egui::Label::new(egui::RichText::new(&other_name).underline())
                                    .sense(egui::Sense::click()),
                            );
                            if name_response.clicked() {
                                response.toggle_node_selection =
                                    Some((other_node_id, modifiers.shift));
                            }
                            name_response
                                .on_hover_text("Click to select, Shift+click to add to selection");

                            // Status indicator
                            if !link.is_active {
                                ui.colored_label(egui::Color32::GRAY, "(disabled)");
                            }
                        });
                        ui.separator();
                    }
                });
        }
    }

    /// Shows the volume control.
    fn show_volume_control(
        ui: &mut Ui,
        node_id: NodeId,
        volume: &VolumeControl,
        response: &mut NodePanelResponse,
    ) -> bool {
        let mut changed = false;

        // Get display mode from persistent state (true = dB, false = %)
        let show_db =
            ui.data_mut(|d| *d.get_persisted_mut_or(egui::Id::new("volume_display_db"), false));

        // Use available_width which returns the layout width available for widgets
        let content_width = ui.available_width();

        // Mute button
        ui.add_space(4.0);
        let mute_text = if volume.muted { "MUTED" } else { "Active" };
        let mute_color = if volume.muted {
            egui::Color32::from_rgb(180, 60, 60)
        } else {
            egui::Color32::from_rgb(60, 140, 60)
        };
        let mute_btn =
            egui::Button::new(egui::RichText::new(mute_text).color(egui::Color32::WHITE))
                .fill(mute_color);
        if ui.add_sized([content_width, 28.0], mute_btn).clicked() {
            response.toggle_mute = Some(node_id);
            changed = true;
        }

        ui.add_space(8.0);

        // Master volume
        let mut master = volume.master;

        // Row: "Master" label + value input
        ui.horizontal(|ui| {
            ui.strong("Master:");
            ui.add_space(8.0);
            if show_db {
                // dB display: -inf to +6 dB
                let mut db_val = crate::domain::audio::linear_to_db(master);
                if db_val < -60.0 {
                    db_val = -60.0;
                }
                let drag = egui::DragValue::new(&mut db_val)
                    .range(-60.0..=6.0)
                    .speed(0.5)
                    .suffix(" dB")
                    .min_decimals(1)
                    .max_decimals(1);
                if ui.add(drag).changed() {
                    master = db_to_linear(db_val);
                    response.volume_changed = Some((node_id, master));
                    changed = true;
                }
            } else {
                // Percent display: 0% to 150%
                let mut pct = master * 100.0;
                let drag = egui::DragValue::new(&mut pct)
                    .range(0.0..=150.0)
                    .speed(1.0)
                    .suffix(" %")
                    .min_decimals(0)
                    .max_decimals(0);
                if ui.add(drag).changed() {
                    master = (pct / 100.0).clamp(0.0, 1.5);
                    response.volume_changed = Some((node_id, master));
                    changed = true;
                }
            }
        });

        // Slider - set slider_width to control the rail, then add_sized for the container
        ui.add_space(4.0);
        ui.spacing_mut().slider_width = content_width;
        let slider = egui::Slider::new(&mut master, 0.0..=1.5)
            .show_value(false)
            .trailing_fill(true);
        let master_changed_this_frame = ui.add_sized([content_width, 20.0], slider).changed();
        if master_changed_this_frame {
            response.volume_changed = Some((node_id, master));
            changed = true;
        }

        ui.add_space(4.0);

        // Display mode toggle
        ui.horizontal(|ui| {
            let mut db_mode = show_db;
            if ui.checkbox(&mut db_mode, "Show dB").changed() {
                ui.data_mut(|d| {
                    d.insert_persisted(egui::Id::new("volume_display_db"), db_mode);
                });
            }
        });

        // Per-channel controls (collapsed)
        if volume.channels.len() > 1 {
            ui.add_space(4.0);
            egui::CollapsingHeader::new(format!("Channels ({})", volume.channels.len())).show(
                ui,
                |ui| {
                    help_button(ui, "audio", "channels_explained");

                    // Get channel content width from available layout width
                    let ch_content_width = ui.available_width();

                    for (i, &ch_vol) in volume.channels.iter().enumerate() {
                        // Use the new master value if master just changed, otherwise use channel value
                        // This prevents 1-frame lag where channels show old values while master shows new
                        let mut ch = if master_changed_this_frame {
                            master
                        } else {
                            ch_vol
                        };

                        ui.horizontal(|ui| {
                            ui.label(format!("Ch {}:", i + 1));
                            ui.add_space(4.0);
                            if show_db {
                                let mut db_val = crate::domain::audio::linear_to_db(ch);
                                if db_val < -60.0 {
                                    db_val = -60.0;
                                }
                                let drag = egui::DragValue::new(&mut db_val)
                                    .range(-60.0..=6.0)
                                    .speed(0.5)
                                    .suffix(" dB")
                                    .min_decimals(1)
                                    .max_decimals(1);
                                if ui.add(drag).changed() {
                                    ch = db_to_linear(db_val);
                                    response.channel_volume_changed = Some((node_id, i, ch));
                                    changed = true;
                                }
                            } else {
                                let mut pct = ch * 100.0;
                                let drag = egui::DragValue::new(&mut pct)
                                    .range(0.0..=150.0)
                                    .speed(1.0)
                                    .suffix(" %")
                                    .min_decimals(0)
                                    .max_decimals(0);
                                if ui.add(drag).changed() {
                                    ch = (pct / 100.0).clamp(0.0, 1.5);
                                    response.channel_volume_changed = Some((node_id, i, ch));
                                    changed = true;
                                }
                            }
                        });

                        // Channel slider - set slider_width to control the rail
                        ui.spacing_mut().slider_width = ch_content_width;
                        let slider = egui::Slider::new(&mut ch, 0.0..=1.5)
                            .show_value(false)
                            .trailing_fill(true);
                        if ui.add_sized([ch_content_width, 20.0], slider).changed() {
                            response.channel_volume_changed = Some((node_id, i, ch));
                            changed = true;
                        }
                        ui.add_space(2.0);
                    }
                },
            );
        }

        changed
    }

    /// Shows a port entry with expandable details.
    fn show_port(ui: &mut Ui, port: &Port, theme: &Theme) {
        // Port indicator color
        let color = theme.port_color(
            port.direction,
            true,
            false,
            false,
            port.is_control,
            port.is_monitor,
        );

        // Build a summary line with flags
        let mut summary = port.display_name().to_string();
        if port.is_monitor {
            summary.push_str(" (monitor)");
        }
        if port.is_control {
            summary.push_str(" (control)");
        }

        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, color);

            // Collapsible port details
            egui::CollapsingHeader::new(&summary)
                .id_salt(port.id.raw())
                .default_open(false)
                .show(ui, |ui| {
                    egui::Grid::new(format!("port_details_{}", port.id.raw()))
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("ID:");
                            ui.label(format!("{}", port.id));
                            ui.end_row();

                            ui.label("Name:");
                            ui.add(egui::Label::new(&port.name).wrap());
                            ui.end_row();

                            if let Some(ref alias) = port.alias {
                                ui.label("Alias:");
                                ui.add(egui::Label::new(alias).wrap());
                                ui.end_row();
                            }

                            if let Some(channel) = port.channel {
                                ui.label("Channel:");
                                ui.label(format!("{}", channel));
                                ui.end_row();
                            }

                            if let Some(ref path) = port.physical_path {
                                ui.label("Physical:");
                                ui.add(egui::Label::new(path).wrap());
                                ui.end_row();
                            }

                            ui.label("Direction:");
                            ui.label(match port.direction {
                                PortDirection::Input => "Input",
                                PortDirection::Output => "Output",
                            });
                            ui.end_row();
                        });
                });
        });
    }

    /// Shows the empty state when no node is selected.
    fn show_empty_state(ui: &mut Ui, _theme: &Theme) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label("Nothing selected");
            ui.add_space(10.0);
            ui.label("Select a node to inspect details, or use the left navigation to focus, automate, or recall a setup.");
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.label("Need a quick orientation?");
                help_button(ui, "general", "nodes_and_ports");
            });
        });
    }
}

/// Converts decibels to linear amplitude.
fn db_to_linear(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

/// Response from the node panel.
#[derive(Debug, Default)]
pub struct NodePanelResponse {
    /// Mute toggle requested
    pub toggle_mute: Option<NodeId>,
    /// Volume changed (node_id, new_volume)
    pub volume_changed: Option<(NodeId, f32)>,
    /// Channel volume changed (node_id, channel, new_volume)
    pub channel_volume_changed: Option<(NodeId, usize, f32)>,
    /// Link to remove
    pub remove_link: Option<crate::util::id::LinkId>,
    /// Link to toggle (link_id, new_active_state)
    pub toggle_link: Option<(crate::util::id::LinkId, bool)>,
    /// Toggle uninteresting status for nodes
    pub toggle_uninteresting: Option<Vec<NodeId>>,
    /// Toggle a node's selection state (with shift = extend, without = replace)
    pub toggle_node_selection: Option<(NodeId, bool)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_panel_response_default() {
        let response = NodePanelResponse::default();
        assert!(response.toggle_mute.is_none());
        assert!(response.volume_changed.is_none());
    }
}
