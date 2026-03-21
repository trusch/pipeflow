//! Dedicated mixer views.
//!
//! Group mixer: balances member node levels directly. It is intentionally not a bus.
//! Node mixer: shows detailed master/channel/routing information for a single node.

use crate::core::state::GraphState;
use crate::domain::graph::{Node, PortDirection};
use crate::domain::groups::NodeGroup;
use crate::domain::mixer_node::MixerNodeState;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct MixerView {
    slider_overrides: HashMap<NodeId, f32>,
    channel_slider_overrides: HashMap<(NodeId, usize), f32>,
}

#[derive(Debug, Default)]
pub struct MixerViewResponse {
    pub back_to_graph: bool,
    pub volume_changes: Vec<(NodeId, f32)>,
    pub channel_volume_changes: Vec<(NodeId, usize, f32)>,
    pub mute_toggles: Vec<NodeId>,
    /// Mixer node: strip gain changes (strip_index, gain)
    pub strip_gain_changes: Vec<(usize, f32)>,
    /// Mixer node: strip mute toggles (strip_index, muted)
    pub strip_mute_toggles: Vec<(usize, bool)>,
    /// Mixer node: master gain change
    pub master_gain_change: Option<f32>,
    /// Mixer node: master mute toggle
    pub master_mute_toggle: Option<bool>,
}

struct MixerStrip {
    node_id: NodeId,
    name: String,
    subtitle: Option<String>,
    backend_volume: f32,
    effective_volume: f32,
    muted: bool,
    meter: f32,
    volume_failed: Option<String>,
}

struct ChannelStrip {
    node_id: NodeId,
    channel_index: usize,
    name: String,
    backend_volume: f32,
    effective_volume: f32,
    meter: f32,
    muted: bool,
}

struct RoutingRow {
    label: String,
    path: String,
    state: &'static str,
}

const MIXER_DB_MARKS: &[(f32, &str)] = &[
    (2.0, "+6"),
    (1.0, "0"),
    (0.707, "-3"),
    (0.5, "-6"),
    (0.25, "-12"),
    (0.125, "-18"),
    (0.0, "-∞"),
];

const MIXER_STRIP_WIDTH: f32 = 168.0;
const MIXER_STRIP_GAP: f32 = 16.0;
const MIXER_FADER_HEIGHT: f32 = 332.0;
const MIXER_CARD_HEIGHT: f32 = 560.0;

