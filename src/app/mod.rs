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
use crate::core::config::ThemePreference;
use crate::core::history::{UndoAction, UndoEntry, UndoStack};
use crate::core::state::SharedState;
use crate::domain::snapshots::SnapshotManager;
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
use crate::ui::sidebar::{SidebarState, MAX_WIDTH, MIN_WIDTH};
use crate::ui::snapshots::SnapshotPanel;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceSection {
    Patch,
    AutoConnect,
    SavedSetups,
}

impl WorkspaceSection {
    fn label(self) -> &'static str {
        match self {
            Self::Patch => "Patch",
            Self::AutoConnect => "Auto Connect",
            Self::SavedSetups => "Saved Setups",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Patch => egui_phosphor::regular::FLOW_ARROW,
            Self::AutoConnect => egui_phosphor::regular::LINK,
            Self::SavedSetups => egui_phosphor::regular::BOOKMARK_SIMPLE,
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::Patch => {
                "Focus the graph, organize related nodes, and keep the current patch readable."
            }
            Self::AutoConnect => {
                "Capture repeatable routing so the patch reconnects itself when nodes appear."
            }
            Self::SavedSetups => {
                "Save the current setup and restore it later when you need a known-good scene."
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct GraphVisibilitySummary {
    total_nodes: usize,
    visible_nodes: usize,
    hidden_by_focus: usize,
    hidden_by_layer: usize,
    hidden_background: usize,
    dimmed_background: usize,
}

impl GraphVisibilitySummary {
    fn has_hidden_state(self) -> bool {
        self.hidden_by_focus > 0
            || self.hidden_by_layer > 0
            || self.hidden_background > 0
            || self.dimmed_background > 0
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
    /// Snapshot management panel
    pub snapshot_panel: SnapshotPanel,
    /// Snapshot manager (persistence)
    pub snapshot_manager: SnapshotManager,
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

    // --- Undo/Redo ---
    /// Undo/redo history stack
    pub undo_stack: UndoStack,

    // --- Dialogs ---
    /// Rename node dialog state
    pub rename_dialog: RenameNodeDialog,

    // --- Status ---
    /// Transient status message: (message, timestamp, is_error)
    pub status_message: Option<(String, std::time::Instant, bool)>,

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
    /// Current left-side workspace grouping
    pub active_workspace: WorkspaceSection,
}

impl AppComponents {
    /// Creates new UI components with saved zoom/pan state.
    fn new(saved_zoom: f32, saved_pan: egui::Vec2, _config: Config) -> Self {
        let mut graph_view = GraphView::new();
        graph_view.zoom = saved_zoom;
        graph_view.pan = saved_pan;

        let snapshot_manager = Config::data_dir()
            .map(SnapshotManager::new)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to get data dir for snapshots: {}", e);
                SnapshotManager::new(std::path::PathBuf::from("."))
            });

        Self {
            command_registry: CommandRegistry::new(),
            command_palette: CommandPalette::new(),
            graph_view,
            group_panel: GroupPanel::new(),
            rules_panel: RulesPanel::new(),
            snapshot_panel: SnapshotPanel::new(),
            snapshot_manager,
            theme: Theme::dark(),
            show_inspector: true,
            show_help: false,
            show_settings: false,
            needs_layout_save: false,
            undo_stack: UndoStack::default(),
            rename_dialog: RenameNodeDialog::default(),
            status_message: None,
            last_layout_save: std::time::Instant::now(),
            last_viewport_size: (1000.0, 800.0),
            left_sidebar: SidebarState::default(),
            right_sidebar: SidebarState::default(),
            active_workspace: WorkspaceSection::Patch,
        }
    }
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
        self.render_graph_view(ctx);

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
                                self.components.status_message =
                                    Some((msg, std::time::Instant::now(), true));
                            } else {
                                tracing::info!("Configuration saved");
                                self.components.status_message = Some((
                                    "Settings saved".to_string(),
                                    std::time::Instant::now(),
                                    false,
                                ));
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

    /// Renders a transient status bar at the bottom for user notifications.
    /// Messages auto-clear after 5 seconds.
    fn render_status_bar(&mut self, ctx: &egui::Context) {
        const STATUS_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

        // Auto-clear expired messages
        if let Some((_, created, _)) = &self.components.status_message {
            if created.elapsed() >= STATUS_DURATION {
                self.components.status_message = None;
            }
        }

        if let Some((msg, _, is_error)) = &self.components.status_message {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                let color = if *is_error {
                    egui::Color32::from_rgb(255, 100, 100)
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(color, msg);
            });
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
                ui.add_space(6.0);
                ui.label(self.components.active_workspace.summary());
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
                            egui::CollapsingHeader::new("Auto Connect")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.label("Reuse routing patterns instead of wiring the same patch by hand every time.");
                                    ui.add_space(6.0);
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
                            egui::CollapsingHeader::new("Saved Setups")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.label("Capture the current patch, then restore it later when you want to get back to a known-good setup.");
                                    ui.add_space(6.0);
                                    let snap_response = self.components.snapshot_panel.show(
                                        ui,
                                        &self.components.snapshot_manager,
                                    );
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
    }

    /// Handles rules panel responses.
    fn handle_rules_panel_response(&mut self, response: crate::ui::rules::RulesPanelResponse) {
        if let Some(rule_id) = response.apply_rule {
            self.apply_rule_now(rule_id);
        }
    }

    /// Handles snapshot panel responses.
    fn handle_snapshot_panel_response(
        &mut self,
        response: crate::ui::snapshots::SnapshotPanelResponse,
    ) {
        // Capture new snapshot
        if let Some(name) = response.capture_snapshot {
            let state = self.state.read();
            let result = self.components.snapshot_manager.capture(
                name.clone(),
                &state.graph,
                command_handling::create_stable_identifier,
            );
            drop(state);
            match result {
                Ok(_id) => {
                    self.components.status_message = Some((
                        format!("Snapshot '{}' saved", name),
                        std::time::Instant::now(),
                        false,
                    ));
                }
                Err(e) => {
                    let msg = format!("Failed to save snapshot: {}", e);
                    tracing::error!("{}", msg);
                    self.components.status_message = Some((msg, std::time::Instant::now(), true));
                }
            }
        }

        // Delete snapshot
        if let Some(id) = response.delete_snapshot {
            if let Err(e) = self.components.snapshot_manager.delete(id) {
                let msg = format!("Failed to delete snapshot: {}", e);
                tracing::error!("{}", msg);
                self.components.status_message = Some((msg, std::time::Instant::now(), true));
            }
        }

        // Restore snapshot
        if let Some(id) = response.restore_snapshot {
            self.restore_snapshot(id);
        }
    }

    /// Restores a snapshot by diffing current connections and applying changes.
    fn restore_snapshot(&mut self, id: uuid::Uuid) {
        use crate::domain::graph::PortDirection;

        let snapshot = match self.components.snapshot_manager.get(id) {
            Some(s) => s.clone(),
            None => return,
        };

        let state = self.state.read();

        // Build a lookup: NodeIdentifier -> Vec<NodeId> for current graph
        let mut identifier_to_nodes: std::collections::HashMap<
            NodeIdentifier,
            Vec<crate::util::id::NodeId>,
        > = std::collections::HashMap::new();
        for node in state.graph.nodes.values() {
            let ident = command_handling::create_stable_identifier(node, &state.graph);
            identifier_to_nodes.entry(ident).or_default().push(node.id);
        }

        // Resolve snapshot connections to port IDs
        let mut desired_links: std::collections::HashSet<(
            crate::util::id::PortId,
            crate::util::id::PortId,
        )> = std::collections::HashSet::new();
        let mut unresolved = 0usize;

        for conn in &snapshot.connections {
            // Find output port
            let out_port = identifier_to_nodes
                .get(&conn.output_node)
                .and_then(|node_ids| {
                    node_ids.iter().find_map(|nid| {
                        state.graph.ports.values().find(|p| {
                            p.node_id == *nid
                                && p.name == conn.output_port_name
                                && p.direction == PortDirection::Output
                        })
                    })
                });

            // Find input port
            let in_port = identifier_to_nodes
                .get(&conn.input_node)
                .and_then(|node_ids| {
                    node_ids.iter().find_map(|nid| {
                        state.graph.ports.values().find(|p| {
                            p.node_id == *nid
                                && p.name == conn.input_port_name
                                && p.direction == PortDirection::Input
                        })
                    })
                });

            match (out_port, in_port) {
                (Some(op), Some(ip)) => {
                    desired_links.insert((op.id, ip.id));
                }
                _ => {
                    unresolved += 1;
                }
            }
        }

        // Diff: find links to remove (exist now but not in snapshot)
        let mut links_to_remove = Vec::new();
        for link in state.graph.links.values() {
            let key = (link.output_port, link.input_port);
            if !desired_links.contains(&key) {
                links_to_remove.push(link.id);
            }
        }

        // Diff: find links to create (in snapshot but not in current graph)
        let mut links_to_create = Vec::new();
        for &(out_port, in_port) in &desired_links {
            let exists = state
                .graph
                .links
                .values()
                .any(|l| l.output_port == out_port && l.input_port == in_port);
            if !exists {
                links_to_create.push((out_port, in_port));
            }
        }

        // Resolve volume changes
        let mut volume_changes: Vec<(
            crate::util::id::NodeId,
            crate::domain::audio::VolumeControl,
        )> = Vec::new();
        for sv in &snapshot.volumes {
            if let Some(node_ids) = identifier_to_nodes.get(&sv.identifier) {
                for &nid in node_ids {
                    volume_changes.push((nid, sv.volume.clone()));
                }
            }
        }

        drop(state);

        // Apply removals
        for link_id in &links_to_remove {
            {
                let mut state = self.state.write();
                state.graph.remove_link(link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(*link_id));
        }

        // Apply creations
        for (output_port, input_port) in &links_to_create {
            self.handle_app_command(AppCommand::CreateLink {
                output_port: *output_port,
                input_port: *input_port,
            });
        }

        // Apply volume changes
        for (node_id, volume) in &volume_changes {
            {
                let mut state = self.state.write();
                state.graph.volumes.insert(*node_id, volume.clone());
            }
            self.handle_app_command(AppCommand::SetVolume {
                node_id: *node_id,
                volume: volume.clone(),
            });
        }

        let msg = format!(
            "Restored '{}': -{} +{} links{}",
            snapshot.name,
            links_to_remove.len(),
            links_to_create.len(),
            if unresolved > 0 {
                format!(" ({} unresolved)", unresolved)
            } else {
                String::new()
            }
        );
        tracing::info!("{}", msg);
        self.components.status_message = Some((msg, std::time::Instant::now(), false));
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
                let output_ports: Vec<_> = state
                    .graph
                    .ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Output)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.output_pattern.matches(
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
                            spec.input_pattern.matches(
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

            self.handle_graph_view_response(ctx, response);
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
            if let Err(e) = manager.save(&state.ui) {
                let msg = format!("Failed to save layout: {}", e);
                tracing::error!("{}", msg);
                self.components.status_message = Some((msg, std::time::Instant::now(), true));
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
