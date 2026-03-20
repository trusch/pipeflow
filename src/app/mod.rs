//! Main application module for Pipeflow.
//!
//! This module contains the `PipeflowApp` struct which implements `eframe::App`
//! and coordinates all application functionality.
//!
//! ## Module Organization
//!
//! - [`initialization`] - Application constructors and setup
//! - [`event_processing`] - PipeWire event handling
//! - [`command_handling`] - UI commands and keyboard shortcuts
//! - [`ui_panels`] - Inspector panel rendering

mod command_handling;
mod event_processing;
mod feedback;
mod initialization;
mod snapshots;
mod types;
mod ui_panels;

pub(crate) use self::types::AppComponents;
use self::types::{CenterViewMode, FeedbackLevel, GraphVisibilitySummary, WorkspaceSection};
use crate::core::commands::{AppCommand, CommandHandler, UiCommand};
use crate::core::config::Config;
use crate::core::config::ThemePreference;
use crate::core::history::{UndoAction, UndoEntry};
use crate::core::state::SharedState;
use crate::pipewire::connection::PwConnection;
use crate::pipewire::meters::MeterCollector;
use crate::ui::filters::FilterPanel;
use crate::ui::help::show_help;
use crate::ui::node_panel::NodePanel;
use crate::ui::settings::SettingsPanel;
use crate::ui::sidebar::{SidebarState, MAX_WIDTH, MIN_WIDTH};
use crate::ui::theme::Theme;
use crate::ui::toolbar::{SessionPresence, Toolbar};
use crate::util::id::NodeIdentifier;
use crate::util::spatial::Position;

/// Main application struct.
///
/// Contains the core application state and connection handlers.
/// UI components are grouped in [`AppComponents`] for better organization.
pub struct PipeflowApp {
    // --- Core state ---
    /// Shared application state (graph, UI state, safety settings)
    state: SharedState,

    // --- Connections ---
    /// PipeWire connection (local mode)
    pw_connection: Option<PwConnection>,
    /// Remote connection (remote mode)
    #[cfg(feature = "network")]
    remote_connection: Option<crate::network::RemoteConnection>,
    /// Command handler for executing PipeWire commands
    command_handler: Option<CommandHandler>,
    /// Whether running in remote mode
    is_remote: bool,
    /// Explicit identity for the current local/remote session.
    session_presence: SessionPresence,

    // --- Audio ---
    /// Meter collector for audio level data
    meter_collector: MeterCollector,

    // --- Configuration ---
    /// Application configuration
    config: Config,

    // --- Initialization state ---
    /// Whether we need to run initial layout (first start)
    needs_initial_layout: bool,

    // --- UI Components (grouped) ---
    /// UI components and transient state
    components: AppComponents,
}

impl eframe::App for PipeflowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Theme Sync ---
        self.sync_theme(ctx);

        // --- Event Processing ---
        self.process_pw_events();
        self.process_pending_rule_connections();
        self.update_animations(ctx);
        self.handle_startup_initialization();

        // --- Meter Updates ---
        self.process_meter_updates();
        self.update_link_meters();

        // --- Input Processing ---
        if self.components.command_palette.handle_shortcuts(ctx) {
            // Command palette was opened
        }
        self.handle_global_shortcuts(ctx);

        // Show command palette (with node search entries)
        let node_entries: Vec<(crate::util::id::NodeId, String)> = {
            let state = self.state.read();
            state
                .graph
                .nodes
                .values()
                .map(|n| (n.id, state.ui.resolved_display_name(n).to_string()))
                .collect()
        };
        if let Some(action) = self.components.command_palette.show(
            ctx,
            &self.components.command_registry,
            &node_entries,
        ) {
            self.handle_command_action(action);
        }

        // --- UI Rendering ---
        self.render_floating_windows(ctx);
        self.render_toolbar(ctx);
        self.render_status_bar(ctx);
        self.render_inspector_panel(ctx);
        self.render_left_panel(ctx);
        self.render_center_panel(ctx);

        // --- Persistence ---
        self.handle_layout_save();

        // Only request continuous repaint when meters are active or animations are running.
        // This avoids 100% CPU usage when the app is idle.
        let has_animations = !self.state.read().ui.position_animations.is_empty();
        let meters_active = self.config.meters.enabled;
        let sidebars_animating = self.components.left_sidebar.is_animating()
            || self.components.right_sidebar.is_animating();

        if meters_active || has_animations || sidebars_animating {
            // Repaint at ~60 Hz when active
            ctx.request_repaint();
        } else {
            // When idle, repaint at ~4 Hz to catch external PipeWire events
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Stop meter collector
        self.meter_collector.stop();

        // Stop PipeWire connection
        if let Some(ref mut pw) = self.pw_connection {
            pw.stop();
        }

        // Save layout if configured
        if self.config.behavior.auto_save_layout {
            self.save_layout_on_exit();
        }
    }
}

