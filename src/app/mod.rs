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
mod initialization;
mod ui_panels;

use crate::core::commands::{AppCommand, CommandHandler, CommandRegistry, UiCommand};
use crate::core::config::Config;
use crate::core::state::SharedState;
use crate::pipewire::connection::PwConnection;
use crate::pipewire::meters::MeterCollector;
use crate::ui::command_palette::CommandPalette;
use crate::ui::filters::FilterPanel;
use crate::ui::graph_view::GraphView;
use crate::ui::groups::GroupPanel;
use crate::ui::help::show_help;
use crate::ui::node_panel::NodePanel;
use crate::ui::rules::RulesPanel;
use crate::ui::settings::SettingsPanel;
use crate::ui::sidebar::{SidebarState, COLLAPSED_WIDTH, MAX_WIDTH, MIN_WIDTH};
use crate::ui::theme::Theme;
use crate::ui::toolbar::Toolbar;
use crate::util::id::{NodeId, NodeIdentifier};
use crate::util::spatial::Position;

/// State for the rename node dialog.
#[derive(Default)]
pub(crate) struct RenameNodeDialog {
    /// Node being renamed (None = dialog closed)
    pub node_id: Option<NodeId>,
    /// Current input text
    pub input: String,
}

impl RenameNodeDialog {
    /// Opens the dialog for a node with the current display name.
    pub fn open(&mut self, node_id: NodeId, current_name: &str) {
        self.node_id = Some(node_id);
        self.input = current_name.to_string();
    }

    /// Closes the dialog.
    pub fn close(&mut self) {
        self.node_id = None;
        self.input.clear();
    }

    /// Returns true if the dialog is open.
    pub fn is_open(&self) -> bool {
        self.node_id.is_some()
    }
}

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

/// UI components and transient state.
///
/// Groups UI-related fields to reduce the size of `PipeflowApp`
/// and make the struct organization clearer.
pub(crate) struct AppComponents {
    // --- UI Components ---
    /// Command registry for palette
    pub command_registry: CommandRegistry,
    /// Command palette UI
    pub command_palette: CommandPalette,
    /// Main graph visualization
    pub graph_view: GraphView,
    /// Groups management panel
    pub group_panel: GroupPanel,
    /// Rules management panel
    pub rules_panel: RulesPanel,
    /// Visual theme settings
    pub theme: Theme,

    // --- Panel visibility flags ---
    /// Show inspector panel
    pub show_inspector: bool,
    /// Show help panel
    pub show_help: bool,
    /// Show settings panel
    pub show_settings: bool,

    // --- State flags ---
    /// Whether state needs layout save
    pub needs_layout_save: bool,

    // --- Dialogs ---
    /// Rename node dialog state
    pub rename_dialog: RenameNodeDialog,

    // --- Timing ---
    /// Last layout save timestamp (for throttling)
    pub last_layout_save: std::time::Instant,

    // --- Layout ---
    /// Last known viewport dimensions
    pub last_viewport_size: (f32, f32),

    // --- Sidebar state ---
    /// Left sidebar state
    pub left_sidebar: SidebarState,
    /// Right sidebar state
    pub right_sidebar: SidebarState,
}

impl AppComponents {
    /// Creates new UI components with saved zoom/pan state.
    fn new(saved_zoom: f32, saved_pan: egui::Vec2, _config: Config) -> Self {
        let mut graph_view = GraphView::new();
        graph_view.zoom = saved_zoom;
        graph_view.pan = saved_pan;

        Self {
            command_registry: CommandRegistry::new(),
            command_palette: CommandPalette::new(),
            graph_view,
            group_panel: GroupPanel::new(),
            rules_panel: RulesPanel::new(),
            theme: Theme::dark(),
            show_inspector: true,
            show_help: false,
            show_settings: false,
            needs_layout_save: false,
            rename_dialog: RenameNodeDialog::default(),
            last_layout_save: std::time::Instant::now(),
            last_viewport_size: (1000.0, 800.0),
            left_sidebar: SidebarState::default(),
            right_sidebar: SidebarState::default(),
        }
    }
}

