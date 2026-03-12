//! Toolbar with quick actions and status.
//!
//! Shows safety indicators, connection status, and quick action buttons.

use crate::core::config::MeterConfig;
use crate::core::state::{ConnectionState, LayerVisibility};
use crate::domain::graph::NodeLayer;
use crate::domain::safety::{SafetyController, SafetyMode};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use egui::{RichText, Ui};

/// Toolbar component.
pub struct Toolbar;

impl Toolbar {
    /// Shows the toolbar.
    pub fn show(
        ui: &mut Ui,
        safety: &SafetyController,
        connection: ConnectionState,
        meter_config: &MeterConfig,
        hide_uninteresting: bool,
        layer_visibility: &LayerVisibility,
        theme: &Theme,
    ) -> ToolbarResponse {
        let mut response = ToolbarResponse::default();

        ui.horizontal(|ui| {
            // Connection status
            Self::show_connection_status(ui, connection, theme);

            ui.separator();

            // Safety controls
            Self::show_safety_controls(ui, safety, &mut response, theme);

            ui.separator();

            // Quick actions
            Self::show_quick_actions(ui, hide_uninteresting, &mut response, theme);

            ui.separator();

            // Layer visibility controls
            Self::show_layer_controls(ui, layer_visibility, &mut response, theme);

            ui.separator();

            // Meter controls
            Self::show_meter_controls(ui, meter_config, &mut response, theme);

            // Spacer
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Help button
                if ui.button(egui_phosphor::regular::QUESTION).on_hover_text("Help (F1)").clicked() {
                    response.show_help = true;
                }

                // Settings button
                if ui.button(egui_phosphor::regular::GEAR).on_hover_text("Settings").clicked() {
                    response.show_settings = true;
                }
            });
        });

        response
    }

    /// Shows connection status indicator.
    fn show_connection_status(ui: &mut Ui, connection: ConnectionState, theme: &Theme) {
        let (color, icon, text) = match connection {
            ConnectionState::Connected => (theme.meter.low, egui_phosphor::regular::WIFI_HIGH, "Connected"),
            ConnectionState::Connecting => (theme.text.warning, egui_phosphor::regular::SPINNER, "Connecting..."),
            ConnectionState::Disconnected => (theme.text.muted, egui_phosphor::regular::WIFI_SLASH, "Disconnected"),
            ConnectionState::Error => (theme.text.error, egui_phosphor::regular::WARNING, "Error"),
        };

        ui.label(RichText::new(icon).color(color));
        ui.colored_label(color, text);
    }

    /// Shows safety controls.
    fn show_safety_controls(
        ui: &mut Ui,
        safety: &SafetyController,
        response: &mut ToolbarResponse,
        theme: &Theme,
    ) {
        // Safety mode selector with help
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("safety_mode")
                .selected_text(safety.mode.display_name())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut response.safety_mode, Some(SafetyMode::Normal), "Normal");
                    ui.selectable_value(
                        &mut response.safety_mode,
                        Some(SafetyMode::ReadOnly),
                        "Read-Only",
                    );
                    ui.selectable_value(&mut response.safety_mode, Some(SafetyMode::Stage), "Stage");
                });
            help_button(ui, "safety", "safety_overview");
        });

        // Show safety status if active
        if safety.should_show_indicator() {
            ui.separator();
            let status = safety.status_summary();
            if !status.is_empty() {
                ui.colored_label(theme.text.warning, &status);
            }
        }
    }

    /// Shows quick action buttons.
    fn show_quick_actions(ui: &mut Ui, hide_uninteresting: bool, response: &mut ToolbarResponse, theme: &Theme) {
        if ui.button(egui_phosphor::regular::ARROWS_CLOCKWISE).on_hover_text("Refresh (R)").clicked() {
            response.refresh = true;
        }

        if ui.button(egui_phosphor::regular::MAGNIFYING_GLASS).on_hover_text("Search (Ctrl+K)").clicked() {
            response.open_search = true;
        }

        if ui.button(egui_phosphor::regular::GRAPH).on_hover_text("Auto-Layout (Ctrl+L)").clicked() {
            response.auto_layout = true;
        }

        ui.separator();

        // Toggle hide uninteresting nodes
        let (icon, hover_text) = if hide_uninteresting {
            (egui_phosphor::regular::EYE_SLASH, "Show all nodes (currently hiding uninteresting)")
        } else {
            (egui_phosphor::regular::EYE, "Hide uninteresting nodes")
        };
        let icon_color = if hide_uninteresting { theme.text.muted } else { theme.text.primary };
        if ui.button(RichText::new(icon).color(icon_color)).on_hover_text(hover_text).clicked() {
            response.toggle_hide_uninteresting = true;
        }
    }

    /// Shows meter configuration controls.
    fn show_meter_controls(
        ui: &mut Ui,
        meter_config: &MeterConfig,
        response: &mut ToolbarResponse,
        theme: &Theme,
    ) {
        ui.horizontal(|ui| {
            // Meter toggle
            let meter_color = if meter_config.enabled {
                theme.text.accent
            } else {
                theme.text.muted
            };

            if ui.button(RichText::new(egui_phosphor::regular::CHART_BAR).color(meter_color))
                .on_hover_text("Toggle meters")
                .clicked()
            {
                response.toggle_meters = true;
            }

            // Refresh rate selector (shown only when meters are enabled)
            if meter_config.enabled {
                ui.separator();
                let current_rate = meter_config.refresh_rate;
                egui::ComboBox::from_id_salt("meter_rate")
                    .width(60.0)
                    .selected_text(format!("{} Hz", current_rate))
                    .show_ui(ui, |ui| {
                        for rate in [15, 30, 60] {
                            if ui
                                .selectable_value(&mut response.meter_refresh_rate, Some(rate), format!("{} Hz", rate))
                                .clicked()
                            {}
                        }
                    });
            }

            help_button(ui, "audio", "understanding_meters");
        });
    }

    /// Shows layer visibility controls.
    fn show_layer_controls(
        ui: &mut Ui,
        layer_visibility: &LayerVisibility,
        response: &mut ToolbarResponse,
        theme: &Theme,
    ) {
        ui.horizontal(|ui| {
            // Helper to draw a pill-style toggle
            let pill = |ui: &mut Ui, label: &str, active: bool, layer: NodeLayer| -> bool {
                let text = if active {
                    RichText::new(label).strong().color(theme.text.accent)
                } else {
                    RichText::new(label).color(theme.text.muted)
                };
                let resp = ui.selectable_label(active, text)
                    .on_hover_text(format!("{}: {}", layer.display_name(), layer.description()));
                resp.clicked()
            };

            if pill(ui, "HW", layer_visibility.hardware, NodeLayer::Hardware) {
                response.toggle_layer = Some(NodeLayer::Hardware);
            }
            if pill(ui, "PW", layer_visibility.pipewire, NodeLayer::Pipewire) {
                response.toggle_layer = Some(NodeLayer::Pipewire);
            }
            if pill(ui, "SM", layer_visibility.session, NodeLayer::Session) {
                response.toggle_layer = Some(NodeLayer::Session);
            }
        });
    }
}

/// Response from the toolbar.
#[derive(Debug, Default)]
pub struct ToolbarResponse {
    /// New safety mode selected
    pub safety_mode: Option<SafetyMode>,
    /// Refresh requested
    pub refresh: bool,
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
    /// Toggle hiding uninteresting nodes
    pub toggle_hide_uninteresting: bool,
    /// Toggle visibility of a specific layer
    pub toggle_layer: Option<NodeLayer>,
    /// Trigger auto-layout
    pub auto_layout: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_response_default() {
        let response = ToolbarResponse::default();
        assert!(response.safety_mode.is_none());
        assert!(!response.refresh);
    }
}
