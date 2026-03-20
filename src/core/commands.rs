//! Command pattern infrastructure.
//!
//! Provides a unified way to execute actions that can be validated
//! against safety constraints.

use crate::domain::audio::VolumeControl;
use crate::domain::safety::{SafetyCheckResult, SafetyController};
use crate::util::id::{LinkId, NodeId, PortId};
use thiserror::Error;

/// Errors that can occur when executing commands.
#[derive(Debug, Error)]
pub enum CommandError {
    /// Command was blocked by the safety controller.
    #[error("Command blocked by safety mode: {0}")]
    SafetyBlocked(String),

    /// The command was invalid or could not be sent.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Result type for command execution.
pub type CommandResult<T> = Result<T, CommandError>;

/// Commands that can be sent to the PipeWire thread.
#[derive(Debug, Clone)]
pub enum AppCommand {
    // Link management
    /// Create a link between two ports
    CreateLink {
        /// Source port ID.
        output_port: PortId,
        /// Destination port ID.
        input_port: PortId,
    },
    /// Remove an existing link
    RemoveLink(LinkId),
    /// Toggle link state (enable/disable)
    ToggleLink {
        /// Link to toggle.
        link_id: LinkId,
        /// New active state.
        active: bool,
    },

    // Audio control
    /// Set volume for a node
    SetVolume {
        /// Target node.
        node_id: NodeId,
        /// New volume settings.
        volume: VolumeControl,
    },
    /// Set mute state for a node
    SetMute {
        /// Target node.
        node_id: NodeId,
        /// Whether to mute.
        muted: bool,
    },
    /// Set volume for a specific channel
    SetChannelVolume {
        /// Target node.
        node_id: NodeId,
        /// Channel index.
        channel: usize,
        /// New volume level.
        volume: f32,
    },

    // Connection
    /// Disconnect from PipeWire
    Disconnect,

    // Metering
    /// Start metering all audio nodes
    StartAllMeters,
    /// Stop all metering
    StopAllMeters,
}

impl AppCommand {
    /// Validates the command against safety constraints.
    pub fn validate(&self, safety: &SafetyController) -> CommandResult<()> {
        let check = match self {
            Self::CreateLink { .. } => safety.check_create_link(),
            Self::RemoveLink(_) => safety.check_remove_link(),
            Self::ToggleLink { .. } => safety.check_create_link(), // Same rules as create
            Self::SetVolume { .. } | Self::SetChannelVolume { .. } => safety.check_volume_change(),
            Self::SetMute { .. } => safety.check_mute_toggle(),
            // These are always allowed
            Self::Disconnect | Self::StartAllMeters | Self::StopAllMeters => {
                SafetyCheckResult::Allowed
            }
        };

        match check {
            SafetyCheckResult::Allowed => Ok(()),
            SafetyCheckResult::Blocked(reason) => Err(CommandError::SafetyBlocked(reason)),
        }
    }
}

/// Handler for executing commands.
pub struct CommandHandler {
    /// Sender for commands to the PipeWire thread
    command_tx: crossbeam::channel::Sender<AppCommand>,
}

impl CommandHandler {
    /// Creates a new command handler.
    pub fn new(command_tx: crossbeam::channel::Sender<AppCommand>) -> Self {
        Self { command_tx }
    }

    /// Executes a command after validating safety constraints.
    pub fn execute(&self, command: AppCommand, safety: &SafetyController) -> CommandResult<()> {
        // Validate first
        command.validate(safety)?;

        // Send to PipeWire thread
        self.command_tx
            .send(command)
            .map_err(|e| CommandError::InvalidOperation(format!("Failed to send command: {}", e)))
    }

    /// Executes a command without safety validation (for internal use).
    pub fn execute_unchecked(&self, command: AppCommand) -> CommandResult<()> {
        self.command_tx
            .send(command)
            .map_err(|e| CommandError::InvalidOperation(format!("Failed to send command: {}", e)))
    }
}

/// Command for the UI to execute.
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Select a node
    SelectNode(NodeId),
    /// Add node to selection
    AddToSelection(NodeId),
    /// Toggle node selection
    ToggleSelection(NodeId),
    /// Clear selection
    ClearSelection,
    /// Set node position
    SetNodePosition(NodeId, f32, f32),
    /// Create a group from selected nodes
    CreateGroupFromSelection(Option<String>),
    /// Set safety mode
    SetSafetyMode(crate::domain::safety::SafetyMode),
    /// Toggle uninteresting status for nodes
    ToggleUninteresting(Vec<NodeId>),
    /// Set custom display name for a node (None clears it)
    SetCustomName(NodeId, Option<String>),
}

