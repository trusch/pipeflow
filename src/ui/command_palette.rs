//! Command palette.
//!
//! Provides fuzzy search for commands and actions.

use crate::core::commands::{CommandAction, CommandEntry, CommandRegistry};
use crate::util::id::NodeId;
use egui::{Key, Ui};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Height of each command entry row.
const ENTRY_HEIGHT: f32 = 48.0;

/// Maximum history entries to track.
const MAX_HISTORY: usize = 50;

/// Command palette state.
pub struct CommandPalette {
    /// Whether the palette is open
    pub open: bool,
    /// Current search text
    pub search: String,
    /// Selected index in results
    pub selected_index: usize,
    /// Fuzzy matcher
    matcher: SkimMatcherV2,
    /// Command usage history (most recent first)
    usage_history: Vec<String>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    /// Creates a new command palette.
    pub fn new() -> Self {
        Self {
            open: false,
            search: String::new(),
            selected_index: 0,
            matcher: SkimMatcherV2::default(),
            usage_history: Vec::new(),
        }
    }

    /// Records that a command was used (for history tracking).
    fn record_usage(&mut self, command_name: &str) {
        // Remove existing entry if present (we'll add it to front)
        self.usage_history.retain(|n| n != command_name);
        // Add to front (most recent)
        self.usage_history.insert(0, command_name.to_string());
        // Trim to max size
        self.usage_history.truncate(MAX_HISTORY);
    }

    /// Gets the history rank for a command (lower = more recent, None = never used).
    fn history_rank(&self, command_name: &str) -> Option<usize> {
        self.usage_history.iter().position(|n| n == command_name)
    }

    /// Opens the command palette.
    pub fn open(&mut self) {
        self.open = true;
        self.search.clear();
        self.selected_index = 0;
    }

    /// Closes the command palette.
    pub fn close(&mut self) {
        self.open = false;
        self.search.clear();
        self.selected_index = 0;
    }