// --- Private implementation methods for update() ---
impl PipeflowApp {
    /// Syncs the visual theme from config, resolving System preference.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let use_dark = match self.config.ui.theme {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => {
                // Use egui's system theme detection
                ctx.style().visuals.dark_mode
            }
        };

        // Also update egui's own visuals to match
        let new_theme = if use_dark {
            ctx.set_visuals(egui::Visuals::dark());
            Theme::dark()
        } else {
            ctx.set_visuals(egui::Visuals::light());
            Theme::light()
        };

        // Only replace if the theme preference changed (compare background color as proxy)
        if self.components.theme.background.primary != new_theme.background.primary {
            self.components.theme = new_theme;
        }
    }

    /// Updates position animations and requests repaint if needed.
    fn update_animations(&mut self, ctx: &egui::Context) {
        let mut state = self.state.write();
        let dt = ctx.input(|i| i.stable_dt);
        let has_animations = state.ui.update_animations(dt);
        if has_animations {
            ctx.request_repaint();
        }
    }

    /// Handles startup initialization.
    fn handle_startup_initialization(&mut self) {
        // Mark initial layout as done once we have nodes
        if self.needs_initial_layout {
            let should_mark = {
                let state = self.state.read();
                state.connection.is_connected() && !state.graph.nodes.is_empty()
            };

            if should_mark {
                self.needs_initial_layout = false;
                {
                    let mut state = self.state.write();
                    state.ui.initial_layout_done = true;
                }
            }
        }
    }

    /// Renders floating windows (help, settings).
    fn render_floating_windows(&mut self, ctx: &egui::Context) {
        // Help panel
        if self.components.show_help {
            egui::Window::new("Help")
                .collapsible(true)
                .resizable(true)
                .default_width(400.0)
                .show(ctx, |ui| {
                    show_help(ui);
                });
        }

        // Settings panel
        if self.components.show_settings {
            let mut open = self.components.show_settings;
            egui::Window::new("Settings")
                .open(&mut open)
                .collapsible(true)
                .resizable(true)
                .default_width(350.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let response = SettingsPanel::show(ui, &mut self.config);
                        if response.save_requested {
                            if let Err(e) = self.config.save() {
                                let msg = format!("Failed to save config: {}", e);
                                tracing::error!("{}", msg);
                                self.set_status_message(&msg, true);
                                self.push_persistent_issue(
                                    "settings-save-failed",
                                    FeedbackLevel::Error,
                                    "Could not save settings",
                                    Some(msg.clone()),
                                );
                            } else {
                                tracing::info!("Configuration saved");
                                self.resolve_persistent_issue("settings-save-failed");
                                self.set_status_message("Settings saved", false);
                            }
                        }
                    });
                });
            self.components.show_settings = open;
        }

        // Rename node dialog
        self.render_rename_dialog(ctx);
    }

    /// Renders the rename node dialog.
    fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(node_id) = self.components.rename_dialog.node_id else {
            return;
        };

        let mut should_close = false;
        let mut should_save = false;
        let mut should_reset = false;

        egui::Window::new("Rename Node")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label("Enter a custom display name:");
                    ui.add_space(4.0);

                    let response =
                        ui.text_edit_singleline(&mut self.components.rename_dialog.input);

                    // Focus the text input on first frame
                    if response.gained_focus() || !response.has_focus() {
                        response.request_focus();
                    }

                    // Handle Enter key
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        should_save = true;
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            should_save = true;
                        }
                        if ui.button("Reset to Default").clicked() {
                            should_reset = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
            });

        // Handle Escape key to close
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            should_close = true;
        }

        if should_save {
            let name = self.components.rename_dialog.input.trim();
            if !name.is_empty() {
                self.handle_ui_command(UiCommand::SetCustomName(node_id, Some(name.to_string())));
            }
            self.components.rename_dialog.close();
        } else if should_reset {
            self.handle_ui_command(UiCommand::SetCustomName(node_id, None));
            self.components.rename_dialog.close();
        } else if should_close {
            self.components.rename_dialog.close();
        }
    }

    /// Renders the top toolbar.
    fn render_toolbar(&mut self, ctx: &egui::Context) {
        let safety_fill = {
            let state = self.state.read();
            match state.safety.mode {
                crate::domain::safety::SafetyMode::Normal => {
                    egui::Color32::from_rgba_unmultiplied(80, 160, 120, 16)
                }
                crate::domain::safety::SafetyMode::ReadOnly => {
                    egui::Color32::from_rgba_unmultiplied(220, 180, 90, 18)
                }
                crate::domain::safety::SafetyMode::Stage => {
                    egui::Color32::from_rgba_unmultiplied(255, 140, 80, 20)
                }
            }
        };

        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::NONE
                    .fill(safety_fill)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(ctx, |ui| {
                let state = self.state.read();
                let can_undo = self.components.undo_stack.can_undo();
                let can_redo = self.components.undo_stack.can_redo();
                let response = Toolbar::show(
                    ui,
                    &state.safety,
                    state.connection,
                    &self.session_presence,
                    &self.config.meters,
                    state.ui.hide_uninteresting,
                    &state.ui.layer_visibility,
                    can_undo,
                    can_redo,
                    &self.components.theme,
                );
                drop(state);

                self.handle_toolbar_response(response);
            });
    }

    /// Handles toolbar button responses.
    fn handle_toolbar_response(&mut self, response: crate::ui::toolbar::ToolbarResponse) {
        if let Some(mode) = response.safety_mode {
            let mut state = self.state.write();
            state.safety.set_mode(mode);
        }

        if response.open_search {
            self.components.command_palette.open();
        }

        if response.toggle_meters {
            self.config.meters.enabled = !self.config.meters.enabled;
            self.meter_collector.set_enabled(self.config.meters.enabled);
        }

        if let Some(rate) = response.meter_refresh_rate {
            self.config.meters.refresh_rate = rate;
            self.meter_collector.set_refresh_rate(rate);
        }

        if response.toggle_hide_background {
            let mut state = self.state.write();
            state.ui.toggle_hide_uninteresting();
        }

        if response.show_settings {
            self.components.show_settings = true;
        }

        if response.show_help {
            self.components.show_help = true;
        }

        if let Some(layer) = response.toggle_layer {
            let mut state = self.state.write();
            state.ui.layer_visibility.toggle(layer);
        }

        if response.auto_layout {
            self.perform_auto_layout(false);
        }

        if response.fit_view {
            self.components.graph_view.reset_view();
        }

        if response.undo {
            self.perform_undo();
        }
        if response.redo {
            self.perform_redo();
        }
    }

    /// Renders the inspector panel (right side).
    fn render_inspector_panel(&mut self, ctx: &egui::Context) {
        if !self.components.show_inspector {
            // Show a thin strip so the user can re-open the inspector
            egui::SidePanel::right("inspector_collapsed_strip")
                .exact_width(24.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(egui_phosphor::regular::CARET_LEFT)
                            .on_hover_text("Show Inspector (I)")
                            .clicked()
                        {
                            self.components.show_inspector = true;
                            self.components.right_sidebar.expand();
                        }
                    });
                });
            return;
        }

        // Animate sidebar
        let dt = ctx.input(|i| i.stable_dt);
        if self.components.right_sidebar.animate(dt) {
            ctx.request_repaint();
        }

        let width = self.components.right_sidebar.current_width;
        let show_collapsed = self.components.right_sidebar.show_collapsed_content();
        let use_exact = self.components.right_sidebar.use_exact_width();

        // Configure panel - use exact_width during animation, otherwise let egui handle resize
        let panel = egui::SidePanel::right("inspector");
        let panel = if use_exact {
            panel.exact_width(width).resizable(false)
        } else {
            panel
                .min_width(MIN_WIDTH)
                .max_width(MAX_WIDTH)
                .resizable(true)
        };

        let response = panel.show(ctx, |ui| {
            // Header with toggle button
            let mut toggle = false;
            ui.horizontal(|ui| {
                if show_collapsed {
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(egui_phosphor::regular::CARET_LEFT)
                            .on_hover_text("Expand (])")
                            .clicked()
                        {
                            toggle = true;
                        }
                    });
                } else {
                    if ui
                        .button(egui_phosphor::regular::CARET_RIGHT)
                        .on_hover_text("Collapse (])")
                        .clicked()
                    {
                        toggle = true;
                    }
                    ui.heading("Inspector");
                }
            });
            ui.separator();

            if toggle {
                SidebarState::clear_egui_state(ctx, "inspector");
                self.components.right_sidebar.toggle();
            }

            // Content
            if show_collapsed {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label("ℹ");
                });
            } else {
                let state = self.state.read();
                let selected_nodes: Vec<_> = state.ui.selected_nodes.iter().copied().collect();
                let selected_link = state.ui.selected_link;
                drop(state);

                if selected_nodes.is_empty() {
                    if let Some(link_id) = selected_link {
                        self.render_link_inspector(ui, link_id);
                        return;
                    }
                }
                self.render_node_inspector(ui, &selected_nodes);
            }
        });

        // Sync our state from the actual panel width
        self.components
            .right_sidebar
            .sync_from_panel(response.response.rect.width());
    }

    /// Renders link inspector details.
    fn render_link_inspector(&mut self, ui: &mut egui::Ui, link_id: crate::util::id::LinkId) {
        let state = self.state.read();
        let link_response = if let Some(link) = state.graph.get_link(&link_id) {
            Some(self.show_link_panel(ui, link, &state.graph))
        } else {
            ui.label("Link not found");
            None
        };
        drop(state);

        if let Some(response) = link_response {
            if let Some((link_id, active)) = response.toggle_link {
                {
                    let mut state = self.state.write();
                    if let Some(l) = state.graph.links.get_mut(&link_id) {
                        l.is_active = active;
                    }
                }
                self.handle_app_command(AppCommand::ToggleLink { link_id, active });
                self.components.undo_stack.push(UndoEntry {
                    description: if active {
                        "Enable link"
                    } else {
                        "Disable link"
                    }
                    .to_string(),
                    forward: UndoAction::AppCommand(AppCommand::ToggleLink { link_id, active }),
                    reverse: UndoAction::AppCommand(AppCommand::ToggleLink {
                        link_id,
                        active: !active,
                    }),
                });
            }

            if let Some(link_id) = response.remove_link {
                let port_ids = {
                    let state = self.state.read();
                    state
                        .graph
                        .get_link(&link_id)
                        .map(|l| (l.output_port, l.input_port))
                };
                {
                    let mut state = self.state.write();
                    state.graph.remove_link(&link_id);
                    state.ui.selected_link = None;
                }
                self.handle_app_command(AppCommand::RemoveLink(link_id));
                if let Some((output_port, input_port)) = port_ids {
                    self.components.undo_stack.push(UndoEntry {
                        description: "Remove link".to_string(),
                        forward: UndoAction::AppCommand(AppCommand::RemoveLink(link_id)),
                        reverse: UndoAction::AppCommand(AppCommand::CreateLink {
                            output_port,
                            input_port,
                        }),
                    });
                }
            }
        }
    }

    /// Renders node inspector details.
    fn render_node_inspector(
        &mut self,
        ui: &mut egui::Ui,
        selected_nodes: &[crate::util::id::NodeId],
    ) {
        let state = self.state.read();
        let response = NodePanel::show_multi(
            ui,
            selected_nodes,
            &state.graph,
            &state.ui.uninteresting_nodes,
            &self.components.theme,
        );
        drop(state);

        // Handle mute toggle
        if let Some(node_id) = response.toggle_mute {
            self.handle_mute_toggle(node_id);
        }

        // Handle volume changes
        if let Some((node_id, volume)) = response.volume_changed {
            self.handle_volume_change(node_id, volume);
        }

        // Handle per-channel volume changes
        if let Some((node_id, channel, volume)) = response.channel_volume_changed {
            self.handle_channel_volume_change(node_id, channel, volume);
        }

        // Handle link removal
        if let Some(link_id) = response.remove_link {
            let port_ids = {
                let state = self.state.read();
                state
                    .graph
                    .get_link(&link_id)
                    .map(|l| (l.output_port, l.input_port))
            };
            {
                let mut state = self.state.write();
                state.graph.remove_link(&link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(link_id));
            if let Some((output_port, input_port)) = port_ids {
                self.components.undo_stack.push(UndoEntry {
                    description: "Remove link".to_string(),
                    forward: UndoAction::AppCommand(AppCommand::RemoveLink(link_id)),
                    reverse: UndoAction::AppCommand(AppCommand::CreateLink {
                        output_port,
                        input_port,
                    }),
                });
            }
        }

        // Handle link toggle
        if let Some((link_id, active)) = response.toggle_link {
            {
                let mut state = self.state.write();
                if let Some(link) = state.graph.links.get_mut(&link_id) {
                    link.is_active = active;
                }
            }
            self.handle_app_command(AppCommand::ToggleLink { link_id, active });
            self.components.undo_stack.push(UndoEntry {
                description: if active {
                    "Enable link"
                } else {
                    "Disable link"
                }
                .to_string(),
                forward: UndoAction::AppCommand(AppCommand::ToggleLink { link_id, active }),
                reverse: UndoAction::AppCommand(AppCommand::ToggleLink {
                    link_id,
                    active: !active,
                }),
            });
        }

        // Handle toggle uninteresting
        if let Some(node_ids) = response.toggle_uninteresting {
            self.handle_ui_command(UiCommand::ToggleUninteresting(node_ids));
        }

        // Handle node selection toggle
        if let Some((node_id, extend)) = response.toggle_node_selection {
            let mut state = self.state.write();
            if extend {
                if state.ui.selected_nodes.contains(&node_id) {
                    state.ui.selected_nodes.remove(&node_id);
                } else {
                    state.ui.selected_nodes.insert(node_id);
                }
            } else {
                state.ui.selected_nodes.clear();
                state.ui.selected_nodes.insert(node_id);
            }
            drop(state);

            // Auto-show and expand the inspector when selecting a node
            self.components.show_inspector = true;
            if self.components.right_sidebar.collapsed {
                self.components.right_sidebar.expand();
            }
        }

        if let Some(node_id) = response.rename_node {
            let state = self.state.read();
            if let Some(node) = state.graph.get_node(&node_id) {
                let current_name = state.ui.resolved_display_name(node).to_string();
                drop(state);
                self.components.rename_dialog.open(node_id, &current_name);
            }
        }
    }

    /// Handles mute toggle from inspector.
    fn handle_mute_toggle(&mut self, node_id: crate::util::id::NodeId) {
        let new_muted = {
            let state = self.state.read();
            state.graph.volumes.get(&node_id).map(|vol| !vol.muted)
        };

        if let Some(new_muted) = new_muted {
            {
                let mut state = self.state.write();
                if let Some(vol) = state.graph.volumes.get_mut(&node_id) {
                    vol.muted = new_muted;
                }
            }
            self.handle_app_command(AppCommand::SetMute {
                node_id,
                muted: new_muted,
            });
        }
    }

    /// Handles volume change from inspector.
    fn handle_volume_change(&mut self, node_id: crate::util::id::NodeId, volume: f32) {
        let new_vol = {
            let state = self.state.read();
            state.graph.volumes.get(&node_id).map(|vol| {
                let mut new_vol = vol.clone();
                new_vol.set_all_channels(volume);
                new_vol
            })
        };

        if let Some(new_vol) = new_vol {
            {
                let mut state = self.state.write();
                state.graph.volumes.insert(node_id, new_vol.clone());
            }
            self.handle_app_command(AppCommand::SetVolume {
                node_id,
                volume: new_vol,
            });
        }
    }

    /// Handles per-channel volume change from inspector.
    fn handle_channel_volume_change(
        &mut self,
        node_id: crate::util::id::NodeId,
        channel: usize,
        volume: f32,
    ) {
        {
            let mut state = self.state.write();
            if let Some(vol) = state.graph.volumes.get_mut(&node_id) {
                vol.set_channel(channel, volume);
            }
        }
        self.handle_app_command(AppCommand::SetChannelVolume {
            node_id,
            channel,
            volume,
        });
    }

    /// Renders the left panel (filters, groups, rules).
    fn render_left_panel(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt);
        if self.components.left_sidebar.animate(dt) {
            ctx.request_repaint();
        }

        let width = self.components.left_sidebar.current_width;
        let show_collapsed = self.components.left_sidebar.show_collapsed_content();
        let use_exact = self.components.left_sidebar.use_exact_width();

        let panel = egui::SidePanel::left("left_panel");
        let panel = if use_exact {
            panel.exact_width(width).resizable(false)
        } else {
            panel
                .min_width(MIN_WIDTH)
                .max_width(MAX_WIDTH)
                .resizable(true)
        };

        let response = panel.show(ctx, |ui| {
            let mut toggle = false;
            ui.horizontal(|ui| {
                if show_collapsed {
                    if ui.button(egui_phosphor::regular::CARET_RIGHT).on_hover_text("Expand navigation ([)").clicked() {
                        toggle = true;
                    }
                } else {
                    ui.heading("Navigate");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::CARET_LEFT).on_hover_text("Collapse navigation ([)").clicked() {
                            toggle = true;
                        }
                    });
                }
            });
            ui.separator();

            if toggle {
                SidebarState::clear_egui_state(ctx, "left_panel");
                self.components.left_sidebar.toggle();
            }

            if show_collapsed {
                ui.vertical_centered(|ui| {
                    for section in [WorkspaceSection::Patch, WorkspaceSection::AutoConnect, WorkspaceSection::SavedSetups] {
                        let selected = self.components.active_workspace == section;
                        let button = egui::Button::new(section.icon()).selected(selected);
                        if ui.add(button).on_hover_text(section.label()).clicked() {
                            self.components.active_workspace = section;
                        }
                        ui.add_space(8.0);
                    }
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    for section in [WorkspaceSection::Patch, WorkspaceSection::AutoConnect, WorkspaceSection::SavedSetups] {
                        let selected = self.components.active_workspace == section;
                        if ui.selectable_label(selected, format!("{} {}", section.icon(), section.label())).clicked() {
                            self.components.active_workspace = section;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(0.0);

                    match self.components.active_workspace {
                        WorkspaceSection::Patch => {
                            egui::CollapsingHeader::new("Focus")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let mut state = self.state.write();
                                    FilterPanel::show(ui, &mut state.ui.filters, &self.components.theme);
                                });

                            ui.add_space(4.0);

                            egui::CollapsingHeader::new("Groups")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let group_response = self.show_groups_panel(ui);
                                    self.handle_group_panel_response(group_response);
                                });
                        }
                        WorkspaceSection::AutoConnect => {
                            egui::CollapsingHeader::new("Rules")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let show_empty = {
                                        let state = self.state.read();
                                        state.ui.rules.is_empty()
                                    };
                                    if show_empty {
                                        ui.weak("No rules yet. Create one here or from a node context menu.");
                                        ui.add_space(6.0);
                                    }
                                    let rules_response = {
                                        let mut guard = self.state.write();
                                        let state = &mut *guard;
                                        self.components.rules_panel.show(
                                            ui,
                                            &mut state.ui.rules,
                                            &state.graph,
                                            &self.components.theme,
                                        )
                                    };
                                    self.handle_rules_panel_response(rules_response);
                                });
                        }
                        WorkspaceSection::SavedSetups => {
                            egui::CollapsingHeader::new("Scenes")
                                .default_open(true)
                                .show(ui, |ui| {
                                    if self.components.snapshot_manager.list().is_empty() {
                                        ui.weak("No saved setups yet.");
                                        ui.add_space(6.0);
                                    }
                                    let state = self.state.read();
                                    let snap_response = self.components.snapshot_panel.show(
                                        ui,
                                        &self.components.snapshot_manager,
                                        &state.graph,
                                    );
                                    drop(state);
                                    self.handle_snapshot_panel_response(snap_response);
                                });
                        }
                    }
                });
            }
        });

        self.components
            .left_sidebar
            .sync_from_panel(response.response.rect.width());
    }

    /// Handles group panel responses.
    fn handle_group_panel_response(&mut self, response: crate::ui::groups::GroupPanelResponse) {
        // Handle new group creation - populate persistent_members immediately
        if let Some(group_id) = response.created_group {
            let mut state = self.state.write();
            // First, collect member node_ids from the group
            let member_ids: Vec<_> = state
                .ui
                .groups
                .get_group(&group_id)
                .map(|g| g.members.iter().copied().collect())
                .unwrap_or_default();
            // Then, build identifiers from the graph
            let identifiers: Vec<_> = member_ids
                .iter()
                .filter_map(|node_id| {
                    state
                        .graph
                        .get_node(node_id)
                        .map(|node| command_handling::create_stable_identifier(node, &state.graph))
                })
                .collect();
            // Finally, insert into the group's persistent_members
            if let Some(group) = state.ui.groups.get_group_mut(&group_id) {
                for identifier in identifiers {
                    group.persistent_members.insert(identifier);
                }
            }
            self.components.needs_layout_save = true;
        }

        if let Some(group_id) = response.select_group_members {
            let mut state = self.state.write();
            if let Some(group) = state.ui.groups.get_group(&group_id) {
                state.ui.selected_nodes = group.members.clone();
            }
        }

        if let Some((node_id, extend)) = response.toggle_node_selection {
            let mut state = self.state.write();
            if extend {
                if state.ui.selected_nodes.contains(&node_id) {
                    state.ui.selected_nodes.remove(&node_id);
                } else {
                    state.ui.selected_nodes.insert(node_id);
                }
            } else {
                state.ui.selected_nodes.clear();
                state.ui.selected_nodes.insert(node_id);
            }
        }

        if let Some(group_id) = response.open_mixer {
            self.components.center_view = CenterViewMode::GroupMixer(group_id);
        }
    }

    /// Handles rules panel responses.
    fn handle_rules_panel_response(&mut self, response: crate::ui::rules::RulesPanelResponse) {
        if let Some(rule_id) = response.apply_rule {
            self.apply_rule_now(rule_id);
        }
    }

    /// Handles snapshot panel responses.
    /// Applies a rule immediately (manual trigger).
    fn apply_rule_now(&mut self, rule_id: crate::util::id::RuleId) {
        use crate::domain::graph::PortDirection;

        let connections_to_create: Vec<_> = {
            let state = self.state.read();
            let rule = match state.ui.rules.get_rule(&rule_id) {
                Some(r) => r,
                None => return,
            };

            let mut results = Vec::new();

            for spec in &rule.connections {
                // Find all output ports matching the output pattern
                let output_ports: Vec<_> = state
                    .graph
                    .ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Output)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.output_pattern.matches_runtime(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        })
                        .unwrap_or(false)
                    })
                    .map(|p| p.id)
                    .collect();

                // Find all input ports matching the input pattern
                let input_ports: Vec<_> = state
                    .graph
                    .ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Input)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.input_pattern.matches_runtime(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        })
                        .unwrap_or(false)
                    })
                    .map(|p| p.id)
                    .collect();

                // Create all matching connections
                for output_port_id in &output_ports {
                    for input_port_id in &input_ports {
                        // Check if link already exists
                        let link_exists = state.graph.links.values().any(|l| {
                            l.output_port == *output_port_id && l.input_port == *input_port_id
                        });

                        if !link_exists {
                            results.push((*output_port_id, *input_port_id));
                        }
                    }
                }
            }

            results
        };

        // Create the connections
        for (output_port, input_port) in connections_to_create {
            self.handle_app_command(crate::core::commands::AppCommand::CreateLink {
                output_port,
                input_port,
            });
        }
    }

    /// Renders the central workspace area.
    fn render_center_panel(&mut self, ctx: &egui::Context) {
        match self.components.center_view {
            CenterViewMode::Graph => self.render_graph_view(ctx),
            CenterViewMode::GroupMixer(group_id) => self.render_group_mixer_view(ctx, group_id),
        }
    }

    /// Renders the central graph view.
    fn render_graph_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let summary = self.graph_visibility_summary();
            if summary.has_hidden_state() {
                self.render_graph_visibility_summary(ui, summary);
                ui.add_space(8.0);
            }

            let available = ui.available_rect_before_wrap();
            self.components.last_viewport_size = (available.width(), available.height());

            let state = self.state.read();
            let is_patch_empty = state.graph.nodes.is_empty();
            let has_selection = !state.ui.selected_nodes.is_empty() || state.ui.selected_link.is_some();
            let response = self.components.graph_view.show(
                ui,
                &state.graph,
                &state.ui.node_positions,
                &state.ui.selected_nodes,
                state.ui.selected_link,
                &state.ui.uninteresting_nodes,
                &state.ui.custom_names,
                state.ui.hide_uninteresting,
                &state.ui.layer_visibility,
                &state.ui.filters,
                &state.graph.ports,
                &self.components.theme,
                &state.ui.groups,
                self.config.ui.show_minimap,
            );
            drop(state);

            if is_patch_empty {
                egui::Window::new("patch_empty_teaching")
                    .title_bar(false)
                    .resizable(false)
                    .movable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.set_max_width(360.0);
                        ui.heading("Waiting for audio nodes");
                        ui.label("Open an app, unmute a device, or connect to a remote machine. As nodes appear, drag from a port to patch them together.");
                    });
            } else if !has_selection {
                egui::Area::new("patch_teaching_hint".into())
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 25, 210))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 200, 255, 80)))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                ui.label("Click a node to inspect it, or drag from a port to start a connection.");
                            });
                    });
            }

            self.handle_graph_view_response(ctx, response);
        });
    }

    fn render_group_mixer_view(
        &mut self,
        ctx: &egui::Context,
        group_id: crate::domain::groups::GroupId,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = self.state.read();
            let maybe_group = state.ui.groups.get_group(&group_id).cloned();
            let response = if let Some(group) = maybe_group.as_ref() {
                self.components
                    .mixer_view
                    .show(ui, &state.graph, group, &self.components.theme)
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading("That group no longer exists");
                    ui.label("Return to the patch view and pick another group.");
                });
                crate::ui::mixer::MixerViewResponse {
                    back_to_graph: true,
                    ..Default::default()
                }
            };
            drop(state);

            if response.back_to_graph {
                self.components.center_view = CenterViewMode::Graph;
            }
            for node_id in response.mute_toggles {
                self.handle_mute_toggle(node_id);
            }
            for (node_id, volume) in response.volume_changes {
                self.handle_volume_change(node_id, volume);
            }
        });
    }

    fn graph_visibility_summary(&self) -> GraphVisibilitySummary {
        let state = self.state.read();
        let mut summary = GraphVisibilitySummary {
            total_nodes: state.graph.nodes.len(),
            ..GraphVisibilitySummary::default()
        };

        for node in state.graph.nodes.values() {
            let is_background = state.ui.uninteresting_nodes.contains(&node.id);
            if state.ui.hide_uninteresting && is_background {
                summary.hidden_background += 1;
                continue;
            }
            if !state.ui.layer_visibility.is_visible(node.layer) {
                summary.hidden_by_layer += 1;
                continue;
            }
            if !state.ui.filters.is_empty()
                && !state
                    .ui
                    .filters
                    .matches_with_ports(node, &state.graph.ports)
            {
                summary.hidden_by_focus += 1;
                continue;
            }
            summary.visible_nodes += 1;
            if is_background {
                summary.dimmed_background += 1;
            }
        }

        summary
    }

    fn render_graph_visibility_summary(
        &mut self,
        ui: &mut egui::Ui,
        summary: GraphVisibilitySummary,
    ) {
        egui::Frame::NONE
            .fill(egui::Color32::from_rgba_unmultiplied(110, 140, 180, 20))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(120, 160, 220, 60),
            ))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!(
                        "Showing {} of {} nodes",
                        summary.visible_nodes, summary.total_nodes
                    ));

                    if summary.hidden_by_focus > 0 {
                        ui.label(format!("• {} hidden by focus", summary.hidden_by_focus));
                    }
                    if summary.hidden_by_layer > 0 {
                        ui.label(format!("• {} hidden by layer", summary.hidden_by_layer));
                    }
                    if summary.hidden_background > 0 {
                        ui.label(format!(
                            "• {} background nodes hidden",
                            summary.hidden_background
                        ));
                    }
                    if summary.dimmed_background > 0 {
                        ui.label(format!(
                            "• {} background nodes still visible",
                            summary.dimmed_background
                        ));
                    }

                    if ui.button("Show everything").clicked() {
                        let mut state = self.state.write();
                        state.ui.filters.clear();
                        state.ui.hide_uninteresting = false;
                        state.ui.layer_visibility = Default::default();
                    }

                    if summary.hidden_by_focus > 0 && ui.button("Clear focus").clicked() {
                        let mut state = self.state.write();
                        state.ui.filters.clear();
                    }

                    if summary.hidden_background > 0 && ui.button("Show background").clicked() {
                        let mut state = self.state.write();
                        state.ui.hide_uninteresting = false;
                    }
                });
            });
    }

    /// Handles graph view responses.
    fn handle_graph_view_response(
        &mut self,
        ctx: &egui::Context,
        response: crate::ui::graph_view::GraphViewResponse,
    ) {
        // Node click
        if let Some(node_id) = response.clicked_node {
            {
                let mut state = self.state.write();
                state.ui.selected_link = None;
            }
            let modifiers = ctx.input(|i| i.modifiers);
            if modifiers.shift {
                self.handle_ui_command(UiCommand::AddToSelection(node_id));
            } else if modifiers.command {
                self.handle_ui_command(UiCommand::ToggleSelection(node_id));
            } else {
                self.handle_ui_command(UiCommand::SelectNode(node_id));
            }
        }

        // Link click
        if let Some(link_id) = response.clicked_link {
            let mut state = self.state.write();
            state.ui.selected_link = Some(link_id);
            state.ui.selected_nodes.clear();
        }

        // Box selection
        if !response.box_selected_nodes.is_empty() {
            if !response.box_selection_additive {
                self.handle_ui_command(UiCommand::ClearSelection);
            }
            for node_id in response.box_selected_nodes {
                self.handle_ui_command(UiCommand::AddToSelection(node_id));
            }
        }

        // Background click
        if response.clicked_background {
            self.handle_ui_command(UiCommand::ClearSelection);
            let mut state = self.state.write();
            state.ui.selected_link = None;
        }

        // Node drag
        if let Some((node_id, delta)) = response.dragged_node {
            self.handle_node_drag(node_id, delta);
        }

        // New connection
        if let Some((from, to)) = response.completed_connection {
            self.handle_app_command(AppCommand::CreateLink {
                output_port: from,
                input_port: to,
            });
            self.components.undo_stack.push(UndoEntry {
                description: "Create link".to_string(),
                forward: UndoAction::AppCommand(AppCommand::CreateLink {
                    output_port: from,
                    input_port: to,
                }),
                reverse: UndoAction::RemoveLinkBetweenPorts {
                    output_port: from,
                    input_port: to,
                },
            });
        }

        // Remove link
        if let Some(link_id) = response.remove_link {
            // Capture port IDs before removal for undo
            let port_ids = {
                let state = self.state.read();
                state
                    .graph
                    .get_link(&link_id)
                    .map(|l| (l.output_port, l.input_port))
            };
            {
                let mut state = self.state.write();
                state.graph.remove_link(&link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(link_id));
            if let Some((output_port, input_port)) = port_ids {
                self.components.undo_stack.push(UndoEntry {
                    description: "Remove link".to_string(),
                    forward: UndoAction::AppCommand(AppCommand::RemoveLink(link_id)),
                    reverse: UndoAction::AppCommand(AppCommand::CreateLink {
                        output_port,
                        input_port,
                    }),
                });
            }
        }

        // Toggle link
        if let Some((link_id, active)) = response.toggle_link {
            {
                let mut state = self.state.write();
                if let Some(link) = state.graph.links.get_mut(&link_id) {
                    link.is_active = active;
                }
            }
            self.handle_app_command(AppCommand::ToggleLink { link_id, active });
            self.components.undo_stack.push(UndoEntry {
                description: if active {
                    "Enable link"
                } else {
                    "Disable link"
                }
                .to_string(),
                forward: UndoAction::AppCommand(AppCommand::ToggleLink { link_id, active }),
                reverse: UndoAction::AppCommand(AppCommand::ToggleLink {
                    link_id,
                    active: !active,
                }),
            });
        }

        // Toggle uninteresting
        if let Some(node_ids) = response.toggle_uninteresting {
            let state = self.state.read();
            let nodes_to_toggle = {
                let any_selected = node_ids
                    .iter()
                    .any(|id| state.ui.selected_nodes.contains(id));
                if any_selected && !state.ui.selected_nodes.is_empty() {
                    state.ui.selected_nodes.iter().cloned().collect()
                } else {
                    node_ids
                }
            };
            drop(state);
            self.handle_ui_command(UiCommand::ToggleUninteresting(nodes_to_toggle));
        }

        // Save connections as rule
        if let Some(node_id) = response.save_connections_as_rule {
            self.create_rule_from_node_connections(node_id);
        }

        // Rename node
        if let Some(node_id) = response.rename_node {
            let state = self.state.read();
            if let Some(node) = state.graph.get_node(&node_id) {
                let current_name = state.ui.resolved_display_name(node).to_string();
                drop(state);
                self.components.rename_dialog.open(node_id, &current_name);
            } else {
                tracing::warn!("Cannot rename node {:?}: node no longer exists", node_id);
            }
        }
    }

    /// Handles node drag in graph view.
    fn handle_node_drag(&mut self, node_id: crate::util::id::NodeId, delta: egui::Vec2) {
        let state = self.state.read();
        let mut nodes_to_move = std::collections::HashSet::new();
        nodes_to_move.insert(node_id);

        let is_selected = state.ui.selected_nodes.contains(&node_id);
        if is_selected && state.ui.selected_nodes.len() > 1 {
            for &id in &state.ui.selected_nodes {
                nodes_to_move.insert(id);
            }
        }
        drop(state);

        for id in nodes_to_move {
            let state = self.state.read();
            let current_pos = state.ui.get_node_position(&id);
            drop(state);

            let new_x = current_pos.x + delta.x;
            let new_y = current_pos.y + delta.y;
            self.handle_ui_command(UiCommand::SetNodePosition(id, new_x, new_y));
        }
    }

    /// Handles throttled layout save.
    fn handle_layout_save(&mut self) {
        const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

        if self.components.needs_layout_save
            && self.components.last_layout_save.elapsed() >= SAVE_INTERVAL
        {
            self.save_layout();
            self.components.needs_layout_save = false;
            self.components.last_layout_save = std::time::Instant::now();
        }
    }

    /// Saves the current layout to disk.
    fn save_layout(&mut self) {
        if let Ok(manager) = crate::core::config::LayoutManager::new() {
            let state = self.state.read();
            let save_result = manager.save(&state.ui);
            drop(state);
            if let Err(e) = save_result {
                let msg = format!("Failed to save layout: {}", e);
                tracing::error!("{}", msg);
                self.set_status_message(&msg, true);
                self.push_persistent_issue(
                    "layout-save-failed",
                    FeedbackLevel::Error,
                    "Could not save the current layout",
                    Some(msg.clone()),
                );
            }
        }
    }

    /// Saves layout on exit.
    fn save_layout_on_exit(&mut self) {
        {
            let mut state = self.state.write();

            // Save zoom and pan
            state.ui.zoom = self.components.graph_view.zoom;
            state.ui.pan = Position::new(
                self.components.graph_view.pan.x,
                self.components.graph_view.pan.y,
            );

            // Sync positions using stable identifiers
            let position_updates: Vec<_> = state
                .ui
                .node_positions
                .iter()
                .filter_map(|(node_id, pos)| {
                    state.graph.get_node(node_id).map(|node| {
                        let identifier =
                            command_handling::create_stable_identifier(node, &state.graph);
                        (identifier, *pos)
                    })
                })
                .collect();

            let uninteresting_updates: Vec<_> = state
                .ui
                .uninteresting_nodes
                .iter()
                .filter_map(|node_id| {
                    state
                        .graph
                        .get_node(node_id)
                        .map(|node| command_handling::create_stable_identifier(node, &state.graph))
                })
                .collect();

            let group_updates: Vec<(usize, Vec<NodeIdentifier>)> = state
                .ui
                .groups
                .groups
                .iter()
                .enumerate()
                .map(|(idx, group)| {
                    let member_identifiers: Vec<_> = group
                        .members
                        .iter()
                        .filter_map(|node_id| {
                            state.graph.get_node(node_id).map(|node| {
                                command_handling::create_stable_identifier(node, &state.graph)
                            })
                        })
                        .collect();
                    (idx, member_identifiers)
                })
                .collect();

            for (identifier, pos) in position_updates {
                state.ui.persistent_positions.insert(identifier, pos);
            }

            for identifier in uninteresting_updates {
                state.ui.persistent_uninteresting.insert(identifier);
            }

            for (idx, identifiers) in group_updates {
                if let Some(group) = state.ui.groups.groups.get_mut(idx) {
                    for identifier in identifiers {
                        group.persistent_members.insert(identifier);
                    }
                }
            }

            state.ui.initial_layout_done = true;
        }

        // Save to disk
        if let Ok(manager) = crate::core::config::LayoutManager::new() {
            let state = self.state.read();
            if let Err(e) = manager.save(&state.ui) {
                tracing::error!("Failed to save layout: {}", e);
            } else {
                tracing::info!(
                    "Saved layout with {} positions",
                    state.ui.persistent_positions.len()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::state::create_shared_state;

    #[test]
    fn test_create_shared_state() {
        let state = create_shared_state();
        let read = state.read();
        assert!(read.graph.nodes.is_empty());
    }
}
