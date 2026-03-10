//! Settings panel UI.
//!
//! Provides controls for editing application configuration.

use crate::core::config::{Config, MeterScale, ThemePreference};
use crate::domain::safety::SafetyMode;
use egui::Ui;

/// Settings panel component.
pub struct SettingsPanel;

/// Response from the settings panel.
#[derive(Debug, Default)]
pub struct SettingsPanelResponse {
    /// Config was modified
    pub config_changed: bool,
    /// Request to save config
    pub save_requested: bool,
}

impl SettingsPanel {
    /// Shows the settings panel content.
    pub fn show(ui: &mut Ui, config: &mut Config) -> SettingsPanelResponse {
        let mut response = SettingsPanelResponse::default();

        ui.heading("Settings");
        ui.separator();

        // Meters section
        egui::CollapsingHeader::new("Meters")
            .default_open(true)
            .show(ui, |ui| {
                response.config_changed |= Self::show_meter_settings(ui, config);
            });

        // Behavior section
        egui::CollapsingHeader::new("Behavior")
            .default_open(true)
            .show(ui, |ui| {
                response.config_changed |= Self::show_behavior_settings(ui, config);
            });

        // Display section
        egui::CollapsingHeader::new("Display")
            .default_open(false)
            .show(ui, |ui| {
                response.config_changed |= Self::show_display_settings(ui, config);
            });

        ui.separator();

        // Save button
        ui.horizontal(|ui| {
            if ui.button("Save Settings").clicked() {
                response.save_requested = true;
            }

            if response.config_changed {
                ui.label("(modified)");
            }
        });

        response
    }

    /// Shows meter-related settings.
    fn show_meter_settings(ui: &mut Ui, config: &mut Config) -> bool {
        let mut changed = false;

        // Enabled toggle
        if ui
            .checkbox(&mut config.meters.enabled, "Enable meters")
            .changed()
        {
            changed = true;
        }

        // Refresh rate
        ui.horizontal(|ui| {
            ui.label("Refresh rate (Hz):");
            let mut rate = config.meters.refresh_rate as f32;
            if ui
                // 10-60 Hz: below 10 is too sluggish for meters; above 60 exceeds frame rate
                .add(egui::Slider::new(&mut rate, 10.0..=60.0).integer())
                .changed()
            {
                config.meters.refresh_rate = rate as u32;
                changed = true;
            }
        });

        // Peak hold
        if ui
            .checkbox(&mut config.meters.show_peak_hold, "Show peak hold")
            .changed()
        {
            changed = true;
        }

        // Peak hold decay
        ui.horizontal(|ui| {
            ui.label("Peak hold decay (ms):");
            let mut decay = config.meters.peak_hold_decay_ms as f32;
            if ui
                // 500-5000ms: below 500ms is too fast to read; above 5s is sluggish
                .add(egui::Slider::new(&mut decay, 500.0..=5000.0).integer())
                .changed()
            {
                config.meters.peak_hold_decay_ms = decay as u32;
                changed = true;
            }
        });

        // Meter scale
        ui.horizontal(|ui| {
            ui.label("Scale:");
            egui::ComboBox::from_id_salt("meter_scale")
                .selected_text(match config.meters.scale {
                    MeterScale::Logarithmic => "Logarithmic",
                    MeterScale::Linear => "Linear",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut config.meters.scale,
                            MeterScale::Logarithmic,
                            "Logarithmic",
                        )
                        .clicked()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut config.meters.scale, MeterScale::Linear, "Linear")
                        .clicked()
                    {
                        changed = true;
                    }
                });
        });

        changed
    }

    /// Shows behavior-related settings.
    fn show_behavior_settings(ui: &mut Ui, config: &mut Config) -> bool {
        let mut changed = false;

        // Startup safety mode
        ui.horizontal(|ui| {
            ui.label("Startup safety mode:");
            egui::ComboBox::from_id_salt("safety_mode")
                .selected_text(match config.behavior.startup_safety_mode {
                    SafetyMode::Normal => "Normal",
                    SafetyMode::ReadOnly => "Read-Only",
                    SafetyMode::Stage => "Stage",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut config.behavior.startup_safety_mode,
                            SafetyMode::Normal,
                            "Normal",
                        )
                        .clicked()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config.behavior.startup_safety_mode,
                            SafetyMode::ReadOnly,
                            "Read-Only",
                        )
                        .clicked()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config.behavior.startup_safety_mode,
                            SafetyMode::Stage,
                            "Stage",
                        )
                        .clicked()
                    {
                        changed = true;
                    }
                });
        });

        // Auto-reconnect
        if ui
            .checkbox(&mut config.behavior.auto_reconnect, "Auto-reconnect")
            .on_hover_text("Automatically reconnect to PipeWire if disconnected")
            .changed()
        {
            changed = true;
        }

        // Auto-save layout
        if ui
            .checkbox(&mut config.behavior.auto_save_layout, "Auto-save layout")
            .on_hover_text("Save layout when exiting")
            .changed()
        {
            changed = true;
        }

        // Confirm link removal
        if ui
            .checkbox(
                &mut config.behavior.confirm_link_removal,
                "Confirm link removal",
            )
            .on_hover_text("Ask before removing links")
            .changed()
        {
            changed = true;
        }

        // Remember window position
        if ui
            .checkbox(
                &mut config.behavior.remember_window_position,
                "Remember window position",
            )
            .changed()
        {
            changed = true;
        }

        changed
    }

    /// Shows display-related settings.
    fn show_display_settings(ui: &mut Ui, config: &mut Config) -> bool {
        let mut changed = false;

        // Theme
        ui.horizontal(|ui| {
            ui.label("Theme:");
            egui::ComboBox::from_id_salt("theme")
                .selected_text(match config.ui.theme {
                    ThemePreference::System => "System",
                    ThemePreference::Light => "Light",
                    ThemePreference::Dark => "Dark",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut config.ui.theme, ThemePreference::System, "System")
                        .clicked()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut config.ui.theme, ThemePreference::Light, "Light")
                        .clicked()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut config.ui.theme, ThemePreference::Dark, "Dark")
                        .clicked()
                    {
                        changed = true;
                    }
                });
        });

        // Show grid
        if ui.checkbox(&mut config.ui.show_grid, "Show grid").changed() {
            changed = true;
        }

        // Grid spacing
        ui.horizontal(|ui| {
            ui.label("Grid spacing:");
            if ui
                .add(egui::Slider::new(&mut config.ui.grid_spacing, 10.0..=50.0))
                .changed()
            {
                changed = true;
            }
        });

        // Snap to grid
        if ui
            .checkbox(&mut config.ui.snap_to_grid, "Snap to grid")
            .changed()
        {
            changed = true;
        }

        // Default zoom
        ui.horizontal(|ui| {
            ui.label("Default zoom:");
            if ui
                .add(egui::Slider::new(&mut config.ui.default_zoom, 0.5..=2.0))
                .changed()
            {
                changed = true;
            }
        });

        // Node width
        ui.horizontal(|ui| {
            ui.label("Node width:");
            if ui
                .add(egui::Slider::new(&mut config.ui.node_width, 150.0..=300.0))
                .changed()
            {
                changed = true;
            }
        });

        changed
    }
}