    /// Shows the command palette UI.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        registry: &CommandRegistry,
        node_entries: &[(NodeId, String)],
    ) -> Option<CommandAction> {
        if !self.open {
            return None;
        }

        let mut result = None;

        // Get filtered results (commands + node entries)
        let node_command_entries = self.build_node_entries(node_entries);
        let results = self.filter_commands_with_nodes(registry, &node_command_entries);

        egui::Window::new("Command Palette")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 100.0])
            .fixed_size([400.0, 300.0])
            .show(ctx, |ui| {
                // Search input
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Type a command...")
                        .desired_width(f32::INFINITY),
                );

                // Focus the text input
                response.request_focus();

                // Handle keyboard navigation
                let mut close = false;
                let mut execute = false;

                ui.input(|i| {
                    if i.key_pressed(Key::Escape) {
                        close = true;
                    } else if i.key_pressed(Key::Enter) {
                        execute = true;
                    } else if i.key_pressed(Key::ArrowUp) {
                        if self.selected_index > 0 {
                            self.selected_index -= 1;
                        }
                    } else if i.key_pressed(Key::ArrowDown)
                        && self.selected_index + 1 < results.len()
                    {
                        self.selected_index += 1;
                    }
                });

                if close {
                    self.close();
                    return;
                }

                // Clamp selection
                if !results.is_empty() && self.selected_index >= results.len() {
                    self.selected_index = results.len() - 1;
                }

                if execute && !results.is_empty() {
                    let selected = &results[self.selected_index];
                    result = Some(selected.action.clone());
                    self.record_usage(&selected.name);
                    self.close();
                    return;
                }

                ui.separator();

                // Results list
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (i, entry) in results.iter().enumerate() {
                            let is_selected = i == self.selected_index;

                            // Allocate space for the entry with proper height
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), ENTRY_HEIGHT),
                                egui::Sense::click(),
                            );

                            // Draw the entry
                            if ui.is_rect_visible(rect) {
                                Self::draw_command_entry(ui, rect, entry, is_selected);
                            }

                            // Scroll to keep selected item visible
                            if is_selected {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }

                            if response.clicked() {
                                result = Some(entry.action.clone());
                                self.record_usage(&entry.name);
                                self.close();
                                return;
                            }

                            if response.hovered() {
                                self.selected_index = i;
                            }
                        }

                        if results.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label("No commands found");
                            });
                        }
                    });
            });

        result
    }

    /// Builds CommandEntry items for node search results.
    fn build_node_entries(&self, nodes: &[(NodeId, String)]) -> Vec<CommandEntry> {
        nodes
            .iter()
            .map(|(id, name)| CommandEntry {
                name: format!("\u{2192} {}", name), // → prefix for visual distinction
                description: "Jump to node".to_string(),
                shortcut: None,
                action: CommandAction::GoToNode(*id),
            })
            .collect()
    }

    /// Filters commands and node entries based on search text.
    /// When search is empty, only commands are shown (sorted by history).
    /// When searching, both commands and node entries are fuzzy-matched.
    fn filter_commands_with_nodes<'a>(
        &self,
        registry: &'a CommandRegistry,
        node_entries: &'a [CommandEntry],
    ) -> Vec<&'a CommandEntry> {
        if self.search.is_empty() {
            // No search: show only commands, sorted by history
            let mut commands: Vec<_> = registry.all().iter().collect();
            commands.sort_by(|a, b| {
                let a_rank = self.history_rank(&a.name);
                let b_rank = self.history_rank(&b.name);
                match (a_rank, b_rank) {
                    (Some(a_r), Some(b_r)) => a_r.cmp(&b_r),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.name.cmp(&b.name),
                }
            });
            return commands;
        }

        // Fuzzy match against both commands and node entries
        let all_entries = registry.all().iter().chain(node_entries.iter());

        let mut scored: Vec<_> = all_entries
            .filter_map(|entry| {
                let score = self
                    .matcher
                    .fuzzy_match(&entry.name, &self.search)
                    .or_else(|| self.matcher.fuzzy_match(&entry.description, &self.search));
                score.map(|s| {
                    let history_boost = match self.history_rank(&entry.name) {
                        Some(rank) => 50 - (rank as i64).min(50),
                        None => 0,
                    };
                    (entry, s + history_boost)
                })
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(entry, _)| entry).collect()
    }

    /// Filters commands based on search text (without node entries).
    /// When search is empty, commands are sorted by usage history (most recent first).
    /// When searching, fuzzy match score is combined with history boost.
    #[cfg_attr(not(test), allow(dead_code))]
    fn filter_commands<'a>(&self, registry: &'a CommandRegistry) -> Vec<&'a CommandEntry> {
        self.filter_commands_with_nodes(registry, &[])
    }

    /// Draws a command entry with improved typography.
    fn draw_command_entry(ui: &mut Ui, rect: egui::Rect, entry: &CommandEntry, is_selected: bool) {
        let painter = ui.painter();
        let visuals = ui.style().visuals.clone();

        // Background for selected item (with rounded corners)
        if is_selected {
            painter.rect_filled(rect.shrink(2.0), 6.0, visuals.selection.bg_fill);
        }

        // Hover highlight (subtle)
        let hovered = ui.rect_contains_pointer(rect);
        if hovered && !is_selected {
            painter.rect_filled(rect.shrink(2.0), 6.0, visuals.widgets.hovered.bg_fill);
        }

        let name_color = if is_selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };

        // Muted color for description (more grey than weak_text_color)
        let desc_color = if is_selected {
            visuals.selection.stroke.color.gamma_multiply(0.7)
        } else {
            visuals.weak_text_color().gamma_multiply(0.8)
        };

        let padding = 12.0;
        let name_y = rect.min.y + 14.0;
        let desc_y = rect.min.y + 32.0;

        // Command name (larger, bolder)
        painter.text(
            egui::pos2(rect.min.x + padding, name_y),
            egui::Align2::LEFT_TOP,
            &entry.name,
            egui::FontId::proportional(15.0),
            name_color,
        );

        // Shortcut (if any) - right aligned with name
        if let Some(ref shortcut) = entry.shortcut {
            painter.text(
                egui::pos2(rect.max.x - padding, name_y),
                egui::Align2::RIGHT_TOP,
                shortcut,
                egui::FontId::monospace(11.0),
                visuals.weak_text_color(),
            );
        }

        // Description (smaller, greyer)
        painter.text(
            egui::pos2(rect.min.x + padding, desc_y),
            egui::Align2::LEFT_TOP,
            &entry.description,
            egui::FontId::proportional(11.0),
            desc_color,
        );
    }

    /// Handles global keyboard shortcuts.
    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let mut should_open = false;

        ctx.input(|i| {
            // Ctrl+K or Ctrl+P to open
            if (i.key_pressed(Key::K) || i.key_pressed(Key::P)) && i.modifiers.command {
                should_open = true;
            }
        });

        if should_open {
            self.open();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_palette_lifecycle() {
        let mut palette = CommandPalette::new();

        assert!(!palette.open);

        palette.open();
        assert!(palette.open);
        assert!(palette.search.is_empty());

        palette.search = "test".to_string();
        palette.close();
        assert!(!palette.open);
        assert!(palette.search.is_empty());
    }

    #[test]
    fn test_command_palette_filtering() {
        let palette = CommandPalette::new();
        let registry = CommandRegistry::new();

        // Empty search returns all
        let results = palette.filter_commands(&registry);
        assert!(!results.is_empty());
    }
}
