//! Collapsible sidebar state and utilities.
//!
//! Provides state management for collapsible sidebars with smooth animations.

use egui::{Context, Id};

/// Width when collapsed (icon-only mode).
pub const COLLAPSED_WIDTH: f32 = 44.0;

/// Default expanded width.
pub const DEFAULT_WIDTH: f32 = 220.0;

/// Minimum expanded width.
pub const MIN_WIDTH: f32 = 150.0;

/// Maximum width.
pub const MAX_WIDTH: f32 = 450.0;

/// Animation speed (width units per second).
const ANIMATION_SPEED: f32 = 2000.0;

/// Persistent state for a collapsible sidebar.
#[derive(Clone, Debug)]
pub struct SidebarState {
    /// Target width (what we're animating towards).
    pub target_width: f32,
    /// Current animated width.
    pub current_width: f32,
    /// Whether collapsed.
    pub collapsed: bool,
    /// Width before collapsing (to restore).
    pub expanded_width: f32,
    /// Whether currently animating.
    pub animating: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            target_width: DEFAULT_WIDTH,
            current_width: DEFAULT_WIDTH,
            collapsed: false,
            expanded_width: DEFAULT_WIDTH,
            animating: false,
        }
    }
}

impl SidebarState {
    /// Toggle between collapsed and expanded.
    pub fn toggle(&mut self) {
        if self.collapsed {
            self.expand();
        } else {
            self.collapse();
        }
    }

    /// Collapse the sidebar.
    pub fn collapse(&mut self) {
        if !self.collapsed {
            self.expanded_width = self.current_width.max(MIN_WIDTH);
            self.collapsed = true;
            self.target_width = COLLAPSED_WIDTH;
            self.animating = true;
        }
    }

    /// Expand the sidebar.
    pub fn expand(&mut self) {
        if self.collapsed {
            self.collapsed = false;
            self.target_width = self.expanded_width;
            self.animating = true;
        }
    }

    /// Update animation. Call this each frame. Returns true if still animating.
    pub fn animate(&mut self, dt: f32) -> bool {
        if !self.animating {
            return false;
        }

        let diff = self.target_width - self.current_width;
        if diff.abs() < 1.0 {
            self.current_width = self.target_width;
            self.animating = false;
            return false;
        }

        let step = diff.signum() * ANIMATION_SPEED * dt;
        if diff.abs() < step.abs() {
            self.current_width = self.target_width;
            self.animating = false;
        } else {
            self.current_width += step;
        }
        true
    }

    /// Returns true if the sidebar is currently animating.
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Returns true if content should be shown in collapsed mode.
    pub fn show_collapsed_content(&self) -> bool {
        self.collapsed || self.current_width < MIN_WIDTH - 20.0
    }

    /// Returns true if we should use exact_width (during animation or when collapsed).
    pub fn use_exact_width(&self) -> bool {
        self.animating || self.collapsed
    }

    /// Sync state from the actual panel width (call after panel.show()).
    pub fn sync_from_panel(&mut self, actual_width: f32) {
        // Only sync when not animating and not collapsed
        if !self.animating && !self.collapsed {
            self.current_width = actual_width;
            self.target_width = actual_width;
            self.expanded_width = actual_width;
        }
    }

    /// Clear egui's stored panel width. Call this when toggling to force our width.
    pub fn clear_egui_state(ctx: &Context, panel_id: &str) {
        // egui stores panel state with various suffixes
        let base_id = Id::new(panel_id);
        ctx.memory_mut(|mem| {
            mem.data.remove::<f32>(base_id.with("__resize"));
            mem.data.remove::<f32>(base_id.with("__panel_resize"));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_state_default() {
        let state = SidebarState::default();
        assert!(!state.collapsed);
        assert_eq!(state.current_width, DEFAULT_WIDTH);
    }

    #[test]
    fn test_sidebar_state_toggle() {
        let mut state = SidebarState::default();

        state.toggle();
        assert!(state.collapsed);
        assert_eq!(state.target_width, COLLAPSED_WIDTH);

        state.toggle();
        assert!(!state.collapsed);
        assert_eq!(state.target_width, DEFAULT_WIDTH);
    }

    #[test]
    fn test_sidebar_state_collapse_preserves_width() {
        let mut state = SidebarState::default();
        state.current_width = 300.0;
        state.target_width = 300.0;

        state.collapse();
        assert!(state.collapsed);
        assert_eq!(state.expanded_width, 300.0);

        state.expand();
        assert!(!state.collapsed);
        assert_eq!(state.target_width, 300.0);
    }

    #[test]
    fn test_sidebar_animation() {
        let mut state = SidebarState::default();
        state.collapse();

        // Should animate towards collapsed width
        assert!(state.animate(0.016)); // ~60fps frame
        assert!(state.current_width < DEFAULT_WIDTH);
    }
}
