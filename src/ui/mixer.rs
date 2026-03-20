//! Group mixer view.
//!
//! Renders a dedicated mixer-style central view for the members of a node group.

use crate::core::state::GraphState;
use crate::domain::groups::NodeGroup;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;

#[derive(Debug, Default)]
pub struct MixerView;

#[derive(Debug, Default)]
pub struct MixerViewResponse {
    pub back_to_graph: bool,
    pub volume_changes: Vec<(NodeId, f32)>,
    pub mute_toggles: Vec<NodeId>,
}

struct MixerStrip {
    node_id: NodeId,
    name: String,
    subtitle: Option<String>,
    volume: f32,
    muted: bool,
    meter: f32,
    volume_failed: Option<String>,
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

impl MixerView {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &GraphState,
        group: &NodeGroup,
        theme: &Theme,
    ) -> MixerViewResponse {
        let mut response = MixerViewResponse::default();
        let strips = self.collect_strips(graph, group);

        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(10, 12, 18))
            .inner_margin(egui::Margin::same(20))
            .show(ui, |ui| {
                self.show_header(ui, group, strips.len(), theme, &mut response);
                ui.add_space(18.0);

                if strips.is_empty() {
                    self.show_empty_state(ui);
                    return;
                }

                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            for strip in &strips {
                                self.show_strip(ui, strip, theme, &mut response);
                                ui.add_space(16.0);
                            }

                            // Vertical separator before master strip
                            let sep_height = 420.0;
                            let (sep_rect, _) = ui.allocate_exact_size(
                                egui::vec2(2.0, sep_height),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                sep_rect,
                                1.0,
                                egui::Color32::from_rgb(60, 70, 90),
                            );
                            ui.add_space(16.0);

                            self.show_master_strip(ui, &strips, group, theme, &mut response);
                        });
                    });
            });

        response
    }

    fn show_header(
        &self,
        ui: &mut egui::Ui,
        group: &NodeGroup,
        member_count: usize,
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
                    ui.heading(format!("{} Mixer", group.name));
                    let chip_text = format!("{} channels", member_count);
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
                        "Dedicated group mixer view with direct level control for every member.",
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
    }

    fn show_empty_state(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("No active members in this group");
            ui.label("Bring the grouped nodes online and they will appear here as mixer strips.");
        });
    }

    fn collect_strips(&self, graph: &GraphState, group: &NodeGroup) -> Vec<MixerStrip> {
        let mut strips: Vec<_> = group
            .members
            .iter()
            .filter_map(|node_id| {
                let node = graph.get_node(node_id)?;
                let volume = graph.volumes.get(node_id)?;
                let meter = graph
                    .meters
                    .get(node_id)
                    .map(|m| m.get_decayed_max_peak(std::time::Duration::from_millis(180)))
                    .unwrap_or(0.0);
                Some(MixerStrip {
                    node_id: *node_id,
                    name: node.display_name().to_string(),
                    subtitle: node
                        .media_class
                        .as_ref()
                        .map(|m| m.display_name().to_string()),
                    volume: volume.master,
                    muted: volume.muted,
                    meter,
                    volume_failed: graph.volume_control_failed.get(node_id).cloned(),
                })
            })
            .collect();

        strips.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        strips
    }

    fn show_strip(
        &self,
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
                ui.set_width(164.0);
                ui.vertical_centered(|ui| {
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
                    ui.add_space(12.0);

                    ui.horizontal_top(|ui| {
                        self.draw_db_scale(ui, theme);
                        ui.add_space(8.0);

                        let mut slider_value = strip.volume;
                        let slider_size = egui::vec2(46.0, 272.0);
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
                            if resp.changed() {
                                response.volume_changes.push((strip.node_id, slider_value));
                            }
                            if resp.double_clicked() {
                                response.volume_changes.push((strip.node_id, 1.0));
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
                        egui::RichText::new(format!("{:.0}%", strip.volume * 100.0))
                            .monospace()
                            .size(20.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(Self::format_db(strip.volume))
                            .monospace()
                            .small()
                            .color(theme.text.muted),
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
                });
            });
    }

    fn draw_db_scale(&self, ui: &mut egui::Ui, theme: &Theme) {
        let height = 272.0;
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

    fn show_master_strip(
        &self,
        ui: &mut egui::Ui,
        strips: &[MixerStrip],
        group: &NodeGroup,
        theme: &Theme,
        response: &mut MixerViewResponse,
    ) {
        if strips.is_empty() {
            return;
        }

        // Derive master state from member strips.
        let count = strips.len() as f32;
        let avg_volume = strips.iter().map(|s| s.volume).sum::<f32>() / count;
        let peak_meter = strips.iter().map(|s| s.meter).fold(0.0_f32, f32::max);
        let all_muted = strips.iter().all(|s| s.muted);
        let any_muted = strips.iter().any(|s| s.muted);

        let group_color = group.color.to_color32();
        let card_fill = egui::Color32::from_rgb(16, 20, 30);
        let card_stroke = if all_muted {
            egui::Color32::from_rgb(130, 60, 60)
        } else {
            group_color
        };

        egui::Frame::NONE
            .fill(card_fill)
            .stroke(egui::Stroke::new(2.0, card_stroke))
            .corner_radius(18)
            .inner_margin(egui::Margin::symmetric(20, 16))
            .show(ui, |ui| {
                ui.set_width(190.0);
                ui.vertical_centered(|ui| {
                    // Group color accent bar
                    let (bar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(160.0, 4.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(bar_rect, 2.0, group_color);
                    ui.add_space(6.0);

                    ui.label(
                        egui::RichText::new("MASTER")
                            .strong()
                            .size(13.0)
                            .color(group_color),
                    );
                    ui.label(
                        egui::RichText::new(&group.name)
                            .strong()
                            .size(17.0)
                            .color(theme.text.primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("{} ch", strips.len()))
                            .small()
                            .color(theme.text.muted),
                    );
                    ui.add_space(12.0);

                    ui.horizontal_top(|ui| {
                        self.draw_db_scale(ui, theme);
                        ui.add_space(8.0);

                        // Master volume slider — adjusts all members proportionally.
                        let mut slider_value = avg_volume;
                        let slider_size = egui::vec2(46.0, 272.0);
                        let mut style = ui.style().as_ref().clone();
                        style.spacing.slider_width = 240.0;
                        style.visuals.widgets.active.bg_fill = group_color;
                        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(
                            group.color.r.saturating_add(30),
                            group.color.g.saturating_add(30),
                            group.color.b.saturating_add(30),
                        );
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
                            if resp.changed() {
                                // Scale all members proportionally so their relative
                                // balance is preserved.
                                let old_avg = avg_volume;
                                for strip in strips {
                                    let new_vol = if old_avg > 0.0001 {
                                        (strip.volume / old_avg * slider_value).clamp(0.0, 2.0)
                                    } else {
                                        slider_value.clamp(0.0, 2.0)
                                    };
                                    response.volume_changes.push((strip.node_id, new_vol));
                                }
                            }
                            if resp.double_clicked() {
                                // Reset all members to unity.
                                for strip in strips {
                                    response.volume_changes.push((strip.node_id, 1.0));
                                }
                            }
                            resp.on_hover_text(
                                "Master fader — scales all member volumes proportionally.\nDouble-click to reset all to unity (0 dB).",
                            );
                        });

                        ui.add_space(10.0);
                        self.draw_level_meter(ui, peak_meter, all_muted, slider_size.y);
                    });

                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", avg_volume * 100.0))
                            .monospace()
                            .size(20.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(Self::format_db(avg_volume))
                            .monospace()
                            .small()
                            .color(theme.text.muted),
                    );

                    ui.add_space(8.0);

                    // Master mute toggles all members.
                    let mute_text = if all_muted {
                        format!("{} All Muted", egui_phosphor::regular::SPEAKER_SLASH)
                    } else if any_muted {
                        format!("{} Partial", egui_phosphor::regular::SPEAKER_LOW)
                    } else {
                        format!("{} Mute All", egui_phosphor::regular::SPEAKER_HIGH)
                    };
                    let mute_fill = if all_muted {
                        egui::Color32::from_rgb(130, 46, 46)
                    } else if any_muted {
                        egui::Color32::from_rgb(90, 60, 40)
                    } else {
                        egui::Color32::from_rgb(35, 42, 56)
                    };
                    if ui
                        .add(
                            egui::Button::new(mute_text)
                                .fill(mute_fill)
                                .corner_radius(10)
                                .min_size(egui::vec2(140.0, 34.0)),
                        )
                        .clicked()
                    {
                        // If any member is unmuted, mute all; otherwise unmute all.
                        let should_mute = !all_muted;
                        for strip in strips {
                            if strip.muted != should_mute {
                                response.mute_toggles.push(strip.node_id);
                            }
                        }
                    }
                });
            });
    }

    fn format_db(volume: f32) -> String {
        if volume <= 0.0001 {
            "−∞ dB".to_string()
        } else {
            format!("{:+.1} dB", 20.0 * volume.log10())
        }
    }
}