/// Registry of available commands for the command palette.
#[derive(Debug)]
pub struct CommandRegistry {
    /// Registered commands
    commands: Vec<CommandEntry>,
}

/// An entry in the command registry.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    /// Command name
    pub name: String,
    /// Description
    pub description: String,
    /// Keyboard shortcut (if any)
    pub shortcut: Option<String>,
    /// Action to execute
    pub action: CommandAction,
}

/// Action associated with a command.
#[derive(Debug, Clone)]
pub enum CommandAction {
    /// Execute a UI command
    Ui(UiCommand),
    /// Custom action (identified by string)
    Custom(String),
    /// Jump to a specific node (pan + select)
    GoToNode(NodeId),
}

impl CommandRegistry {
    /// Creates a new command registry with default commands.
    pub fn new() -> Self {
        let mut registry = Self {
            commands: Vec::new(),
        };
        registry.register_defaults();
        registry
    }

    /// Registers default commands.
    fn register_defaults(&mut self) {
        // Safety commands
        self.register(CommandEntry {
            name: "Switch to Read-Only".to_string(),
            description: "Lock editing until you switch back to Normal".to_string(),
            shortcut: None,
            action: CommandAction::Ui(UiCommand::SetSafetyMode(
                crate::domain::safety::SafetyMode::ReadOnly,
            )),
        });

        self.register(CommandEntry {
            name: "Switch to Normal".to_string(),
            description: "Unlock routing and volume changes".to_string(),
            shortcut: None,
            action: CommandAction::Ui(UiCommand::SetSafetyMode(
                crate::domain::safety::SafetyMode::Normal,
            )),
        });

        self.register(CommandEntry {
            name: "Switch to Stage".to_string(),
            description: "Lock the patch for live use while keeping emergency mute".to_string(),
            shortcut: None,
            action: CommandAction::Ui(UiCommand::SetSafetyMode(
                crate::domain::safety::SafetyMode::Stage,
            )),
        });

        // Selection commands
        self.register(CommandEntry {
            name: "Clear Selection".to_string(),
            description: "Deselect all nodes".to_string(),
            shortcut: Some("Escape".to_string()),
            action: CommandAction::Ui(UiCommand::ClearSelection),
        });

        // View commands
        self.register(CommandEntry {
            name: "Zoom In".to_string(),
            description: "Increase zoom level".to_string(),
            shortcut: Some("Ctrl++".to_string()),
            action: CommandAction::Custom("zoom_in".to_string()),
        });

        self.register(CommandEntry {
            name: "Zoom Out".to_string(),
            description: "Decrease zoom level".to_string(),
            shortcut: Some("Ctrl+-".to_string()),
            action: CommandAction::Custom("zoom_out".to_string()),
        });

        self.register(CommandEntry {
            name: "Fit All".to_string(),
            description: "Fit the full patch in view (formerly Reset View)".to_string(),
            shortcut: Some("Ctrl+0".to_string()),
            action: CommandAction::Custom("reset_view".to_string()),
        });

        self.register(CommandEntry {
            name: "Increase Spacing".to_string(),
            description: "Increase grid spacing".to_string(),
            shortcut: None,
            action: CommandAction::Custom("increase_spacing".to_string()),
        });

        self.register(CommandEntry {
            name: "Decrease Spacing".to_string(),
            description: "Decrease grid spacing".to_string(),
            shortcut: None,
            action: CommandAction::Custom("decrease_spacing".to_string()),
        });

        // UI panel toggles
        self.register(CommandEntry {
            name: "Toggle Help".to_string(),
            description: "Show or hide help".to_string(),
            shortcut: Some("H".to_string()),
            action: CommandAction::Custom("toggle_help".to_string()),
        });

        self.register(CommandEntry {
            name: "Toggle Details".to_string(),
            description: "Show or hide the details panel (Inspector)".to_string(),
            shortcut: Some("I".to_string()),
            action: CommandAction::Custom("toggle_inspector".to_string()),
        });

        self.register(CommandEntry {
            name: "Toggle Settings".to_string(),
            description: "Show or hide settings".to_string(),
            shortcut: Some(",".to_string()),
            action: CommandAction::Custom("toggle_settings".to_string()),
        });

        // Sidebar commands
        self.register(CommandEntry {
            name: "Toggle Navigation".to_string(),
            description:
                "Show or hide the left navigation rail (Patch, Auto Connect, Saved Setups)"
                    .to_string(),
            shortcut: Some("[".to_string()),
            action: CommandAction::Custom("toggle_left_sidebar".to_string()),
        });

        self.register(CommandEntry {
            name: "Toggle Details Sidebar".to_string(),
            description: "Show or hide the details sidebar".to_string(),
            shortcut: Some("]".to_string()),
            action: CommandAction::Custom("toggle_right_sidebar".to_string()),
        });

        // Undo/Redo
        self.register(CommandEntry {
            name: "Undo".to_string(),
            description: "Undo the last action".to_string(),
            shortcut: Some("Ctrl+Z".to_string()),
            action: CommandAction::Custom("undo".to_string()),
        });

        self.register(CommandEntry {
            name: "Redo".to_string(),
            description: "Redo the last undone action".to_string(),
            shortcut: Some("Ctrl+Shift+Z".to_string()),
            action: CommandAction::Custom("redo".to_string()),
        });

        // Snapshot commands
        self.register(CommandEntry {
            name: "Save Setup".to_string(),
            description: "Save the current routing as a setup (snapshot)".to_string(),
            shortcut: None,
            action: CommandAction::Custom("save_snapshot".to_string()),
        });

        // Group commands
        self.register(CommandEntry {
            name: "Create Group".to_string(),
            description: "Collect the selected nodes into a named group".to_string(),
            shortcut: Some("Ctrl+G".to_string()),
            action: CommandAction::Ui(UiCommand::CreateGroupFromSelection(None)),
        });

        self.register(CommandEntry {
            name: "Open Group Mixer".to_string(),
            description: "Switch to the mixer view for the selected or nearest group".to_string(),
            shortcut: Some("Ctrl+Shift+M".to_string()),
            action: CommandAction::Custom("open_group_mixer".to_string()),
        });

        self.register(CommandEntry {
            name: "Back to Patch".to_string(),
            description: "Leave the group mixer and return to the patch graph".to_string(),
            shortcut: Some("Escape".to_string()),
            action: CommandAction::Custom("back_to_patch".to_string()),
        });

        // Layout commands
        self.register(CommandEntry {
            name: "Organize Patch".to_string(),
            description: "Rearrange the full patch for clarity (auto-layout)".to_string(),
            shortcut: Some("Ctrl+L".to_string()),
            action: CommandAction::Custom("auto_layout".to_string()),
        });

        self.register(CommandEntry {
            name: "Organize Selection".to_string(),
            description: "Rearrange only the selected nodes".to_string(),
            shortcut: None,
            action: CommandAction::Custom("auto_layout_selected".to_string()),
        });
    }

    /// Registers a command.
    fn register(&mut self, entry: CommandEntry) {
        self.commands.push(entry);
    }

    /// Returns all commands.
    pub fn all(&self) -> &[CommandEntry] {
        &self.commands
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_validation() {
        let safety = SafetyController::default();

        // Normal mode allows all commands
        let cmd = AppCommand::CreateLink {
            output_port: PortId::new(1),
            input_port: PortId::new(2),
        };
        assert!(cmd.validate(&safety).is_ok());

        // Read-only mode blocks link creation
        let mut safety = SafetyController::default();
        safety.set_mode(crate::domain::safety::SafetyMode::ReadOnly);
        assert!(cmd.validate(&safety).is_err());
    }

    #[test]
    fn test_command_registry() {
        let registry = CommandRegistry::new();
        assert!(!registry.all().is_empty());
    }
}
