//! Toolbar with quick actions and status.
//!
//! Shows the primary patch actions, safety state, and secondary view controls.

use crate::core::config::MeterConfig;
use crate::core::state::{ConnectionState, LayerVisibility};
use crate::domain::graph::NodeLayer;
use crate::domain::safety::{SafetyController, SafetyMode};
use crate::ui::theme::Theme;
use egui::{Color32, RichText, Ui};

/// Explicit session identity shown in the app chrome.
#[derive(Debug, Clone, Default)]
pub struct SessionPresence {
    /// Whether the UI is controlling a remote Pipeflow instance.
    pub is_remote: bool,
    /// Human-readable target, e.g. `studio@rack.local`.
    pub target_label: Option<String>,
    /// Transport summary, e.g. `SSH tunnel via 127.0.0.1:50051`.
    pub transport_label: Option<String>,
}

/// Toolbar component.
pub struct Toolbar;

impl Toolbar {
    /// Shows the toolbar.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        ui: &mut Ui,
        safety: &SafetyController,
        connection: ConnectionState,
        session_presence: &SessionPresence,
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
            let show_presence = session_presence
                .target_label
                .as_deref()
                .is_some_and(|l| l != "This machine");
            if show_presence {
                ui.separator();
                Self::show_session_presence(ui, session_presence, connection, theme);
            }
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
        let (color, icon, text, detail) = match connection {
            ConnectionState::Connected => (
                theme.meter.low,
                egui_phosphor::regular::WIFI_HIGH,
                "Connected",
                "Pipeflow is receiving live graph updates.",
            ),
            ConnectionState::Connecting => (
                theme.text.warning,
                egui_phosphor::regular::SPINNER,
                "Connecting",
                "Waiting for the graph stream to become live.",
            ),
            ConnectionState::Disconnected => (
                theme.text.muted,
                egui_phosphor::regular::WIFI_SLASH,
                "Disconnected",
                "No live graph stream right now.",
            ),
            ConnectionState::Error => (
                theme.text.error,
                egui_phosphor::regular::WARNING,
                "Error",
                "Pipeflow hit a transport or server error.",
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
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(icon).color(color));
                    ui.label(RichText::new(text).color(color).strong());
                });
            })
            .response
            .on_hover_text(detail);
    }

    fn show_session_presence(
        ui: &mut Ui,
        session_presence: &SessionPresence,
        connection: ConnectionState,
        _theme: &Theme,
    ) {
        let (accent, icon, detail) = match connection {
            ConnectionState::Connected => (
                Color32::from_rgb(120, 220, 150),
                egui_phosphor::regular::DESKTOP,
                "You are controlling a remote machine right now.",
            ),
            ConnectionState::Connecting => (
                Color32::from_rgb(255, 200, 110),
                egui_phosphor::regular::SPINNER,
                "Reaching the remote machine through the tunnel…",
            ),
            ConnectionState::Disconnected => (
                Color32::from_rgb(180, 180, 190),
                egui_phosphor::regular::DESKTOP,
                "The remote machine is selected, but the live session is offline.",
            ),
            ConnectionState::Error => (
                Color32::from_rgb(255, 130, 130),
                egui_phosphor::regular::WARNING_OCTAGON,
                "The remote session hit an error. Check the tunnel or remote server.",
            ),
        };

        let title = session_presence
            .target_label
            .as_deref()
            .unwrap_or("Remote machine");
        let transport = session_presence
            .transport_label
            .as_deref()
            .unwrap_or("SSH tunnel");

        let chip_label = if session_presence.is_remote {
            format!("Remote: {}", title)
        } else {
            title.to_string()
        };

        let tooltip = format!("{}\nTransport: {}", detail, transport);

        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                22,
            ))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(icon).color(accent));
                    ui.label(RichText::new(chip_label).strong().color(accent));
                });
            })
            .response
            .on_hover_text(tooltip);
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
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(icon).color(tint));
                    ui.label(RichText::new(label).strong().color(tint));

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
            })
            .response
            .on_hover_text(summary);
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
