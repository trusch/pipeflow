//! Configuration management.
//!
//! Handles loading and saving user preferences.

use crate::core::state::UiState;
use crate::domain::safety::SafetyMode;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// UI configuration
    pub ui: UiConfig,
    /// Metering configuration
    pub meters: MeterConfig,
    /// Behavior configuration
    pub behavior: BehaviorConfig,
    /// Keyboard shortcuts
    pub shortcuts: ShortcutConfig,
}

/// UI-related configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Theme (dark/light)
    pub theme: ThemePreference,
    /// Initial zoom level
    pub default_zoom: f32,
    /// Show grid in graph view
    pub show_grid: bool,
    /// Grid spacing in UI pixels
    pub grid_spacing: f32,
    /// Snap nodes to grid
    pub snap_to_grid: bool,
    /// Show mini-map
    pub show_minimap: bool,
    /// Node width in UI pixels
    pub node_width: f32,
    /// Vertical spacing between ports in UI pixels
    pub port_spacing: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            default_zoom: 1.0,
            show_grid: true,
            grid_spacing: 20.0,
            snap_to_grid: false,
            show_minimap: true,
            node_width: 200.0,
            port_spacing: 24.0,
        }
    }
}

/// Theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemePreference {
    /// Follow system theme.
    #[default]
    System,
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
}

/// Meter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MeterConfig {
    /// Enable meters globally
    pub enabled: bool,
    /// Refresh rate in Hz
    pub refresh_rate: u32,
    /// Show peak hold
    pub show_peak_hold: bool,
    /// Peak hold decay time in ms
    pub peak_hold_decay_ms: u32,
    /// Meter scale type
    pub scale: MeterScale,
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_rate: 30,
            show_peak_hold: true,
            peak_hold_decay_ms: 1500,
            scale: MeterScale::Logarithmic,
        }
    }
}

/// Meter scale type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MeterScale {
    /// Logarithmic (dB) scale — better for audio.
    #[default]
    Logarithmic,
    /// Linear scale.
    Linear,
}

/// Behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Start in this safety mode
    pub startup_safety_mode: SafetyMode,
    /// Remember window position
    pub remember_window_position: bool,
    /// Auto-reconnect to PipeWire
    pub auto_reconnect: bool,
    /// Reconnect delay in ms
    pub reconnect_delay_ms: u32,
    /// Confirm before removing links
    pub confirm_link_removal: bool,
    /// Auto-save layout on exit
    pub auto_save_layout: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            startup_safety_mode: SafetyMode::Normal,
            remember_window_position: true,
            auto_reconnect: true,
            reconnect_delay_ms: 1000,
            confirm_link_removal: false,
            auto_save_layout: true,
        }
    }
}

/// Keyboard shortcuts configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutConfig {
    /// Open command palette
    pub command_palette: String,
    /// Panic mute
    pub panic_mute: String,
    /// Select all
    pub select_all: String,
    /// Delete selected
    pub delete_selected: String,
    /// Create group
    pub create_group: String,
    /// Toggle selected mute
    pub toggle_mute: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            command_palette: "Ctrl+K".to_string(),
            panic_mute: "Ctrl+Shift+M".to_string(),
            select_all: "Ctrl+A".to_string(),
            delete_selected: "Delete".to_string(),
            create_group: "Ctrl+G".to_string(),
            toggle_mute: "M".to_string(),
        }
    }
}

impl Config {
    /// Loads configuration from the default location.
    /// Falls back to defaults if the config file is corrupt or unreadable.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let contents = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Failed to read config from {:?}: {}. Using defaults.",
                        path,
                        e
                    );
                    return Ok(Self::default());
                }
            };
            match toml::from_str::<Config>(&contents) {
                Ok(config) => Ok(config),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse config from {:?}: {}. Using defaults.",
                        path,
                        e
                    );
                    Ok(Self::default())
                }
            }
        } else {
            // Return default and save it
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Saves configuration to the default location.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        Ok(())
    }

    /// Returns the path to the config file.
    pub fn config_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "pipeflow", "pipeflow")
            .context("Failed to determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Returns the path to the data directory.
    pub fn data_dir() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "pipeflow", "pipeflow")
            .context("Failed to determine data directory")?;
        Ok(dirs.data_dir().to_path_buf())
    }

    /// Returns the path to the layout file.
    pub fn layout_path() -> Result<PathBuf> {
        let data_dir = Self::data_dir()?;
        Ok(data_dir.join("layout.json"))
    }
}

/// Manager for persisting UI state.
#[derive(Debug)]
pub struct LayoutManager {
    path: PathBuf,
}

impl LayoutManager {
    /// Creates a new layout manager.
    pub fn new() -> Result<Self> {
        let path = Config::layout_path()?;
        Ok(Self { path })
    }

    /// Saves UI state to disk.
    pub fn save(&self, state: &UiState) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create layout directory: {:?}", parent))?;
        }

        let contents = serde_json::to_string_pretty(state)
            .context("Failed to serialize UI state for layout save")?;
        std::fs::write(&self.path, contents)
            .with_context(|| format!("Failed to write layout file: {:?}", self.path))?;
        Ok(())
    }

    /// Loads UI state from disk.
    /// Falls back to defaults if the layout file is corrupt or unreadable.
    pub fn load(&self) -> Result<UiState> {
        if self.path.exists() {
            let contents = match std::fs::read_to_string(&self.path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Failed to read layout file {:?}: {}. Using defaults.",
                        self.path,
                        e
                    );
                    return Ok(UiState::default());
                }
            };
            match serde_json::from_str::<UiState>(&contents) {
                Ok(state) => Ok(state),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse layout file {:?}: {}. Using defaults.",
                        self.path,
                        e
                    );
                    Ok(UiState::default())
                }
            }
        } else {
            Ok(UiState::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();

        assert_eq!(config.ui.default_zoom, 1.0);
        assert!(config.meters.enabled);
        assert_eq!(config.meters.refresh_rate, 30);
        assert!(config.behavior.auto_reconnect);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.ui.default_zoom, restored.ui.default_zoom);
        assert_eq!(config.meters.enabled, restored.meters.enabled);
    }

    #[test]
    fn test_config_partial_deserialization() {
        // Should use defaults for missing fields
        let toml_str = r#"
            [ui]
            default_zoom = 1.5
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.ui.default_zoom, 1.5);
        assert!(config.meters.enabled); // Default value
    }
}
