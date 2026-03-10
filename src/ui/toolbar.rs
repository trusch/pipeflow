//! Toolbar with quick actions and status.
//!
//! Shows safety indicators, connection status, and quick action buttons.

use crate::core::config::MeterConfig;
use crate::core::state::{ConnectionState, LayerVisibility};
use crate::domain::graph::NodeLayer;
use crate::domain::safety::{SafetyController, SafetyMode};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use egui::Ui;

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
            Self::show_quick_actions(ui, hide_uninteresting, &mut response);

            ui.separator();

            // Layer visibility controls
            Self::show_layer_controls(ui, layer_visibility, &mut response);

            ui.separator();

            // Meter controls
            Self::show_meter_controls(ui, meter_config, &mut response);

            // Spacer
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Help button
                if ui.button("?").clicked() {
                    response.show_help = true;
                }

                // Settings button
                if ui.button("Settings").clicked() {
                    response.show_settings = true;
                }
            });
        });

        response
    }

    /// Shows connection status indicator.
    fn show_connection_status(ui: &mut Ui, connection: ConnectionState, theme: &Theme) {
        let (color, text) = match connection {
            ConnectionState::Connected => (theme.meter.low, "[*] Connected"),
            ConnectionState::Connecting => (theme.text.warning, "[~] Connecting..."),
            ConnectionState::Disconnected => (theme.text.muted, "[ ] Disconnected"),
            ConnectionState::Error => (theme.text.error, "[!] Error"),
        };

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
    fn show_quick_actions(ui: &mut Ui, hide_uninteresting: bool, response: &mut ToolbarResponse) {
        if ui.button("Refresh").clicked() {
            response.refresh = true;
        }

        if ui.button("Search").clicked() {
            response.open_search = true;
        }

        ui.separator();

        // Toggle hide uninteresting nodes
        let hide_text = if hide_uninteresting {
            "Show All"
        } else {
            "Hide Boring"
        };
        let hover_text = if hide_uninteresting {
            "Show all nodes including those marked as uninteresting"
        } else {
            "Hide nodes marked as uninteresting"
        };
        if ui.button(hide_text).on_hover_text(hover_text).clicked() {
            response.toggle_hide_uninteresting = true;
        }
    }

    /// Shows meter configuration controls.
    fn show_meter_controls(
        ui: &mut Ui,
        meter_config: &MeterConfig,
        response: &mut ToolbarResponse,
    ) {
        ui.horizontal(|ui| {
            // Meter toggle
            let meter_text = if meter_config.enabled {
                "Meters: On"
            } else {
                "Meters: Off"
            };

            if ui.button(meter_text).clicked() {
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
    ) {
        ui.horizontal(|ui| {
            ui.label("Layers:");

            // Hardware layer toggle
            let hw_text = if layer_visibility.hardware { "[HW]" } else { " HW " };
            if ui
                .button(hw_text)
                .on_hover_text(format!(
                    "{}: {}",
                    NodeLayer::Hardware.display_name(),
                    NodeLayer::Hardware.description()
                ))
                .clicked()
            {
                response.toggle_layer = Some(NodeLayer::Hardware);
            }

            // PipeWire layer toggle
            let pw_text = if layer_visibility.pipewire { "[PW]" } else { " PW " };
            if ui
                .button(pw_text)
                .on_hover_text(format!(
                    "{}: {}",
                    NodeLayer::Pipewire.display_name(),
                    NodeLayer::Pipewire.description()
                ))
                .clicked()
            {
                response.toggle_layer = Some(NodeLayer::Pipewire);
            }

            // Session layer toggle
            let sm_text = if layer_visibility.session { "[SM]" } else { " SM " };
            if ui
                .button(sm_text)
                .on_hover_text(format!(
                    "{}: {}",
                    NodeLayer::Session.display_name(),
                    NodeLayer::Session.description()
                ))
                .clicked()
            {
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
