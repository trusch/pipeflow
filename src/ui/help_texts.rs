//! Help text management system.
//!
//! Provides infrastructure for loading, storing, and displaying contextual help texts.
//! Help content is organized in JSON files for easy maintenance and future i18n support.

use egui::{Color32, Response, RichText, Ui, Vec2};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A single help entry with title, content, and optional tip.
#[derive(Debug, Clone, Deserialize)]
pub struct HelpEntry {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tip: Option<String>,
}

/// Metadata for a help category file.
/// Fields are populated by serde deserialization from JSON.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HelpMeta {
    category: String,
    description: String,
    version: String,
}

/// A help category containing multiple entries.
#[derive(Debug, Clone, Deserialize)]
pub struct HelpCategory {
    /// Metadata from JSON (required for deserialization but not used at runtime).
    #[serde(rename = "_meta")]
    #[allow(dead_code)]
    meta: HelpMeta,
    #[serde(flatten)]
    pub entries: HashMap<String, HelpEntry>,
}

/// The complete help text database.
#[derive(Debug, Default)]
pub struct HelpDatabase {
    categories: HashMap<String, HelpCategory>,
}

impl HelpDatabase {
    /// Creates a new help database by loading all embedded help files.
    pub fn load() -> Self {
        let mut db = Self::default();

        // Load all help categories from embedded JSON
        db.load_category("general", include_str!("../../help/general.json"));
        db.load_category("safety", include_str!("../../help/safety.json"));
        db.load_category("filters", include_str!("../../help/filters.json"));
        db.load_category("groups", include_str!("../../help/groups.json"));
        db.load_category("snapshots", include_str!("../../help/snapshots.json"));
        db.load_category("graph", include_str!("../../help/graph.json"));
        db.load_category("audio", include_str!("../../help/audio.json"));
        db.load_category("shortcuts", include_str!("../../help/shortcuts.json"));

        db
    }

    fn load_category(&mut self, name: &str, json: &str) {
        match serde_json::from_str::<HelpCategory>(json) {
            Ok(category) => {
                self.categories.insert(name.to_string(), category);
            }
            Err(e) => {
                tracing::error!("Failed to load help category '{}': {}", name, e);
            }
        }
    }

    /// Gets a help entry by category and key.
    pub fn get(&self, category: &str, key: &str) -> Option<&HelpEntry> {
        self.categories
            .get(category)
            .and_then(|cat| cat.entries.get(key))
    }

    /// Gets all entries in a category.
    pub fn get_category(&self, category: &str) -> Option<&HelpCategory> {
        self.categories.get(category)
    }

    /// Returns all category names.
    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.categories.keys().map(|s| s.as_str())
    }
}

/// Global help database singleton.
static HELP_DB: OnceLock<HelpDatabase> = OnceLock::new();

/// Gets the global help database.
pub fn help_db() -> &'static HelpDatabase {
    HELP_DB.get_or_init(HelpDatabase::load)
}

/// Gets a help entry by category and key.
pub fn get_help(category: &str, key: &str) -> Option<&'static HelpEntry> {
    help_db().get(category, key)
}

/// Style configuration for help UI elements.
#[derive(Debug, Clone)]
pub struct HelpStyle {
    pub button_size: f32,
    pub button_color: Color32,
    pub button_hover_color: Color32,
    pub popup_width: f32,
    pub title_color: Color32,
    pub content_color: Color32,
    pub tip_color: Color32,
    pub tip_bg_color: Color32,
}

impl Default for HelpStyle {
    fn default() -> Self {
        Self {
            button_size: 16.0,
            button_color: Color32::from_rgb(120, 120, 140),
            button_hover_color: Color32::from_rgb(100, 180, 255),
            popup_width: 350.0,
            title_color: Color32::from_rgb(220, 220, 230),
            content_color: Color32::from_rgb(180, 180, 190),
            tip_color: Color32::from_rgb(100, 180, 255),
            tip_bg_color: Color32::from_rgba_unmultiplied(100, 180, 255, 20),
        }
    }
}

/// Renders a small help button that shows a popup on hover.
/// Returns the Response for the button.
pub fn help_button(ui: &mut Ui, category: &str, key: &str) -> Response {
    help_button_styled(ui, category, key, &HelpStyle::default())
}

