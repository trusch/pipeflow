//! Safety mode controller.
//!
//! Provides protection against accidental destructive actions,
//! especially important for live performance scenarios.

use serde::{Deserialize, Serialize};

/// Safety mode for the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SafetyMode {
    /// Normal operation - all actions allowed
    #[default]
    Normal,
    /// Read-only mode - no changes can be made
    ReadOnly,
    /// Stage mode - read-only + routing lock + quick panic access
    Stage,
}

impl SafetyMode {
    /// Returns true if routing changes are allowed.
    pub fn allows_routing(&self) -> bool {
        matches!(self, Self::Normal)
    }

    /// Returns true if volume changes are allowed.
    pub fn allows_volume(&self) -> bool {
        matches!(self, Self::Normal)
    }

    /// Returns the display name for this mode.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::ReadOnly => "Read-Only",
            Self::Stage => "Stage",
        }
    }

    /// Returns a short indicator string for the mode.
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Normal => "",
            Self::ReadOnly => "RO",
            Self::Stage => "STAGE",
        }
    }
}

/// Result of attempting an action under safety constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyCheckResult {
    /// Action is allowed
    Allowed,
    /// Action is blocked with a reason
    Blocked(String),
}

impl SafetyCheckResult {
    /// Returns true if the action is allowed.
    #[cfg(test)]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Controller for safety features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyController {
    /// Current safety mode
    pub mode: SafetyMode,
}

impl Default for SafetyController {
    fn default() -> Self {
        Self {
            mode: SafetyMode::Normal,
        }
    }
}

impl SafetyController {
    /// Sets the safety mode.
    pub fn set_mode(&mut self, mode: SafetyMode) {
        self.mode = mode;
    }

    /// Checks if a link creation is allowed.
    pub fn check_create_link(&self) -> SafetyCheckResult {
        if !self.mode.allows_routing() {
            return SafetyCheckResult::Blocked(format!(
                "Routing changes blocked in {} mode",
                self.mode.display_name()
            ));
        }

        SafetyCheckResult::Allowed
    }

    /// Checks if a link removal is allowed.
    pub fn check_remove_link(&self) -> SafetyCheckResult {
        // Same rules as create
        self.check_create_link()
    }

    /// Checks if a volume change is allowed.
    pub fn check_volume_change(&self) -> SafetyCheckResult {
        if !self.mode.allows_volume() {
            return SafetyCheckResult::Blocked(format!(
                "Volume changes blocked in {} mode",
                self.mode.display_name()
            ));
        }

        SafetyCheckResult::Allowed
    }

    /// Checks if a mute toggle is allowed.
    pub fn check_mute_toggle(&self) -> SafetyCheckResult {
        // Mute toggle is allowed even in read-only for safety
        SafetyCheckResult::Allowed
    }

    /// Returns true if the UI should show safety indicators.
    pub fn should_show_indicator(&self) -> bool {
        self.mode != SafetyMode::Normal
    }

    /// Returns a summary of the current safety state.
    pub fn status_summary(&self) -> String {
        if self.mode != SafetyMode::Normal {
            self.mode.indicator().to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safety_mode_permissions() {
        // Normal mode allows all changes
        assert!(SafetyMode::Normal.allows_routing());
        assert!(SafetyMode::Normal.allows_volume());

        // ReadOnly mode blocks all changes
        assert!(!SafetyMode::ReadOnly.allows_routing());
        assert!(!SafetyMode::ReadOnly.allows_volume());

        // Stage mode blocks all changes
        assert!(!SafetyMode::Stage.allows_routing());
        assert!(!SafetyMode::Stage.allows_volume());
    }

    #[test]
    fn test_safety_controller_default() {
        let controller = SafetyController::default();

        assert_eq!(controller.mode, SafetyMode::Normal);
    }

    #[test]
    fn test_safety_controller_routing_check() {
        let mut controller = SafetyController::default();

        // Normal mode allows routing
        assert!(controller.check_create_link().is_allowed());
        assert!(controller.check_remove_link().is_allowed());

        // Read-only mode blocks routing
        controller.set_mode(SafetyMode::ReadOnly);
        assert!(!controller.check_create_link().is_allowed());

        // Back to normal allows routing again
        controller.set_mode(SafetyMode::Normal);
        assert!(controller.check_create_link().is_allowed());
    }

    #[test]
    fn test_stage_mode_blocks_routing() {
        let mut controller = SafetyController::default();

        controller.set_mode(SafetyMode::Stage);
        assert!(!controller.check_create_link().is_allowed());
    }

    #[test]
    fn test_status_summary() {
        let mut controller = SafetyController::default();

        assert_eq!(controller.status_summary(), "");

        controller.set_mode(SafetyMode::ReadOnly);
        assert!(controller.status_summary().contains("RO"));

        controller.set_mode(SafetyMode::Stage);
        assert!(controller.status_summary().contains("STAGE"));
    }
}
