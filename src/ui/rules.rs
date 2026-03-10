//! Rules management UI.
//!
//! Provides controls for creating and managing connection rules.

use crate::domain::rules::{ConnectionRule, RuleManager, RuleTrigger};
use crate::ui::theme::Theme;
use crate::util::id::RuleId;
use egui::Ui;

/// Rules management panel.
pub struct RulesPanel {
    /// Which rule is currently being edited (if any)
    editing_rule: Option<RuleId>,
    /// Text buffer for editing
    edit_buffer: String,
}

impl Default for RulesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RulesPanel {
    /// Creates a new rules panel.
    pub fn new() -> Self {
        Self {
            editing_rule: None,
            edit_buffer: String::new(),
        }
    }

    /// Shows the rules management panel.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rules: &mut RuleManager,
        _theme: &Theme,
    ) -> RulesPanelResponse {
        let mut response = RulesPanelResponse::default();

        // Create rule button - matches Groups panel style
        ui.horizontal(|ui| {
            if ui.button("+ New Rule").clicked() {
                let rule = ConnectionRule::new("New Rule");
                let id = rules.add_rule(rule);
                self.editing_rule = Some(id);
                self.edit_buffer = "New Rule".to_string();
            }
            ui.weak("(or right-click a node)");
        });

        ui.separator();

        // List of rules
        if rules.is_empty() {
            ui.weak("No rules defined");
        } else {
            let mut rules_to_remove = Vec::new();

            for rule in &mut rules.rules {
                ui.push_id(rule.id.raw(), |ui| {
                    self.show_rule_entry(ui, rule, &mut response, &mut rules_to_remove);
                });
            }

            // Remove rules marked for deletion
            for id in rules_to_remove {
                rules.remove_rule(&id);
            }
        }

        response
    }

    /// Shows a single rule entry.
    fn show_rule_entry(
        &mut self,
        ui: &mut Ui,
        rule: &mut ConnectionRule,
        panel_response: &mut RulesPanelResponse,
        rules_to_remove: &mut Vec<RuleId>,
    ) {
        let is_editing = self.editing_rule == Some(rule.id);
        let rule_id = rule.id;

        // Header row: [checkbox] [name] [count] ... [✎] [▶] [×]
        ui.horizontal(|ui| {
            // Enable/disable checkbox
            ui.checkbox(&mut rule.enabled, "")
                .on_hover_text(if rule.enabled { "Enabled - click to disable" } else { "Disabled - click to enable" });

            if is_editing {
                // Inline text edit
                let text_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .desired_width(ui.available_width() - 70.0)
                );

                if text_edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !self.edit_buffer.is_empty() {
                        rule.name = self.edit_buffer.clone();
                    }
                    self.editing_rule = None;
                    self.edit_buffer.clear();
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.editing_rule = None;
                    self.edit_buffer.clear();
                }

                text_edit.request_focus();
            } else {
                // Rule name - truncated, clickable for info
                let name_response = ui.add(
                    egui::Label::new(&rule.name)
                        .sense(egui::Sense::click())
                        .truncate()
                );

                if name_response.clicked() {
                    panel_response.apply_rule = Some(rule_id);
                }
                name_response.on_hover_text(format!("{}\n\nClick to apply rule", &rule.name));

                // Count badge
                ui.weak(format!("({})", rule.connections.len()));
            }

            // Action buttons - right aligned, consistent order: edit, apply, delete
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Delete (rightmost)
                if ui.small_button("×").on_hover_text("Delete rule").clicked() {
                    rules_to_remove.push(rule_id);
                }

                // Apply
                if ui.small_button("▶").on_hover_text("Apply rule now").clicked() {
                    panel_response.apply_rule = Some(rule_id);
                }

                // Edit/Rename
                if ui.small_button("✎").on_hover_text("Rename rule").clicked() {
                    self.editing_rule = Some(rule_id);
                    self.edit_buffer = rule.name.clone();
                }
            });
        });

        // Detail row: trigger, exclusive, primary node
        ui.horizontal(|ui| {
            ui.add_space(24.0); // Indent to align with name

            // Trigger dropdown - compact
            egui::ComboBox::from_id_salt(("trigger", rule.id.raw()))
                .selected_text(rule.trigger.display_name())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for trigger in RuleTrigger::all() {
                        ui.selectable_value(&mut rule.trigger, *trigger, trigger.display_name());
                    }
                });

            // Exclusive toggle
            ui.checkbox(&mut rule.exclusive, "Excl.")
                .on_hover_text("Remove other connections when applied");
        });

        // Info row: primary node (if set)
        if let Some(ref primary) = rule.primary_node {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("For: {}", primary))
                            .weak()
                            .small()
                    ).truncate()
                ).on_hover_text(primary);
            });
        }

        ui.add_space(2.0);
        ui.separator();
    }
}

/// Response from the rules panel.
#[derive(Debug, Default)]
pub struct RulesPanelResponse {
    /// Apply a rule immediately
    pub apply_rule: Option<RuleId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rules_panel_response_default() {
        let response = RulesPanelResponse::default();
        assert!(response.apply_rule.is_none());
    }
}
