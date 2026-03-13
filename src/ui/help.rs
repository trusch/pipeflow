//! Help panel UI.
//!
//! Provides a comprehensive help overlay with contextual information,
//! keyboard shortcuts, and feature explanations.

use egui::{RichText, ScrollArea, Ui};

use super::help_texts::{help_db, help_section, show_help_entry, HelpStyle};

/// Help panel component with categorized, searchable help content.
pub struct HelpPanel {
    /// Current search query for filtering help content.
    search: String,
    /// Currently selected category tab.
    selected_category: HelpCategory,
}

/// Help content categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpCategory {
    #[default]
    GettingStarted,
    Shortcuts,
    Features,
    Audio,
}

impl HelpCategory {
    fn label(&self) -> &'static str {
        match self {
            Self::GettingStarted => "Getting Started",
            Self::Shortcuts => "Shortcuts",
            Self::Features => "Features",
            Self::Audio => "Audio",
        }
    }

    fn all() -> &'static [HelpCategory] {
        &[
            HelpCategory::GettingStarted,
            HelpCategory::Shortcuts,
            HelpCategory::Features,
            HelpCategory::Audio,
        ]
    }
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpPanel {
    /// Creates a new help panel.
    pub fn new() -> Self {
        Self {
            search: String::new(),
            selected_category: HelpCategory::default(),
        }
    }

    /// Shows the help panel content.
    pub fn show(&mut self, ui: &mut Ui) {
        let style = HelpStyle::default();

        // Header
        ui.horizontal(|ui| {
            ui.heading("Help");
            ui.add_space(8.0);
            ui.label(RichText::new("Press H to close").small().weak());
        });
        ui.separator();

        // Search bar
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search);
            if ui.small_button("Clear").clicked() {
                self.search.clear();
            }
        });
        ui.add_space(8.0);

        // Category tabs
        ui.horizontal(|ui| {
            for category in HelpCategory::all() {
                let selected = self.selected_category == *category;
                if ui.selectable_label(selected, category.label()).clicked() {
                    self.selected_category = *category;
                }
            }
        });
        ui.separator();

        // Content area with scroll
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.search.is_empty() {
                    self.show_category_content(ui, &style);
                } else {
                    self.show_search_results(ui, &style);
                }
            });
    }

    fn show_category_content(&self, ui: &mut Ui, _style: &HelpStyle) {
        match self.selected_category {
            HelpCategory::GettingStarted => self.show_getting_started(ui),
            HelpCategory::Shortcuts => self.show_shortcuts(ui),
            HelpCategory::Features => self.show_features(ui),
            HelpCategory::Audio => self.show_audio(ui),
        }
    }

    fn show_getting_started(&self, ui: &mut Ui) {
        help_section(
            ui,
            "general",
            "Welcome",
            &["welcome", "what_is_pipewire"],
            true,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "general",
            "Basic Concepts",
            &["nodes_and_ports", "making_connections", "the_graph"],
            true,
        );

        ui.add_space(12.0);

        help_section(ui, "general", "Interface", &["connection_status"], false);
    }

    fn show_shortcuts(&self, ui: &mut Ui) {
        // Quick reference grid
        egui::CollapsingHeader::new(RichText::new("Quick Reference").strong())
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("shortcuts_grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        Self::shortcut_row(ui, "Ctrl+K / Ctrl+P", "Open command palette");
                        Self::shortcut_row(ui, "Space / F9", "Panic mute (emergency!)");
                        Self::shortcut_row(ui, "Escape", "Clear selection");
                        Self::shortcut_row(ui, "Delete / Backspace", "Remove selected link");
                        Self::shortcut_row(ui, "H", "Toggle help panel");
                        Self::shortcut_row(ui, "F", "Toggle filter panel");
                        Self::shortcut_row(ui, "G", "Toggle groups panel");
                        Self::shortcut_row(ui, "I", "Toggle inspector panel");
                        Self::shortcut_row(ui, "S", "Toggle Saved Setups panel");
                        Self::shortcut_row(ui, "+/-", "Zoom in/out");
                        Self::shortcut_row(ui, "Ctrl+0", "Reset zoom / fit all");
                        Self::shortcut_row(ui, "Ctrl+Shift+R", "Smart reorganize layout");
                        Self::shortcut_row(ui, "Ctrl+A", "Select all nodes");
                    });
            });

        ui.add_space(12.0);

        help_section(
            ui,
            "shortcuts",
            "Panel Shortcuts",
            &["panel_shortcuts"],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "shortcuts",
            "Navigation & Selection",
            &["view_shortcuts", "selection_shortcuts"],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "shortcuts",
            "Safety & Commands",
            &["safety_shortcuts", "command_palette"],
            false,
        );
    }

    fn show_features(&self, ui: &mut Ui) {
        help_section(
            ui,
            "safety",
            "Safety Features",
            &["safety_overview", "panic_button"],
            true,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "safety",
            "Safety Modes",
            &[
                "safety_mode_normal",
                "safety_mode_readonly",
                "safety_mode_stage",
            ],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "filters",
            "Filtering",
            &[
                "filters_overview",
                "search_filter",
                "media_type_filters",
                "combining_filters",
            ],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "groups",
            "Groups",
            &["groups_overview", "creating_groups", "moving_groups"],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "snapshots",
            "Snapshots",
            &[
                "snapshots_overview",
                "what_gets_saved",
                "restoring_snapshots",
            ],
            false,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "graph",
            "Graph Layout",
            &["smart_reorganize", "snap_to_grid", "uninteresting_nodes"],
            false,
        );
    }

    fn show_audio(&self, ui: &mut Ui) {
        help_section(
            ui,
            "audio",
            "Audio Basics",
            &["audio_basics", "media_classes"],
            true,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "audio",
            "Volume & Metering",
            &[
                "volume_control",
                "understanding_meters",
                "link_flow_visualization",
            ],
            true,
        );

        ui.add_space(12.0);

        help_section(
            ui,
            "audio",
            "Advanced",
            &["channels_explained", "latency"],
            false,
        );
    }

    fn show_search_results(&self, ui: &mut Ui, _style: &HelpStyle) {
        let query = self.search.to_lowercase();
        let mut found = false;

        // Search through all categories
        for category_name in help_db().categories() {
            if let Some(category) = help_db().get_category(category_name) {
                for (key, entry) in &category.entries {
                    // Skip metadata
                    if key.starts_with('_') {
                        continue;
                    }

                    // Check if entry matches search
                    let matches = entry.title.to_lowercase().contains(&query)
                        || entry.content.to_lowercase().contains(&query)
                        || entry
                            .tip
                            .as_ref()
                            .is_some_and(|t| t.to_lowercase().contains(&query));

                    if matches {
                        found = true;
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("[{}]", category_name)).small().weak());
                        });
                        show_help_entry(ui, entry);
                        ui.separator();
                    }
                }
            }
        }

        if !found {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No results found").weak());
                ui.label(
                    RichText::new("Try different keywords or browse the categories above")
                        .small()
                        .weak(),
                );
            });
        }
    }

    /// Renders a single shortcut row.
    fn shortcut_row(ui: &mut Ui, key: &str, description: &str) {
        ui.label(RichText::new(key).monospace().strong());
        ui.label(description);
        ui.end_row();
    }
}

/// Legacy static show function for backward compatibility.
/// Prefer using HelpPanel::new() for stateful behavior.
pub fn show_help(ui: &mut Ui) {
    let mut panel = HelpPanel::new();
    panel.show(ui);
}
