//! Node inspection panel.
//!
//! Shows detailed information about selected nodes.

use crate::core::state::GraphState;
use crate::domain::audio::VolumeControl;
use crate::domain::explain::explain_node;
use crate::domain::graph::{Link, Node, Port, PortDirection};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;
use egui::{RichText, Ui};
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
            if let Some(node) = graph.get_node(&node_ids[0]) {
                let is_uninteresting = uninteresting_nodes.contains(&node.id);
                Self::show_node_details(ui, node, graph, is_uninteresting, theme, &mut response);
            } else {
                Self::show_empty_state(ui, theme);
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.heading(format!("{} selected", node_ids.len()));
                if ui.button("Mark as Background").clicked() {
                    response.toggle_uninteresting = Some(node_ids.to_vec());
                }
            });
            ui.add_space(6.0);

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
                        ui.add_space(4.0);
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
        let explanation = explain_node(node, graph);
        let meter_peak = graph
            .meters
            .get(&node.id)
            .map(|meter| meter.max_peak())
            .unwrap_or(0.0);
        let has_signal = meter_peak > 0.02;
        let volume_failed = graph.volume_control_failed.get(&node.id).cloned();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.heading(node.display_name());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let active_color = if node.is_active {
                        egui::Color32::from_rgb(100, 220, 140)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.colored_label(active_color, if node.is_active { "Active" } else { "Idle" });

                    if has_signal {
                        ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "Signal present");
                    }

                    if is_uninteresting {
                        ui.colored_label(ui.visuals().weak_text_color(), "Background node");
                    }

                    if volume_failed.is_some() {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 200, 100),
                            "Volume unavailable",
                        );
                    }

                    if let Some(media_class) = &node.media_class {
                        ui.label(media_class.display_name());
                    }
                });

                ui.add_space(6.0);
                ui.label(explanation);

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Rename…").clicked() {
                        response.rename_node = Some(node.id);
                    }

                    let button_text = if is_uninteresting {
                        "Bring Into Focus"
                    } else {
                        "Mark as Background"
                    };
                    if ui.button(button_text).clicked() {
                        response.toggle_uninteresting = Some(vec![node.id]);
                    }
                });
            });

        if let Some(volume) = graph.volumes.get(&node.id) {
            ui.add_space(8.0);
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.heading("Volume");

                    if let Some(error_msg) = volume_failed.as_deref() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
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

                    let _ = Self::show_volume_control(ui, node.id, volume, response);
                });
        }

        ui.add_space(8.0);
        Self::show_relationships(ui, node, graph, response);

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Details")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new(format!("node_metadata_{}", node.id.raw()))
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
                    });

                if let Some(ref format) = node.format {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new("Audio Format")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("");
                                help_button(ui, "audio", "audio_basics");
                            });
                            egui::Grid::new(format!("node_format_{}", node.id.raw())).show(
                                ui,
                                |ui| {
                                    ui.label("Sample Rate:");
                                    ui.label(format!("{} Hz", format.sample_rate));
                                    ui.end_row();

                                    ui.label("Channels:");
                                    ui.label(format!("{}", format.channels));
                                    ui.end_row();

                                    ui.label("Format:");
                                    ui.label(&format.format);
                                    ui.end_row();
                                },
                            );
                        });
                }

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
                    ui.add_space(8.0);
                    ui.collapsing(format!("Input Ports ({})", input_ports.len()), |ui| {
                        for port in input_ports {
                            Self::show_port(ui, port, theme);
                        }
                    });
                }

                if !output_ports.is_empty() {
                    ui.add_space(8.0);
                    ui.collapsing(format!("Output Ports ({})", output_ports.len()), |ui| {
                        for port in output_ports {
                            Self::show_port(ui, port, theme);
                        }
                    });
                }
            });
    }

    fn show_relationships(
        ui: &mut Ui,
        node: &Node,
        graph: &GraphState,
        response: &mut NodePanelResponse,
    ) {
        let mut receiving: Vec<_> = graph
            .links_for_node(&node.id)
            .into_iter()
            .filter(|link| link.input_node == node.id)
            .collect();
        receiving.sort_by_key(|link| link.id.raw());

        let mut sending: Vec<_> = graph
            .links_for_node(&node.id)
            .into_iter()
            .filter(|link| link.output_node == node.id)
            .collect();
        sending.sort_by_key(|link| link.id.raw());

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.heading("Routing");
                ui.add_space(8.0);

                ui.columns(2, |columns| {
                    Self::show_relationship_column(
                        &mut columns[0],
                        "Receiving from",
                        "No inputs",
                        node.id,
                        &receiving,
                        graph,
                        response,
                    );
                    Self::show_relationship_column(
                        &mut columns[1],
                        "Sending to",
                        "No outputs",
                        node.id,
                        &sending,
                        graph,
                        response,
                    );
                });
            });
    }

    fn show_relationship_column(
        ui: &mut Ui,
        title: &str,
        empty_label: &str,
        node_id: NodeId,
        links: &[&Link],
        graph: &GraphState,
        response: &mut NodePanelResponse,
    ) {
        ui.strong(title);
        ui.add_space(4.0);

        if links.is_empty() {
            ui.weak(empty_label);
            return;
        }

        for link in links {
            let (other_node_id, source_port, dest_port) = if link.output_node == node_id {
                (link.input_node, link.output_port, link.input_port)
            } else {
                (link.output_node, link.output_port, link.input_port)
            };

            let other_name = graph
                .get_node(&other_node_id)
                .map(|n| n.display_name().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let source_port_name = graph
                .get_port(&source_port)
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let dest_port_name = graph
                .get_port(&dest_port)
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            egui::Frame::NONE
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(6)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
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

                        if ui
                            .small_button(egui_phosphor::regular::X)
                            .on_hover_text("Remove connection")
                            .clicked()
                        {
                            response.remove_link = Some(link.id);
                        }

                        let name_response = ui.add(
                            egui::Label::new(egui::RichText::new(&other_name).underline())
                                .sense(egui::Sense::click()),
                        );
                        if name_response.clicked() {
                            let modifiers = ui.input(|i| i.modifiers);
                            response.toggle_node_selection = Some((other_node_id, modifiers.shift));
                        }
                        name_response.on_hover_text("Click to select, Shift+click to extend");

                        let status = if link.is_active { "live" } else { "paused" };
                        ui.weak(format!(
                            "{} → {} ({})",
                            source_port_name, dest_port_name, status
                        ));
                    });
                });
            ui.add_space(4.0);
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

        let show_db =
            ui.data_mut(|d| *d.get_persisted_mut_or(egui::Id::new("volume_display_db"), false));

        let content_width = ui.available_width();

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

        let mut master = volume.master;

        ui.horizontal(|ui| {
            ui.strong("Master:");
            ui.add_space(8.0);
            if show_db {
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
        ui.horizontal(|ui| {
            let mut db_mode = show_db;
            if ui.checkbox(&mut db_mode, "Show dB").changed() {
                ui.data_mut(|d| {
                    d.insert_persisted(egui::Id::new("volume_display_db"), db_mode);
                });
            }
        });

        if volume.channels.len() > 1 {
            ui.add_space(4.0);
            egui::CollapsingHeader::new(format!("Channels ({})", volume.channels.len())).show(
                ui,
                |ui| {
                    help_button(ui, "audio", "channels_explained");

                    let ch_content_width = ui.available_width();

                    for (i, &ch_vol) in volume.channels.iter().enumerate() {
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
        let color = theme.port_color(
            port.direction,
            true,
            false,
            false,
            port.is_control,
            port.is_monitor,
        );

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
                            ui.add(egui::Label::new(port.full_display_name()).wrap());
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
            ui.add_space(40.0);
            ui.label(
                RichText::new(egui_phosphor::regular::CURSOR_CLICK)
                    .size(32.0)
                    .weak(),
            );
            ui.add_space(8.0);
            ui.weak("Select a node");
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
    /// Open the rename dialog for a node
    pub rename_node: Option<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_panel_response_default() {
        let response = NodePanelResponse::default();
        assert!(response.toggle_mute.is_none());
        assert!(response.volume_changed.is_none());
        assert!(response.rename_node.is_none());
    }
}