impl MixerView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes slider overrides for nodes that are no longer active.
    pub fn cleanup_stale_overrides(&mut self, active_node_ids: &HashSet<NodeId>) {
        self.slider_overrides
            .retain(|id, _| active_node_ids.contains(id));
        self.channel_slider_overrides
            .retain(|(id, _), _| active_node_ids.contains(id));
    }

    fn slider_value(&self, node_id: NodeId, backend_value: f32) -> f32 {
        self.slider_overrides
            .get(&node_id)
            .copied()
            .unwrap_or(backend_value)
    }

    fn sync_slider_override(&mut self, node_id: NodeId, backend_value: f32, ui_value: f32) {
        if (backend_value - ui_value).abs() < 0.01 {
            self.slider_overrides.remove(&node_id);
        } else {
            self.slider_overrides.insert(node_id, ui_value);
        }
    }

    fn channel_slider_value(&self, node_id: NodeId, channel: usize, backend_value: f32) -> f32 {
        self.channel_slider_overrides
            .get(&(node_id, channel))
            .copied()
            .unwrap_or(backend_value)
    }

    fn sync_channel_slider_override(
        &mut self,
        node_id: NodeId,
        channel: usize,
        backend_value: f32,
        ui_value: f32,
    ) {
        if (backend_value - ui_value).abs() < 0.01 {
            self.channel_slider_overrides.remove(&(node_id, channel));
        } else {
            self.channel_slider_overrides
                .insert((node_id, channel), ui_value);
        }
    }

    pub fn show_group(
        &mut self,
        ui: &mut egui::Ui,
        graph: &GraphState,
        group: &NodeGroup,
        theme: &Theme,
    ) -> MixerViewResponse {
        let mut response = MixerViewResponse::default();
        self.cleanup_stale_overrides(&group.members);
        let strips = self.collect_group_strips(graph, group);

        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(10, 12, 18))
            .inner_margin(egui::Margin::same(20))
            .show(ui, |ui| {
                self.show_group_header(ui, group, strips.len(), theme, &mut response, &strips);
                ui.add_space(18.0);

                if strips.is_empty() {
                    self.show_empty_state(
                        ui,
                        "No active members in this group",
                        "Bring the grouped nodes online and they will appear here as mixer strips.",
                    );
                    return;
                }

                let top_space = ((ui.available_height() - MIXER_CARD_HEIGHT) * 0.35).max(0.0);
                if top_space > 0.0 {
                    ui.add_space(top_space);
                }

                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let strip_count = strips.len() as f32;
                        let content_width = strip_count * MIXER_STRIP_WIDTH
                            + (strip_count - 1.0).max(0.0) * MIXER_STRIP_GAP;
                        let leading_space = ((ui.available_width() - content_width) * 0.5).max(0.0);

                        ui.horizontal_top(|ui| {
                            if leading_space > 0.0 {
                                ui.add_space(leading_space);
                            }

                            for (idx, strip) in strips.iter().enumerate() {
                                self.show_strip(ui, strip, theme, &mut response);
                                if idx + 1 != strips.len() {
                                    ui.add_space(MIXER_STRIP_GAP);
                                }
                            }
                        });
                    });
            });

        response
    }

    pub fn show_node(
        &mut self,
        ui: &mut egui::Ui,
        graph: &GraphState,
        node: &Node,
        theme: &Theme,
    ) -> MixerViewResponse {
        let mut response = MixerViewResponse::default();
        let active: HashSet<NodeId> = [node.id].into_iter().collect();
        self.cleanup_stale_overrides(&active);
        let Some(volume) = graph.volumes.get(&node.id) else {
            self.show_empty_state(
                ui,
                "No mixer data for this node",
                "This node has not published volume information yet.",
            );
            return response;
        };
        let strip = self.collect_node_strip(graph, node);
        let channels = self.collect_channel_strips(graph, node, volume);
        let inputs = self.collect_routing_rows(graph, node.id, true);
        let outputs = self.collect_routing_rows(graph, node.id, false);
        let input_port_count = graph
            .ports_for_node(&node.id)
            .iter()
            .filter(|port| port.direction == PortDirection::Input)
            .count();
        let output_port_count = graph
            .ports_for_node(&node.id)
            .iter()
            .filter(|port| port.direction == PortDirection::Output)
            .count();

        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(10, 12, 18))
            .inner_margin(egui::Margin::same(20))
            .show(ui, |ui| {
                self.show_node_header(ui, node, channels.len(), theme, &mut response);
                ui.add_space(18.0);

                ui.columns(2, |columns| {
                    columns[0].vertical(|ui| {
                        egui::ScrollArea::horizontal()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    self.show_strip(ui, &strip, theme, &mut response);
                                    for channel in &channels {
                                        ui.add_space(MIXER_STRIP_GAP);
                                        self.show_channel_strip(ui, channel, theme, &mut response);
                                    }
                                });
                            });
                    });

                    columns[1].vertical(|ui| {
                        self.show_node_summary_card(
                            ui,
                            node,
                            channels.len(),
                            input_port_count,
                            output_port_count,
                            inputs.len(),
                            outputs.len(),
                            theme,
                        );
                        ui.add_space(12.0);
                        self.show_routing_card(ui, "Receiving from", &inputs, theme);
                        ui.add_space(12.0);
                        self.show_routing_card(ui, "Sending to", &outputs, theme);
                    });
                });
            });

        response
    }

    fn show_group_header(
        &self,
        ui: &mut egui::Ui,
        group: &NodeGroup,
        member_count: usize,
        theme: &Theme,
        response: &mut MixerViewResponse,
        strips: &[MixerStrip],
    ) {
        let all_muted = !strips.is_empty() && strips.iter().all(|strip| strip.muted);
        let any_muted = strips.iter().any(|strip| strip.muted);
        let peak_meter = strips
            .iter()
            .map(|strip| strip.meter)
            .fold(0.0_f32, f32::max);

        ui.horizontal(|ui| {
            let back = egui::Button::new(format!(
                "{} Back to Patch",
                egui_phosphor::regular::ARROW_LEFT
            ))
            .corner_radius(10)
            .fill(egui::Color32::from_rgb(28, 34, 48))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 90, 120)))
            .min_size(egui::vec2(150.0, 34.0));
            if ui.add(back).clicked() {
                response.back_to_graph = true;
            }

            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(format!("{} Group Mixer", group.name));
                    let chip_text = format!("{} members", member_count);
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_unmultiplied(
                            group.color.r,
                            group.color.g,
                            group.color.b,
                            36,
                        ))
                        .stroke(egui::Stroke::new(1.0, group.color.to_color32()))
                        .corner_radius(255)
                        .inner_margin(egui::Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(chip_text)
                                    .color(theme.text.primary)
                                    .strong(),
                            );
                        });
                });
                ui.label(
                    egui::RichText::new(
                        "Direct member balancing only — this group mixer is not a real bus or master channel.",
                    )
                    .color(theme.text.muted),
                );
                ui.label(
                    egui::RichText::new(
                        "Shortcut: Ctrl+Shift+M opens the mixer for the selected group. Escape returns to the patch.",
                    )
                    .small()
                    .color(theme.text.muted),
                );
            });
        });

        ui.add_space(12.0);
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(18, 22, 31))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 68, 88)))
            .corner_radius(16)
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Live peak {}", Self::format_db(peak_meter)))
                            .color(theme.text.primary)
                            .strong(),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(match (all_muted, any_muted) {
                            (true, _) => "All members muted",
                            (false, true) => "Some members muted",
                            (false, false) => "All members unmuted",
                        })
                        .color(theme.text.muted),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new("Group actions affect member nodes directly.")
                            .color(theme.text.muted),
                    );
                });

                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    let mute_label = if all_muted {
                        "Unmute All Members"
                    } else {
                        "Mute All Members"
                    };
                    if ui.button(mute_label).clicked() {
                        let should_mute = !all_muted;
                        for strip in strips {
                            if strip.muted != should_mute {
                                response.mute_toggles.push(strip.node_id);
                            }
                        }
                    }

                    if ui.button("Reset All Levels to 0 dB").clicked() {
                        for strip in strips {
                            response.volume_changes.push((strip.node_id, 1.0));
                        }
                    }
                });
            });
    }

    fn show_node_header(
        &self,
        ui: &mut egui::Ui,
        node: &Node,
        channel_count: usize,
        theme: &Theme,
        response: &mut MixerViewResponse,
    ) {
        ui.horizontal(|ui| {
            let back = egui::Button::new(format!(
                "{} Back to Patch",
                egui_phosphor::regular::ARROW_LEFT
            ))
            .corner_radius(10)
            .fill(egui::Color32::from_rgb(28, 34, 48))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 90, 120)))
            .min_size(egui::vec2(150.0, 34.0));
            if ui.add(back).clicked() {
                response.back_to_graph = true;
            }

            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(format!("{} Node Mixer", node.display_name()));
                    let chip_text = format!("{} channels", channel_count.max(1));
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_unmultiplied(90, 140, 220, 28))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 140, 220)))
                        .corner_radius(255)
                        .inner_margin(egui::Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(chip_text)
                                    .color(theme.text.primary)
                                    .strong(),
                            );
                        });
                });
                ui.label(
                    egui::RichText::new(
                        "Detailed single-node view for master level, per-channel balancing, and current routing context.",
                    )
                    .color(theme.text.muted),
                );
                ui.label(
                    egui::RichText::new(
                        "This is not a synthetic group bus. It operates on the selected node and reuses its real volume/channel state.",
                    )
                    .small()
                    .color(theme.text.muted),
                );
            });
        });
    }

    fn show_empty_state(&self, ui: &mut egui::Ui, heading: &str, body: &str) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading(heading);
            ui.label(body);
        });
    }

    fn collect_group_strips(&self, graph: &GraphState, group: &NodeGroup) -> Vec<MixerStrip> {
        let mut strips: Vec<_> = group
            .members
            .iter()
            .filter_map(|node_id| {
                let node = graph.get_node(node_id)?;
                Some(self.collect_node_strip(graph, node))
            })
            .collect();

        strips.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        strips
    }

    fn collect_node_strip(&self, graph: &GraphState, node: &Node) -> MixerStrip {
        let volume = graph.volumes.get(&node.id).cloned().unwrap_or_default();
        let meter = graph
            .meters
            .get(&node.id)
            .map(|m| m.get_decayed_max_peak(std::time::Duration::from_millis(180)))
            .unwrap_or(0.0);
        let backend_volume = volume.master;
        MixerStrip {
            node_id: node.id,
            name: node.display_name().to_string(),
            subtitle: node
                .media_class
                .as_ref()
                .map(|m| m.display_name().to_string()),
            backend_volume,
            effective_volume: self.slider_value(node.id, backend_volume),
            muted: volume.muted,
            meter,
            volume_failed: graph.volume_control_failed.get(&node.id).cloned(),
        }
    }

    fn collect_channel_strips(
        &self,
        graph: &GraphState,
        node: &Node,
        volume: &crate::domain::audio::VolumeControl,
    ) -> Vec<ChannelStrip> {
        let meter = graph.meters.get(&node.id);
        volume
            .channels
            .iter()
            .enumerate()
            .map(|(channel_index, &backend_volume)| ChannelStrip {
                node_id: node.id,
                channel_index,
                name: format!("Ch {}", channel_index + 1),
                backend_volume,
                effective_volume: self.channel_slider_value(node.id, channel_index, backend_volume),
                meter: meter
                    .map(|m| {
                        m.get_decayed_peak(channel_index, std::time::Duration::from_millis(180))
                    })
                    .unwrap_or(0.0),
                muted: volume.muted,
            })
            .collect()
    }

    fn collect_routing_rows(
        &self,
        graph: &GraphState,
        node_id: NodeId,
        receiving: bool,
    ) -> Vec<RoutingRow> {
        let mut rows: Vec<_> = graph
            .links_for_node(&node_id)
            .into_iter()
            .filter(|link| {
                if receiving {
                    link.input_node == node_id
                } else {
                    link.output_node == node_id
                }
            })
            .map(|link| {
                let other_node_id = if receiving {
                    link.output_node
                } else {
                    link.input_node
                };
                let other_name = graph
                    .get_node(&other_node_id)
                    .map(|node| node.display_name().to_string())
                    .unwrap_or_else(|| "Unknown node".to_string());
                let local_port = graph
                    .get_port(
                        &(if receiving {
                            link.input_port
                        } else {
                            link.output_port
                        }),
                    )
                    .map(|port| port.display_name().to_string())
                    .unwrap_or_else(|| "Unknown port".to_string());
                let remote_port = graph
                    .get_port(
                        &(if receiving {
                            link.output_port
                        } else {
                            link.input_port
                        }),
                    )
                    .map(|port| port.display_name().to_string())
                    .unwrap_or_else(|| "Unknown port".to_string());
                RoutingRow {
                    label: other_name,
                    path: if receiving {
                        format!("{} → {}", remote_port, local_port)
                    } else {
                        format!("{} → {}", local_port, remote_port)
                    },
                    state: if link.is_active { "live" } else { "paused" },
                }
            })
            .collect();
        rows.sort_by(|a, b| a.label.cmp(&b.label));
        rows
    }

    fn show_strip(
        &mut self,
        ui: &mut egui::Ui,
        strip: &MixerStrip,
        theme: &Theme,
        response: &mut MixerViewResponse,
    ) {
        let card_fill = egui::Color32::from_rgb(20, 24, 34);
        let card_stroke = if strip.muted {
            egui::Color32::from_rgb(110, 70, 70)
        } else {
            egui::Color32::from_rgb(52, 63, 86)
        };

        egui::Frame::NONE
            .fill(card_fill)
            .stroke(egui::Stroke::new(1.0, card_stroke))
            .corner_radius(18)
            .inner_margin(egui::Margin::symmetric(16, 16))
            .show(ui, |ui| {
                ui.set_width(MIXER_STRIP_WIDTH);
                ui.allocate_ui_with_layout(
                    egui::vec2(MIXER_STRIP_WIDTH, MIXER_CARD_HEIGHT),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.label(
                            egui::RichText::new(&strip.name)
                                .strong()
                                .size(16.0)
                                .color(theme.text.primary),
                        );
                        if let Some(subtitle) = &strip.subtitle {
                            ui.label(
                                egui::RichText::new(subtitle)
                                    .small()
                                    .color(theme.text.muted),
                            );
                        }
                        ui.add_space(8.0);

                        ui.horizontal_top(|ui| {
                            self.draw_db_scale(ui, theme);
                            ui.add_space(8.0);

                            let mut slider_value = strip.effective_volume;
                            let slider_size = egui::vec2(48.0, MIXER_FADER_HEIGHT);
                            let mut style = ui.style().as_ref().clone();
                            style.spacing.slider_width = 240.0;
                            style.visuals.widgets.active.bg_fill =
                                egui::Color32::from_rgb(102, 162, 255);
                            style.visuals.widgets.hovered.bg_fill =
                                egui::Color32::from_rgb(124, 180, 255);
                            style.visuals.widgets.inactive.bg_fill =
                                egui::Color32::from_rgb(39, 45, 58);
                            style.visuals.widgets.inactive.weak_bg_fill =
                                egui::Color32::from_rgb(28, 33, 44);
                            ui.scope(|ui| {
                                ui.set_style(style);
                                let slider = egui::Slider::new(&mut slider_value, 0.0..=2.0)
                                    .vertical()
                                    .show_value(false)
                                    .step_by(0.01)
                                    .trailing_fill(true)
                                    .handle_shape(egui::style::HandleShape::Rect {
                                        aspect_ratio: 0.55,
                                    });
                                let resp = ui.add_sized(slider_size, slider);
                                if resp.double_clicked() {
                                    slider_value = 1.0;
                                }
                                if resp.changed() || resp.double_clicked() {
                                    self.sync_slider_override(
                                        strip.node_id,
                                        strip.backend_volume,
                                        slider_value,
                                    );
                                    response.volume_changes.push((strip.node_id, slider_value));
                                } else {
                                    self.sync_slider_override(
                                        strip.node_id,
                                        strip.backend_volume,
                                        slider_value,
                                    );
                                }
                                resp.on_hover_text(
                                    "Drag to set volume. Double-click to reset to unity (0 dB).",
                                );
                            });

                            ui.add_space(10.0);
                            self.draw_level_meter(ui, strip.meter, strip.muted, slider_size.y);
                        });

                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(Self::format_volume_pair(strip.effective_volume))
                                .monospace()
                                .size(18.0)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        let mute_text = if strip.muted {
                            format!("{} Muted", egui_phosphor::regular::SPEAKER_SLASH)
                        } else {
                            format!("{} Mute", egui_phosphor::regular::SPEAKER_HIGH)
                        };
                        let mute_fill = if strip.muted {
                            egui::Color32::from_rgb(120, 46, 46)
                        } else {
                            egui::Color32::from_rgb(35, 42, 56)
                        };
                        if ui
                            .add(
                                egui::Button::new(mute_text)
                                    .fill(mute_fill)
                                    .corner_radius(10)
                                    .min_size(egui::vec2(112.0, 32.0)),
                            )
                            .clicked()
                        {
                            response.mute_toggles.push(strip.node_id);
                        }

                        if let Some(err) = &strip.volume_failed {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    egui_phosphor::regular::WARNING,
                                    err
                                ))
                                .small()
                                .color(egui::Color32::from_rgb(255, 210, 120)),
                            );
                        }
                    },
                );
            });
    }

    fn show_channel_strip(
        &mut self,
        ui: &mut egui::Ui,
        strip: &ChannelStrip,
        theme: &Theme,
        response: &mut MixerViewResponse,
    ) {
        let card_fill = egui::Color32::from_rgb(18, 22, 31);
        let card_stroke = if strip.muted {
            egui::Color32::from_rgb(110, 70, 70)
        } else {
            egui::Color32::from_rgb(70, 88, 118)
        };

        egui::Frame::NONE
            .fill(card_fill)
            .stroke(egui::Stroke::new(1.0, card_stroke))
            .corner_radius(18)
            .inner_margin(egui::Margin::symmetric(16, 16))
            .show(ui, |ui| {
                ui.set_width(MIXER_STRIP_WIDTH * 0.9);
                ui.allocate_ui_with_layout(
                    egui::vec2(MIXER_STRIP_WIDTH * 0.9, MIXER_CARD_HEIGHT),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.label(
                            egui::RichText::new(&strip.name)
                                .strong()
                                .size(16.0)
                                .color(theme.text.primary),
                        );
                        ui.label(
                            egui::RichText::new("Per-channel level")
                                .small()
                                .color(theme.text.muted),
                        );
                        ui.add_space(8.0);

                        ui.horizontal_top(|ui| {
                            self.draw_db_scale(ui, theme);
                            ui.add_space(8.0);

                            let mut slider_value = strip.effective_volume;
                            let slider_size = egui::vec2(42.0, MIXER_FADER_HEIGHT);
                            let resp = ui.add_sized(
                                slider_size,
                                egui::Slider::new(&mut slider_value, 0.0..=2.0)
                                    .vertical()
                                    .show_value(false)
                                    .step_by(0.01)
                                    .trailing_fill(true)
                                    .handle_shape(egui::style::HandleShape::Rect {
                                        aspect_ratio: 0.55,
                                    }),
                            );
                            if resp.double_clicked() {
                                slider_value = 1.0;
                            }
                            if resp.changed() || resp.double_clicked() {
                                self.sync_channel_slider_override(
                                    strip.node_id,
                                    strip.channel_index,
                                    strip.backend_volume,
                                    slider_value,
                                );
                                response.channel_volume_changes.push((
                                    strip.node_id,
                                    strip.channel_index,
                                    slider_value,
                                ));
                            } else {
                                self.sync_channel_slider_override(
                                    strip.node_id,
                                    strip.channel_index,
                                    strip.backend_volume,
                                    slider_value,
                                );
                            }
                            resp.on_hover_text(
                                "Drag to set the per-channel level. Double-click to reset to unity (0 dB).",
                            );

                            ui.add_space(10.0);
                            self.draw_level_meter(ui, strip.meter, strip.muted, slider_size.y);
                        });

                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(Self::format_volume_pair(strip.effective_volume))
                                .monospace()
                                .size(18.0)
                                .strong(),
                        );
                    },
                );
            });
    }

    fn draw_db_scale(&self, ui: &mut egui::Ui, theme: &Theme) {
        let height = MIXER_FADER_HEIGHT;
        let width = 30.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let painter = ui.painter();
        for (value, label) in MIXER_DB_MARKS {
            let y = rect.bottom() - (value / 2.0).clamp(0.0, 1.0) * rect.height();
            painter.line_segment(
                [
                    egui::pos2(rect.right() - 6.0, y),
                    egui::pos2(rect.right(), y),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 90, 110)),
            );
            painter.text(
                egui::pos2(rect.left(), y),
                egui::Align2::LEFT_CENTER,
                *label,
                egui::FontId::monospace(11.0),
                theme.text.muted,
            );
        }
    }

    fn draw_level_meter(&self, ui: &mut egui::Ui, level: f32, muted: bool, height: f32) {
        let desired = egui::vec2(16.0, height);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter();
        let bg = egui::Color32::from_rgb(22, 26, 34);
        painter.rect_filled(rect, 8.0, bg);

        let segments = 24;
        let gap = 2.0;
        let segment_height = (rect.height() - gap * (segments as f32 - 1.0)) / segments as f32;
        let active_segments = ((level.clamp(0.0, 1.2) / 1.2) * segments as f32).ceil() as usize;

        for i in 0..segments {
            let top = rect.bottom() - (i as f32 + 1.0) * segment_height - i as f32 * gap;
            let seg_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), top),
                egui::vec2(rect.width(), segment_height),
            );
            let is_active = i < active_segments;
            let color = if !is_active {
                egui::Color32::from_rgb(34, 39, 48)
            } else if muted {
                egui::Color32::from_rgb(120, 70, 70)
            } else if i > 18 {
                egui::Color32::from_rgb(255, 94, 94)
            } else if i > 14 {
                egui::Color32::from_rgb(255, 206, 96)
            } else {
                egui::Color32::from_rgb(88, 218, 152)
            };
            painter.rect_filled(seg_rect, 2.0, color);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_node_summary_card(
        &self,
        ui: &mut egui::Ui,
        node: &Node,
        channel_count: usize,
        input_port_count: usize,
        output_port_count: usize,
        input_link_count: usize,
        output_link_count: usize,
        theme: &Theme,
    ) {
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(18, 22, 31))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 68, 88)))
            .corner_radius(16)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.heading("Node summary");
                ui.add_space(8.0);
                egui::Grid::new(("node_mixer_summary", node.id.raw()))
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Media class");
                        ui.label(
                            node.media_class
                                .as_ref()
                                .map(|m| m.display_name().to_string())
                                .unwrap_or_else(|| "Unknown".to_string()),
                        );
                        ui.end_row();

                        ui.label("Channels");
                        ui.label(channel_count.max(1).to_string());
                        ui.end_row();

                        ui.label("Input ports");
                        ui.label(input_port_count.to_string());
                        ui.end_row();

                        ui.label("Output ports");
                        ui.label(output_port_count.to_string());
                        ui.end_row();

                        ui.label("Incoming links");
                        ui.label(input_link_count.to_string());
                        ui.end_row();

                        ui.label("Outgoing links");
                        ui.label(output_link_count.to_string());
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Routing shown here is live graph context. Level controls operate on this node only.",
                    )
                    .small()
                    .color(theme.text.muted),
                );
            });
    }

    fn show_routing_card(
        &self,
        ui: &mut egui::Ui,
        heading: &str,
        rows: &[RoutingRow],
        theme: &Theme,
    ) {
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(18, 22, 31))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 68, 88)))
            .corner_radius(16)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.heading(heading);
                ui.add_space(8.0);
                if rows.is_empty() {
                    ui.label(egui::RichText::new("No live connections").color(theme.text.muted));
                    return;
                }
                for row in rows {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(24, 28, 38))
                        .corner_radius(10)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&row.label)
                                    .strong()
                                    .color(theme.text.primary),
                            );
                            ui.label(
                                egui::RichText::new(&row.path)
                                    .small()
                                    .color(theme.text.muted),
                            );
                            ui.label(
                                egui::RichText::new(row.state)
                                    .small()
                                    .color(theme.text.muted),
                            );
                        });
                    ui.add_space(6.0);
                }
            });
    }

    /// Renders the mixer-node view (graph-native mixer created by pipeflow).
    pub fn show_mixer_node(
        &mut self,
        ui: &mut egui::Ui,
        mixer_state: &MixerNodeState,
        theme: &Theme,
        strip_meters: &[f32],
        master_meter: f32,
    ) -> MixerViewResponse {
        let mut response = MixerViewResponse::default();

        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(10, 12, 18))
            .inner_margin(egui::Margin::same(20))
            .show(ui, |ui| {
                // Header — unified with group/node mixer style
                ui.horizontal(|ui| {
                    let back = egui::Button::new(format!(
                        "{} Back to Patch",
                        egui_phosphor::regular::ARROW_LEFT
                    ))
                    .corner_radius(10)
                    .fill(egui::Color32::from_rgb(28, 34, 48))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 90, 120)))
                    .min_size(egui::vec2(150.0, 34.0));
                    if ui.add(back).clicked() {
                        response.back_to_graph = true;
                    }

                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.heading(format!("{} Mixer Node", mixer_state.name));
                            let chip_text = format!("{} strips", mixer_state.strip_count());
                            egui::Frame::NONE
                                .fill(egui::Color32::from_rgba_unmultiplied(90, 140, 220, 28))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(90, 140, 220),
                                ))
                                .corner_radius(255)
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(chip_text)
                                            .color(theme.text.primary)
                                            .strong(),
                                    );
                                });
                        });
                        ui.label(
                            egui::RichText::new(
                                "Graph-native mixer node created by pipeflow (pw-loopback).",
                            )
                            .color(theme.text.muted),
                        );
                    });
                });
                ui.add_space(18.0);

                let top_space = ((ui.available_height() - MIXER_CARD_HEIGHT) * 0.35).max(0.0);
                if top_space > 0.0 {
                    ui.add_space(top_space);
                }

                // Strips area — horizontal scroll
                egui::ScrollArea::horizontal()
                    .id_salt("mixer_node_strips")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let strip_count = mixer_state.strip_count() as f32 + 1.0; // +1 for master
                        let content_width = strip_count * MIXER_STRIP_WIDTH
                            + (strip_count - 1.0).max(0.0) * MIXER_STRIP_GAP;
                        let leading_space = ((ui.available_width() - content_width) * 0.5).max(0.0);

                        ui.horizontal_top(|ui| {
                            if leading_space > 0.0 {
                                ui.add_space(leading_space);
                            }

                            // Input strips
                            for (i, strip) in mixer_state.strips.iter().enumerate() {
                                let card_fill = egui::Color32::from_rgb(20, 24, 34);
                                let card_stroke = if strip.muted {
                                    egui::Color32::from_rgb(110, 70, 70)
                                } else {
                                    egui::Color32::from_rgb(52, 63, 86)
                                };

                                egui::Frame::NONE
                                    .fill(card_fill)
                                    .stroke(egui::Stroke::new(1.0, card_stroke))
                                    .corner_radius(18)
                                    .inner_margin(egui::Margin::symmetric(16, 16))
                                    .show(ui, |ui| {
                                        ui.set_width(MIXER_STRIP_WIDTH);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(MIXER_STRIP_WIDTH, MIXER_CARD_HEIGHT),
                                            egui::Layout::top_down(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(&strip.label)
                                                        .strong()
                                                        .size(16.0)
                                                        .color(theme.text.primary),
                                                );
                                                ui.add_space(8.0);

                                                ui.horizontal_top(|ui| {
                                                    self.draw_db_scale(ui, theme);
                                                    ui.add_space(8.0);

                                                    let mut gain = strip.gain;
                                                    let slider_size =
                                                        egui::vec2(48.0, MIXER_FADER_HEIGHT);
                                                    let slider =
                                                        egui::Slider::new(&mut gain, 0.0..=2.0)
                                                            .vertical()
                                                            .show_value(false)
                                                            .step_by(0.01)
                                                            .trailing_fill(true)
                                                            .handle_shape(
                                                                egui::style::HandleShape::Rect {
                                                                    aspect_ratio: 0.55,
                                                                },
                                                            );
                                                    let resp = ui.add_sized(slider_size, slider);
                                                    if resp.double_clicked() {
                                                        gain = 1.0;
                                                    }
                                                    if resp.changed() || resp.double_clicked() {
                                                        response.strip_gain_changes.push((i, gain));
                                                    }

                                                    ui.add_space(10.0);
                                                    let meter_level =
                                                        strip_meters.get(i).copied().unwrap_or(0.0);
                                                    self.draw_level_meter(
                                                        ui,
                                                        meter_level,
                                                        strip.muted,
                                                        slider_size.y,
                                                    );
                                                });

                                                ui.add_space(10.0);
                                                ui.label(
                                                    egui::RichText::new(Self::format_volume_pair(
                                                        strip.gain,
                                                    ))
                                                    .monospace()
                                                    .size(18.0)
                                                    .strong(),
                                                );
                                                ui.add_space(8.0);

                                                // Mute button
                                                let mute_text = if strip.muted {
                                                    format!(
                                                        "{} Muted",
                                                        egui_phosphor::regular::SPEAKER_SLASH
                                                    )
                                                } else {
                                                    format!(
                                                        "{} Mute",
                                                        egui_phosphor::regular::SPEAKER_HIGH
                                                    )
                                                };
                                                let mute_fill = if strip.muted {
                                                    egui::Color32::from_rgb(120, 46, 46)
                                                } else {
                                                    egui::Color32::from_rgb(35, 42, 56)
                                                };
                                                if ui
                                                    .add(
                                                        egui::Button::new(mute_text)
                                                            .fill(mute_fill)
                                                            .corner_radius(10)
                                                            .min_size(egui::vec2(112.0, 32.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    response
                                                        .strip_mute_toggles
                                                        .push((i, !strip.muted));
                                                }
                                            },
                                        );
                                    });
                                ui.add_space(MIXER_STRIP_GAP);
                            }

                            // Styled separator line between strips and master
                            let sep_rect = ui.allocate_exact_size(
                                egui::vec2(2.0, MIXER_CARD_HEIGHT + 32.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                sep_rect.0,
                                1.0,
                                egui::Color32::from_rgb(52, 63, 86),
                            );
                            ui.add_space(MIXER_STRIP_GAP);

                            // Master strip
                            egui::Frame::NONE
                                .fill(egui::Color32::from_rgb(16, 20, 30))
                                .stroke(egui::Stroke::new(2.0, theme.text.accent))
                                .corner_radius(18)
                                .inner_margin(egui::Margin::symmetric(16, 16))
                                .show(ui, |ui| {
                                    ui.set_width(MIXER_STRIP_WIDTH);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(MIXER_STRIP_WIDTH, MIXER_CARD_HEIGHT),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new("MASTER")
                                                    .strong()
                                                    .size(16.0)
                                                    .color(theme.text.accent),
                                            );
                                            ui.add_space(8.0);

                                            ui.horizontal_top(|ui| {
                                                self.draw_db_scale(ui, theme);
                                                ui.add_space(8.0);

                                                let mut master_gain = mixer_state.master_gain;
                                                let slider_size =
                                                    egui::vec2(48.0, MIXER_FADER_HEIGHT);
                                                let slider =
                                                    egui::Slider::new(&mut master_gain, 0.0..=2.0)
                                                        .vertical()
                                                        .show_value(false)
                                                        .step_by(0.01)
                                                        .trailing_fill(true)
                                                        .handle_shape(
                                                            egui::style::HandleShape::Rect {
                                                                aspect_ratio: 0.55,
                                                            },
                                                        );
                                                let resp = ui.add_sized(slider_size, slider);
                                                if resp.double_clicked() {
                                                    master_gain = 1.0;
                                                }
                                                if resp.changed() || resp.double_clicked() {
                                                    response.master_gain_change = Some(master_gain);
                                                }

                                                ui.add_space(10.0);
                                                self.draw_level_meter(
                                                    ui,
                                                    master_meter,
                                                    mixer_state.master_muted,
                                                    slider_size.y,
                                                );
                                            });

                                            ui.add_space(10.0);
                                            ui.label(
                                                egui::RichText::new(Self::format_volume_pair(
                                                    mixer_state.master_gain,
                                                ))
                                                .monospace()
                                                .size(18.0)
                                                .strong(),
                                            );
                                            ui.add_space(8.0);

                                            // Master mute button
                                            let mute_text = if mixer_state.master_muted {
                                                format!(
                                                    "{} Muted",
                                                    egui_phosphor::regular::SPEAKER_SLASH
                                                )
                                            } else {
                                                format!(
                                                    "{} Mute",
                                                    egui_phosphor::regular::SPEAKER_HIGH
                                                )
                                            };
                                            let mute_fill = if mixer_state.master_muted {
                                                egui::Color32::from_rgb(120, 46, 46)
                                            } else {
                                                egui::Color32::from_rgb(35, 42, 56)
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(mute_text)
                                                        .fill(mute_fill)
                                                        .corner_radius(10)
                                                        .min_size(egui::vec2(112.0, 32.0)),
                                                )
                                                .clicked()
                                            {
                                                response.master_mute_toggle =
                                                    Some(!mixer_state.master_muted);
                                            }
                                        },
                                    );
                                });
                        });
                    });
            });

        response
    }

    fn format_db(volume: f32) -> String {
        if volume <= 0.0001 {
            "−∞ dB".to_string()
        } else {
            format!("{:+.1} dB", 20.0 * volume.log10())
        }
    }

    fn format_volume_pair(volume: f32) -> String {
        format!("{} ({:.0}%)", Self::format_db(volume), volume * 100.0)
    }
}
