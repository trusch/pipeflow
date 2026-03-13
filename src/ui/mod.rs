//! UI layer for Pipeflow.
//!
//! This module contains all egui-based user interface components including:
//! - Graph visualization with node/port/link rendering
//! - Node inspection panel
//! - Command palette
//! - Toolbar and filter controls
//! - Contextual help system

pub mod command_palette;
pub mod filters;
pub mod graph_view;
pub mod groups;
pub mod help;
pub mod help_texts;
pub mod node_panel;
pub mod rules;
pub mod settings;
pub mod sidebar;
pub mod snapshots;
pub mod theme;
pub mod toolbar;
