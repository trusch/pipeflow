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
                            for strip in strips {
                                self.show_strip(ui, strip, theme, &mut response);
                                ui.add_space(16.0);
                            }
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
        strip: MixerStrip,
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
                ui.set_width(132.0);
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
                    ui.add_space(10.0);

                    self.draw_level_meter(ui, strip.meter, strip.muted);
                    ui.add_space(10.0);

                    let mut slider_value = strip.volume;
                    let mut style = ui.style().as_ref().clone();
                    style.spacing.slider_width = 220.0;
                    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(102, 162, 255);
                    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(124, 180, 255);
                    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(39, 45, 58);
                    style.visuals.widgets.inactive.weak_bg_fill =
                        egui::Color32::from_rgb(28, 33, 44);
                    ui.scope(|ui| {
                        ui.set_style(style);
                        let slider = egui::Slider::new(&mut slider_value, 0.0..=2.0)
                            .vertical()
                            .show_value(false)
                            .step_by(0.01)
                            .trailing_fill(true);
                        let resp = ui.add_sized(egui::vec2(42.0, 240.0), slider);
                        if resp.changed() {
                            response.volume_changes.push((strip.node_id, slider_value));
                        }
                    });

                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", strip.volume * 100.0))
                            .monospace()
                            .size(20.0)
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
                                .min_size(egui::vec2(100.0, 32.0)),
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

    fn draw_level_meter(&self, ui: &mut egui::Ui, level: f32, muted: bool) {
        let desired = egui::vec2(72.0, 10.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter();
        let bg = egui::Color32::from_rgb(32, 38, 48);
        painter.rect_filled(rect, 999.0, bg);

        let clamped = level.clamp(0.0, 1.0);
        let fill_w = rect.width() * clamped;
        if fill_w > 0.0 {
            let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
            let fill = if muted {
                egui::Color32::from_rgb(120, 70, 70)
            } else if level > 1.0 {
                egui::Color32::from_rgb(255, 90, 90)
            } else if level > 0.8 {
                egui::Color32::from_rgb(255, 210, 100)
            } else {
                egui::Color32::from_rgb(88, 218, 152)
            };
            painter.rect_filled(fill_rect, 999.0, fill);
        }
    }
}
