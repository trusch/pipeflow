//! Toolbar with quick actions and status.
//!
//! Shows the primary patch actions, safety state, and secondary view controls.

use crate::core::config::MeterConfig;
use crate::core::state::{ConnectionState, LayerVisibility};
use crate::domain::graph::NodeLayer;
use crate::domain::safety::{SafetyController, SafetyMode};
use crate::ui::theme::Theme;
use egui::{Color32, RichText, Ui};

/// Toolbar component.
pub struct Toolbar;

impl Toolbar {
    /// Shows the toolbar.
    pub fn show(
        ui: &mut Ui,
        safety: &SafetyController,
        connection: ConnectionState,
        meter_config: &MeterConfig,
        hide_background: bool,
        layer_visibility: &LayerVisibility,
        can_undo: bool,
        can_redo: bool,
        theme: &Theme,
    ) -> ToolbarResponse {
        let mut response = ToolbarResponse::default();

        ui.horizontal_wrapped(|ui| {
            Self::show_connection_status(ui, connection, theme);
            ui.separator();
            Self::show_safety_state(ui, safety, &mut response, theme);
            ui.separator();
            Self::show_primary_actions(ui, can_undo, can_redo, &mut response, theme);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Self::show_view_menu(
                    ui,
                    meter_config,
                    hide_background,
                    layer_visibility,
                    &mut response,
                    theme,
                );

                if ui
                    .button(egui_phosphor::regular::GEAR)
                    .on_hover_text("Settings")
                    .clicked()
                {
                    response.show_settings = true;
                }

                if ui
                    .button(egui_phosphor::regular::QUESTION)
                    .on_hover_text("Help")
                    .clicked()
                {
                    response.show_help = true;
                }

                if ui
                    .button(
                        RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                            .color(theme.text.primary),
                    )
                    .on_hover_text("Command search (Ctrl+K)")
                    .clicked()
                {
                    response.open_search = true;
                }
            });
        });

        response
    }

    fn show_connection_status(ui: &mut Ui, connection: ConnectionState, theme: &Theme) {
        let (color, icon, text) = match connection {
            ConnectionState::Connected => (
                theme.meter.low,
                egui_phosphor::regular::WIFI_HIGH,
                "Connected",
            ),
            ConnectionState::Connecting => (
                theme.text.warning,
                egui_phosphor::regular::SPINNER,
                "Connecting",
            ),
            ConnectionState::Disconnected => (
                theme.text.muted,
                egui_phosphor::regular::WIFI_SLASH,
                "Disconnected",
            ),
            ConnectionState::Error => (
                theme.text.error,
                egui_phosphor::regular::WARNING,
                "Connection error",
            ),
        };

        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                24,
            ))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).color(color));
                    ui.label(RichText::new(text).color(color).strong());
                });
            });
    }

    fn show_safety_state(
        ui: &mut Ui,
        safety: &SafetyController,
        response: &mut ToolbarResponse,
        theme: &Theme,
    ) {
        let (label, icon, tint, summary) = match safety.mode {
            SafetyMode::Normal => (
                "Normal",
                egui_phosphor::regular::SHIELD_CHECK,
                theme.meter.low,
                "All edits are available.",
            ),
            SafetyMode::ReadOnly => (
                "Read-Only",
                egui_phosphor::regular::LOCK,
                theme.text.warning,
                "Edits are blocked. Switch back to Normal to change routing or volume.",
            ),
            SafetyMode::Stage => (
                "Stage",
                egui_phosphor::regular::WARNING_CIRCLE,
                theme.text.warning,
                "Routing and volume are locked. Mute still works for emergencies.",
            ),
        };

        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(
                tint.r(),
                tint.g(),
                tint.b(),
                28,
            ))
            .stroke(egui::Stroke::new(1.0, tint))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).color(tint));
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!("Safety: {}", label))
                                .strong()
                                .color(tint),
                        );
                        ui.label(RichText::new(summary).small().color(theme.text.secondary));
                    });

                    egui::ComboBox::from_id_salt("toolbar_safety_mode")
                        .selected_text(label)
                        .width(96.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut response.safety_mode,
                                Some(SafetyMode::Normal),
                                "Normal",
                            );
                            ui.selectable_value(
                                &mut response.safety_mode,
                                Some(SafetyMode::ReadOnly),
                                "Read-Only",
                            );
                            ui.selectable_value(
                                &mut response.safety_mode,
                                Some(SafetyMode::Stage),
                                "Stage",
                            );
                        });
                });
            });
    }

    fn show_primary_actions(
        ui: &mut Ui,
        can_undo: bool,
        can_redo: bool,
        response: &mut ToolbarResponse,
        theme: &Theme,
    ) {
        if ui
            .button(egui_phosphor::regular::GRAPH)
            .on_hover_text("Organize patch (Ctrl+L)")
            .clicked()
        {
            response.auto_layout = true;
        }

        if ui
            .button(egui_phosphor::regular::CORNERS_OUT)
            .on_hover_text("Fit all (Ctrl+0)")
            .clicked()
        {
            response.fit_view = true;
        }

        let undo_color = if can_undo {
            theme.text.primary
        } else {
            theme.text.muted
        };
        if ui
            .add_enabled(
                can_undo,
                egui::Button::new(
                    RichText::new(egui_phosphor::regular::ARROW_U_UP_LEFT).color(undo_color),
                ),
            )
            .on_hover_text("Undo (Ctrl+Z)")
            .clicked()
        {
            response.undo = true;
        }

        let redo_color = if can_redo {
            theme.text.primary
        } else {
            theme.text.muted
        };
        if ui
            .add_enabled(
                can_redo,
                egui::Button::new(
                    RichText::new(egui_phosphor::regular::ARROW_U_UP_RIGHT).color(redo_color),
                ),
            )
            .on_hover_text("Redo (Ctrl+Shift+Z)")
            .clicked()
        {
            response.redo = true;
        }
    }

    fn show_view_menu(
        ui: &mut Ui,
        meter_config: &MeterConfig,
        hide_background: bool,
        layer_visibility: &LayerVisibility,
        response: &mut ToolbarResponse,
        _theme: &Theme,
    ) {
        ui.menu_button("View", |ui| {
            let background_label = if hide_background {
                "Show background nodes"
            } else {
                "Hide background nodes"
            };
            if ui.button(background_label).clicked() {
                response.toggle_hide_background = true;
                ui.close();
            }

            ui.separator();
            ui.label("Layers");
            Self::layer_toggle(
                ui,
                "Devices",
                layer_visibility.hardware,
                NodeLayer::Hardware,
                response,
            );
            Self::layer_toggle(
                ui,
                "Engine",
                layer_visibility.pipewire,
                NodeLayer::Pipewire,
                response,
            );
            Self::layer_toggle(
                ui,
                "Apps",
                layer_visibility.session,
                NodeLayer::Session,
                response,
            );

            ui.separator();
            let meter_label = if meter_config.enabled {
                "Hide meters"
            } else {
                "Show meters"
            };
            if ui.button(meter_label).clicked() {
                response.toggle_meters = true;
                ui.close();
            }

            if meter_config.enabled {
                ui.separator();
                ui.label("Meter refresh");
                for rate in [15, 30, 60] {
                    if ui
                        .selectable_label(meter_config.refresh_rate == rate, format!("{} Hz", rate))
                        .clicked()
                    {
                        response.meter_refresh_rate = Some(rate);
                        ui.close();
                    }
                }
            }
        });
    }

    fn layer_toggle(
        ui: &mut Ui,
        label: &str,
        active: bool,
        layer: NodeLayer,
        response: &mut ToolbarResponse,
    ) {
        let text = if active {
            format!("Hide {}", label)
        } else {
            format!("Show {}", label)
        };
        if ui.selectable_label(active, text).clicked() {
            response.toggle_layer = Some(layer);
            ui.close();
        }
    }
}

/// Response from the toolbar.
#[derive(Debug, Default)]
pub struct ToolbarResponse {
    /// New safety mode selected
    pub safety_mode: Option<SafetyMode>,
    /// Open search/command palette
    pub open_search: bool,
    /// Show settings
    pub show_settings: bool,
    /// Show help
    pub show_help: bool,
    /// Toggle meters on/off
    pub toggle_meters: bool,
    /// New meter refresh rate
    pub meter_refresh_rate: Option<u32>,
    /// Toggle hiding background nodes
    pub toggle_hide_background: bool,
    /// Toggle visibility of a specific layer
    pub toggle_layer: Option<NodeLayer>,
    /// Trigger patch organize
    pub auto_layout: bool,
    /// Fit the full patch in view
    pub fit_view: bool,
    /// Trigger undo
    pub undo: bool,
    /// Trigger redo
    pub redo: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_response_default() {
        let response = ToolbarResponse::default();
        assert!(response.safety_mode.is_none());
        assert!(!response.auto_layout);
        assert!(!response.fit_view);
    }
}
