//! Central state management.
//!
//! Contains the main application state and state query helpers.

mod animation;
mod connection;
mod graph_state;
mod layer_visibility;
mod serde_helpers;
mod ui_state;

pub use connection::ConnectionState;
pub use graph_state::GraphState;
pub use layer_visibility::LayerVisibility;
pub use ui_state::UiState;

use crate::domain::safety::SafetyController;
use parking_lot::RwLock;
use std::sync::Arc;

#[cfg(test)]
use crate::domain::graph::{Link, Node, Port};
#[cfg(test)]
use crate::util::id::{LinkId, NodeId, NodeIdentifier, PortId};
#[cfg(test)]
use crate::util::spatial::Position;

/// Thread-safe shared state wrapper.
pub type SharedState = Arc<RwLock<AppState>>;

/// Creates a new shared state instance.
pub fn create_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::default()))
}

/// Main application state - single source of truth.
#[derive(Debug, Default)]
pub struct AppState {
    /// Graph data (nodes, ports, links)
    pub graph: GraphState,
    /// UI state (selection, positions, filters)
    pub ui: UiState,
    /// Safety state (mode, locks, panic)
    pub safety: SafetyController,
    /// Connection status
    pub connection: ConnectionState,
}

impl AppState {
    /// Clears all graph state (e.g., on disconnect).
    pub fn clear_graph(&mut self) {
        self.graph.clear();
    }
}

#[cfg(test)]
include!("tests.inc");
