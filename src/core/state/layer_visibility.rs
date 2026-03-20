//! Visibility settings for PipeWire stack layers.

use crate::domain::graph::NodeLayer;
use serde::{Deserialize, Serialize};

/// Visibility settings for PipeWire stack layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerVisibility {
    /// Show hardware layer nodes (ALSA devices, etc.)
    pub hardware: bool,
    /// Show PipeWire layer nodes (splits, adapters, etc.)
    pub pipewire: bool,
    /// Show session layer nodes (WirePlumber-managed app nodes)
    pub session: bool,
}

impl Default for LayerVisibility {
    fn default() -> Self {
        Self {
            hardware: true,
            pipewire: true,
            session: true,
        }
    }
}

impl LayerVisibility {
    /// Returns true if the given layer is visible.
    pub fn is_visible(&self, layer: NodeLayer) -> bool {
        match layer {
            NodeLayer::Hardware => self.hardware,
            NodeLayer::Pipewire => self.pipewire,
            NodeLayer::Session => self.session,
        }
    }

    /// Toggles visibility for the given layer.
    pub fn toggle(&mut self, layer: NodeLayer) {
        match layer {
            NodeLayer::Hardware => self.hardware = !self.hardware,
            NodeLayer::Pipewire => self.pipewire = !self.pipewire,
            NodeLayer::Session => self.session = !self.session,
        }
    }
}
