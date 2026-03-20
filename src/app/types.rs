//! Shared app-local types and UI component wiring.

use crate::core::commands::CommandRegistry;
use crate::core::config::Config;
use crate::core::history::UndoStack;
use crate::domain::groups::GroupId;
use crate::domain::snapshots::SnapshotManager;
use crate::ui::command_palette::CommandPalette;
use crate::ui::graph_view::GraphView;
use crate::ui::groups::GroupPanel;
use crate::ui::mixer::MixerView;
use crate::ui::rules::RulesPanel;
use crate::ui::sidebar::SidebarState;
use crate::ui::snapshots::SnapshotPanel;
use crate::ui::theme::Theme;
use crate::util::id::NodeId;

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
pub(crate) enum CenterViewMode {
    Graph,
    GroupMixer(GroupId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceSection {
    Patch,
    AutoConnect,
    SavedSetups,
}

impl WorkspaceSection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Patch => "Patch",
            Self::AutoConnect => "Auto Connect",
            Self::SavedSetups => "Saved Setups",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Patch => egui_phosphor::regular::FLOW_ARROW,
            Self::AutoConnect => egui_phosphor::regular::LINK,
            Self::SavedSetups => egui_phosphor::regular::BOOKMARK_SIMPLE,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct GraphVisibilitySummary {
    pub(super) total_nodes: usize,
    pub(super) visible_nodes: usize,
    pub(super) hidden_by_focus: usize,
    pub(super) hidden_by_layer: usize,
    pub(super) hidden_background: usize,
    pub(super) hidden_internal_meter_nodes: usize,
    pub(super) dimmed_background: usize,
}

impl GraphVisibilitySummary {
    pub(super) fn has_hidden_state(self) -> bool {
        self.hidden_by_focus > 0
            || self.hidden_by_layer > 0
            || self.hidden_background > 0
            || self.hidden_internal_meter_nodes > 0
            || self.dimmed_background > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FeedbackLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub(super) struct PersistentIssue {
    pub(super) key: String,
    pub(super) level: FeedbackLevel,
    pub(super) summary: String,
    pub(super) detail: Option<String>,
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
    /// Mixer view for groups
    pub mixer_view: MixerView,
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
    /// Persistent warnings and errors that should not disappear after a few seconds
    pub(super) persistent_issues: Vec<PersistentIssue>,

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
    /// Current central view mode
    pub center_view: CenterViewMode,
}

impl AppComponents {
    /// Creates new UI components with saved zoom/pan state.
    pub(super) fn new(saved_zoom: f32, saved_pan: egui::Vec2, _config: Config) -> Self {
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
            mixer_view: MixerView::new(),
            snapshot_manager,
            theme: Theme::dark(),
            show_inspector: true,
            show_help: false,
            show_settings: false,
            needs_layout_save: false,
            undo_stack: UndoStack::default(),
            rename_dialog: RenameNodeDialog::default(),
            status_message: None,
            persistent_issues: Vec::new(),
            last_layout_save: std::time::Instant::now(),
            last_viewport_size: (1000.0, 800.0),
            left_sidebar: SidebarState::default(),
            right_sidebar: SidebarState::default(),
            active_workspace: WorkspaceSection::Patch,
            center_view: CenterViewMode::Graph,
        }
    }
}