/// Renders a help button with custom styling.
pub fn help_button_styled(
    ui: &mut Ui,
    category: &str,
    key: &str,
    style: &HelpStyle,
) -> Response {
    let button_id = ui.make_persistent_id(format!("help_btn_{}_{}", category, key));

    let (rect, response) = ui.allocate_exact_size(Vec2::splat(style.button_size), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let color = if response.hovered() {
            style.button_hover_color
        } else {
            style.button_color
        };

        // Draw the question mark button
        let painter = ui.painter();
        let center = rect.center();
        let radius = style.button_size / 2.0 - 1.0;

        // Circle background
        painter.circle(
            center,
            radius,
            Color32::TRANSPARENT,
            egui::Stroke::new(1.5, color),
        );

        // Question mark
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(style.button_size * 0.7),
            color,
        );
    }

    // Show popup on hover
    if response.hovered() {
        if let Some(entry) = get_help(category, key) {
            show_help_popup(ui, button_id, entry, style);
        }
    }

    response
}

/// Shows the help popup for an entry.
#[allow(deprecated)]
fn show_help_popup(ui: &mut Ui, id: egui::Id, entry: &HelpEntry, style: &HelpStyle) {
    egui::containers::show_tooltip(ui.ctx(), egui::LayerId::new(egui::Order::Tooltip, id), id, |ui: &mut Ui| {
        ui.set_max_width(style.popup_width);

        // Title
        ui.label(RichText::new(&entry.title).strong().size(14.0).color(style.title_color));
        ui.add_space(6.0);

        // Content - split by newlines for better rendering
        for paragraph in entry.content.split("\n\n") {
            ui.label(RichText::new(paragraph).size(12.0).color(style.content_color));
            ui.add_space(4.0);
        }

        // Tip section
        if let Some(tip) = &entry.tip {
            ui.add_space(4.0);
            egui::Frame::NONE
                .fill(style.tip_bg_color)
                .corner_radius(4)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Tip:").strong().size(11.0).color(style.tip_color));
                        ui.label(RichText::new(tip).size(11.0).color(style.tip_color));
                    });
                });
        }
    });
}

/// Renders a help entry inline (for help panels).
pub fn show_help_entry(ui: &mut Ui, entry: &HelpEntry) {
    show_help_entry_styled(ui, entry, &HelpStyle::default());
}

/// Renders a help entry inline with custom styling.
pub fn show_help_entry_styled(ui: &mut Ui, entry: &HelpEntry, style: &HelpStyle) {
    // Title
    ui.label(RichText::new(&entry.title).strong().size(14.0).color(style.title_color));
    ui.add_space(4.0);

    // Content
    for paragraph in entry.content.split("\n\n") {
        let trimmed = paragraph.trim();
        if !trimmed.is_empty() {
            ui.label(RichText::new(trimmed).size(12.0).color(style.content_color));
            ui.add_space(4.0);
        }
    }

    // Tip
    if let Some(tip) = &entry.tip {
        ui.add_space(2.0);
        egui::Frame::NONE
            .fill(style.tip_bg_color)
            .corner_radius(4)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Tip:").strong().size(11.0).color(style.tip_color));
                    ui.label(RichText::new(tip).size(11.0).color(style.tip_color));
                });
            });
    }
}

/// A collapsible help section for help panels.
pub fn help_section(ui: &mut Ui, category: &str, title: &str, keys: &[&str], default_open: bool) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .default_open(default_open)
        .show(ui, |ui| {
            for key in keys {
                if let Some(entry) = get_help(category, key) {
                    ui.add_space(8.0);
                    show_help_entry(ui, entry);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_database_loads() {
        let db = HelpDatabase::load();
        assert!(!db.categories.is_empty(), "Should have loaded categories");
    }

    #[test]
    fn test_get_help_entry() {
        let db = HelpDatabase::load();
        let entry = db.get("general", "welcome");
        assert!(entry.is_some(), "Should find welcome entry");

        let entry = entry.unwrap();
        assert!(!entry.title.is_empty());
        assert!(!entry.content.is_empty());
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let db = HelpDatabase::load();
        let entry = db.get("nonexistent", "nonexistent");
        assert!(entry.is_none());
    }

    #[test]
    fn test_help_db_singleton() {
        let db1 = help_db();
        let db2 = help_db();
        assert!(std::ptr::eq(db1, db2), "Should return same instance");
    }
}