impl eframe::App for PipeflowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        // Show command palette
        if let Some(action) = self
            .components
            .command_palette
            .show(ctx, &self.components.command_registry)
        {
            self.handle_command_action(action);
        }

        // --- UI Rendering ---
        self.render_floating_windows(ctx);
        self.render_toolbar(ctx);
        self.render_inspector_panel(ctx);
        self.render_left_panel(ctx);
        self.render_graph_view(ctx);

        // --- Persistence ---
        self.handle_layout_save();

        // Request continuous repaint
        ctx.request_repaint();
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
                                tracing::error!("Failed to save config: {}", e);
                            } else {
                                tracing::info!("Configuration saved");
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

                    let response = ui.text_edit_singleline(&mut self.components.rename_dialog.input);

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
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            let state = self.state.read();
            let response = Toolbar::show(
                ui,
                &state.safety,
                state.connection,
                &self.config.meters,
                state.ui.hide_uninteresting,
                &state.ui.layer_visibility,
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

        if response.toggle_hide_uninteresting {
            let mut state = self.state.write();
            state.ui.toggle_hide_uninteresting();
        }

        if let Some(layer) = response.toggle_layer {
            let mut state = self.state.write();
            state.ui.layer_visibility.toggle(layer);
        }
    }

    /// Renders the inspector panel (right side).
    fn render_inspector_panel(&mut self, ctx: &egui::Context) {
        if !self.components.show_inspector {
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
            panel.min_width(MIN_WIDTH).max_width(MAX_WIDTH).resizable(true)
        };

        let response = panel.show(ctx, |ui| {
            // Header with toggle button
            let mut toggle = false;
            ui.horizontal(|ui| {
                if show_collapsed {
                    ui.vertical_centered(|ui| {
                        if ui.button("◀").on_hover_text("Expand (])").clicked() {
                            toggle = true;
                        }
                    });
                } else {
                    if ui.button("▶").on_hover_text("Collapse (])").clicked() {
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
                    ui.weak("Inspector");
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
        self.components.right_sidebar.sync_from_panel(response.response.rect.width());
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
            }

            if let Some(link_id) = response.remove_link {
                {
                    let mut state = self.state.write();
                    state.graph.remove_link(&link_id);
                    state.ui.selected_link = None;
                }
                self.handle_app_command(AppCommand::RemoveLink(link_id));
            }
        }
    }

    /// Renders node inspector details.
    fn render_node_inspector(&mut self, ui: &mut egui::Ui, selected_nodes: &[crate::util::id::NodeId]) {
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
            {
                let mut state = self.state.write();
                state.graph.remove_link(&link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(link_id));
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
        // Animate sidebar
        let dt = ctx.input(|i| i.stable_dt);
        if self.components.left_sidebar.animate(dt) {
            ctx.request_repaint();
        }

        let width = self.components.left_sidebar.current_width;
        let show_collapsed = self.components.left_sidebar.show_collapsed_content();
        let use_exact = self.components.left_sidebar.use_exact_width();

        // Configure panel - use exact_width during animation, otherwise let egui handle resize
        let panel = egui::SidePanel::left("left_panel");
        let panel = if use_exact {
            panel.exact_width(width).resizable(false)
        } else {
            panel.min_width(MIN_WIDTH).max_width(MAX_WIDTH).resizable(true)
        };

        let response = panel.show(ctx, |ui| {
            // Header with toggle button
            let mut toggle = false;
            ui.horizontal(|ui| {
                if show_collapsed {
                    ui.vertical_centered(|ui| {
                        if ui.button("▶").on_hover_text("Expand ([)").clicked() {
                            toggle = true;
                        }
                    });
                } else {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("◀").on_hover_text("Collapse ([)").clicked() {
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

            // Content
            if show_collapsed {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label("🔍");
                    ui.weak("Filter");
                    ui.add_space(8.0);
                    ui.label("📁");
                    ui.weak("Groups");
                    ui.add_space(8.0);
                    ui.label("🔗");
                    ui.weak("Rules");
                });
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(0.0);

                    egui::CollapsingHeader::new("Filters")
                        .default_open(false)
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

                    ui.add_space(4.0);

                    egui::CollapsingHeader::new("Connection Rules")
                        .default_open(true)
                        .show(ui, |ui| {
                            let rules_response = {
                                let mut state = self.state.write();
                                self.components.rules_panel.show(
                                    ui,
                                    &mut state.ui.rules,
                                    &self.components.theme,
                                )
                            };
                            self.handle_rules_panel_response(rules_response);
                        });
                });
            }
        });

        // Sync our state from the actual panel width
        self.components.left_sidebar.sync_from_panel(response.response.rect.width());
    }

    /// Handles group panel responses.
    fn handle_group_panel_response(&mut self, response: crate::ui::groups::GroupPanelResponse) {
        // Handle new group creation - populate persistent_members immediately
        if let Some(group_id) = response.created_group {
            let mut state = self.state.write();
            // First, collect member node_ids from the group
            let member_ids: Vec<_> = state.ui.groups.get_group(&group_id)
                .map(|g| g.members.iter().copied().collect())
                .unwrap_or_default();
            // Then, build identifiers from the graph
            let identifiers: Vec<_> = member_ids.iter()
                .filter_map(|node_id| {
                    state.graph.get_node(node_id).map(|node| {
                        command_handling::create_stable_identifier(node, &state.graph)
                    })
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
    }

    /// Handles rules panel responses.
    fn handle_rules_panel_response(&mut self, response: crate::ui::rules::RulesPanelResponse) {
        if let Some(rule_id) = response.apply_rule {
            self.apply_rule_now(rule_id);
        }
    }

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
                let output_ports: Vec<_> = state.graph.ports.values()
                    .filter(|p| p.direction == PortDirection::Output)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.output_pattern.matches(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        }).unwrap_or(false)
                    })
                    .map(|p| p.id)
                    .collect();

                // Find all input ports matching the input pattern
                let input_ports: Vec<_> = state.graph.ports.values()
                    .filter(|p| p.direction == PortDirection::Input)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.input_pattern.matches(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        }).unwrap_or(false)
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

    /// Renders the central graph view.
    fn render_graph_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            self.components.last_viewport_size = (available.width(), available.height());

            let state = self.state.read();
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
            );
            drop(state);

            self.handle_graph_view_response(ctx, response);
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
        }

        // Remove link
        if let Some(link_id) = response.remove_link {
            {
                let mut state = self.state.write();
                state.graph.remove_link(&link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(link_id));
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
        }

        // Toggle uninteresting
        if let Some(node_ids) = response.toggle_uninteresting {
            let state = self.state.read();
            let nodes_to_toggle = {
                let any_selected = node_ids.iter().any(|id| state.ui.selected_nodes.contains(id));
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
            let current_name = state.ui.resolved_display_name(
                state.graph.get_node(&node_id).unwrap_or_else(|| panic!("Node not found"))
            ).to_string();
            drop(state);
            self.components.rename_dialog.open(node_id, &current_name);
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
    fn save_layout(&self) {
        if let Ok(manager) = crate::core::config::LayoutManager::new() {
            let state = self.state.read();
            if let Err(e) = manager.save(&state.ui) {
                tracing::error!("Failed to save layout: {}", e);
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
                        let identifier = command_handling::create_stable_identifier(node, &state.graph);
                        (identifier, *pos)
                    })
                })
                .collect();

            let uninteresting_updates: Vec<_> = state
                .ui
                .uninteresting_nodes
                .iter()
                .filter_map(|node_id| {
                    state.graph.get_node(node_id).map(|node| {
                        command_handling::create_stable_identifier(node, &state.graph)
                    })
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
