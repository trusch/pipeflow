//! Rules management UI.
//!
//! Provides controls for creating and managing connection rules,
//! with live graph matching info and collapsible detail cards.

use crate::core::state::GraphState;
use crate::domain::graph::PortDirection;
use crate::domain::rules::{ConnectionRule, MatchPattern, RuleManager, RuleTrigger};
use crate::ui::theme::Theme;
use crate::util::id::{PortId, RuleId};
use egui::{Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};

/// A matched port: node display name, port name, port ID.
struct PortMatch {
    node_name: String,
    port_name: String,
    port_id: PortId,
}

/// Live match info for a single ConnectionSpec.
struct SpecMatchInfo {
    output_pattern: MatchPattern,
    input_pattern: MatchPattern,
    matched_outputs: Vec<PortMatch>,
    matched_inputs: Vec<PortMatch>,
    /// (output_port_id, input_port_id) pairs that have an active link.
    active_links: HashSet<(PortId, PortId)>,
}

/// Aggregate match info for a whole rule.
struct RuleMatchInfo {
    specs: Vec<SpecMatchInfo>,
    total_possible: usize,
    total_active: usize,
}

/// Format a MatchPattern as a compact display string.
fn format_pattern(p: &MatchPattern) -> String {
    let app = if p.app_name.is_empty() { "*" } else { &p.app_name };
    let node = if p.node_name.is_empty() { "*" } else { &p.node_name };
    let port = if p.port_name.is_empty() { "*" } else { &p.port_name };
    format!("{}:{}:{}", app, node, port)
}

/// Compute live match info for a rule against the current graph.
fn compute_match_info(rule: &ConnectionRule, graph: &GraphState) -> RuleMatchInfo {
    let mut specs = Vec::with_capacity(rule.connections.len());
    let mut total_possible: usize = 0;
    let mut total_active: usize = 0;

    for conn in &rule.connections {
        let matched_outputs = find_matching_ports(&conn.output_pattern, PortDirection::Output, graph);
        let matched_inputs = find_matching_ports(&conn.input_pattern, PortDirection::Input, graph);

        let mut active_links = HashSet::new();
        for out in &matched_outputs {
            for inp in &matched_inputs {
                let linked = graph.links.values().any(|l| {
                    l.output_port == out.port_id && l.input_port == inp.port_id
                });
                if linked {
                    active_links.insert((out.port_id, inp.port_id));
                }
            }
        }

        let possible = matched_outputs.len() * matched_inputs.len();
        let active = active_links.len();
        total_possible += possible;
        total_active += active;

        specs.push(SpecMatchInfo {
            output_pattern: conn.output_pattern.clone(),
            input_pattern: conn.input_pattern.clone(),
            matched_outputs,
            matched_inputs,
            active_links,
        });
    }

    RuleMatchInfo {
        specs,
        total_possible,
        total_active,
    }
}

fn find_matching_ports(
    pattern: &MatchPattern,
    direction: PortDirection,
    graph: &GraphState,
) -> Vec<PortMatch> {
    graph
        .ports
        .values()
        .filter(|p| p.direction == direction)
        .filter_map(|p| {
            let node = graph.get_node(&p.node_id)?;
            if pattern.matches(node.application_name.as_deref(), &node.name, &p.name) {
                Some(PortMatch {
                    node_name: node.display_name().to_string(),
                    port_name: p.name.clone(),
                    port_id: p.id,
                })
            } else {
                None
            }
        })
        .collect()
}

/// A port-pair entry within a node-pair group.
struct PortPairEntry {
    out_port: String,
    in_port: String,
    linked: bool,
}

/// Rules management panel.
pub struct RulesPanel {
    /// Which rule is currently being edited (if any)
    editing_rule: Option<RuleId>,
    /// Text buffer for editing
    edit_buffer: String,
    /// Which rules are expanded to show detail
    expanded_rules: HashSet<RuleId>,
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
            expanded_rules: HashSet::new(),
        }
    }

    /// Shows the rules management panel.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rules: &mut RuleManager,
        graph: &GraphState,
        _theme: &Theme,
    ) -> RulesPanelResponse {
        let mut response = RulesPanelResponse::default();

        // Create rule button
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

        if rules.is_empty() {
            ui.weak("No rules defined");
        } else {
            let mut rules_to_remove = Vec::new();

            // Pre-compute match info for all rules
            let match_infos: Vec<_> = rules
                .rules
                .iter()
                .map(|rule| compute_match_info(rule, graph))
                .collect();

            for (rule, info) in rules.rules.iter_mut().zip(match_infos.iter()) {
                ui.push_id(rule.id.raw(), |ui| {
                    self.show_rule_entry(ui, rule, info, &mut response, &mut rules_to_remove);
                });
            }

            for id in rules_to_remove {
                rules.remove_rule(&id);
            }
        }

        response
    }

    /// Shows a single rule entry with collapsible detail.
    fn show_rule_entry(
        &mut self,
        ui: &mut Ui,
        rule: &mut ConnectionRule,
        info: &RuleMatchInfo,
        panel_response: &mut RulesPanelResponse,
        rules_to_remove: &mut Vec<RuleId>,
    ) {
        let is_editing = self.editing_rule == Some(rule.id);
        let rule_id = rule.id;
        let is_expanded = self.expanded_rules.contains(&rule_id);

        // Header row: [checkbox] [name] ... [edit] [apply] [delete]
        ui.horizontal(|ui| {
            ui.checkbox(&mut rule.enabled, "")
                .on_hover_text(if rule.enabled {
                    "Enabled - click to disable"
                } else {
                    "Disabled - click to enable"
                });

            if is_editing {
                let text_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .desired_width(ui.available_width() - 70.0),
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
                // Rule name - click to toggle expand
                let name_response = ui.add(
                    egui::Label::new(&rule.name)
                        .sense(egui::Sense::click())
                        .truncate(),
                );

                if name_response.clicked() {
                    if is_expanded {
                        self.expanded_rules.remove(&rule_id);
                    } else {
                        self.expanded_rules.insert(rule_id);
                    }
                }
                name_response.on_hover_text("Click to expand/collapse details");
            }

            // Right-aligned action buttons
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(egui_phosphor::regular::X).on_hover_text("Delete rule").clicked() {
                    rules_to_remove.push(rule_id);
                }

                if ui
                    .small_button(egui_phosphor::regular::PLAY)
                    .on_hover_text("Apply rule now")
                    .clicked()
                {
                    panel_response.apply_rule = Some(rule_id);
                }

                if ui
                    .small_button("✎")
                    .on_hover_text("Rename rule")
                    .clicked()
                {
                    self.editing_rule = Some(rule_id);
                    self.edit_buffer = rule.name.clone();
                }
            });
        });

        // Summary row: trigger + match summary
        ui.horizontal(|ui| {
            ui.add_space(24.0);

            // Trigger dropdown
            egui::ComboBox::from_id_salt(("trigger", rule.id.raw()))
                .selected_text(rule.trigger.display_name())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for trigger in RuleTrigger::all() {
                        ui.selectable_value(&mut rule.trigger, *trigger, trigger.display_name());
                    }
                });

            ui.checkbox(&mut rule.exclusive, "Excl.")
                .on_hover_text("Remove other connections when applied");

            // Connection count with status
            let summary = if info.total_possible == 0 {
                RichText::new(format!("{} specs (no matches)", rule.connections.len()))
                    .weak()
            } else if info.total_active == info.total_possible {
                RichText::new(format!(
                    "{} connections ({} active) {}",
                    info.total_possible, info.total_active, egui_phosphor::regular::CHECK
                ))
                .color(Color32::from_rgb(0, 180, 120))
            } else {
                let missing = info.total_possible - info.total_active;
                RichText::new(format!(
                    "{} connections ({} missing) {}",
                    info.total_possible, missing, egui_phosphor::regular::WARNING
                ))
                .color(ui.visuals().warn_fg_color)
            };
            ui.label(summary);
        });

        // Expanded detail: connection specs with matched ports
        if is_expanded {
            ui.add_space(2.0);
            for spec_info in &info.specs {
                self.show_spec_detail(ui, spec_info);
            }
        }

        ui.add_space(2.0);
        ui.separator();
    }

    /// Show detail for a single ConnectionSpec.
    fn show_spec_detail(&self, ui: &mut Ui, spec: &SpecMatchInfo) {
        let green = Color32::from_rgb(0, 180, 120);
        let weak_color = ui.visuals().weak_text_color();

        if spec.matched_outputs.is_empty() && spec.matched_inputs.is_empty() {
            // No matches - show raw patterns
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(format!(
                        "{}  {}  {}",
                        format_pattern(&spec.output_pattern),
                        egui_phosphor::regular::ARROW_RIGHT,
                        format_pattern(&spec.input_pattern),
                    ))
                    .weak()
                    .small(),
                );
            });
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(RichText::new("(no matches)").weak().small().italics());
            });
            return;
        }

        // Group by source_node -> target_node
        let mut groups: HashMap<(String, String), Vec<PortPairEntry>> = HashMap::new();

        for out in &spec.matched_outputs {
            for inp in &spec.matched_inputs {
                let linked = spec.active_links.contains(&(out.port_id, inp.port_id));
                groups
                    .entry((out.node_name.clone(), inp.node_name.clone()))
                    .or_default()
                    .push(PortPairEntry {
                        out_port: out.port_name.clone(),
                        in_port: inp.port_name.clone(),
                        linked,
                    });
            }
        }

        // Also show unmatched sides
        if spec.matched_outputs.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}  {}  ...",
                        egui_phosphor::regular::CIRCLE,
                        format_pattern(&spec.output_pattern),
                        egui_phosphor::regular::ARROW_RIGHT,
                    ))
                    .color(weak_color)
                    .small(),
                );
            });
            return;
        }
        if spec.matched_inputs.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(format!(
                        "...  {}  {} {}",
                        egui_phosphor::regular::ARROW_RIGHT,
                        egui_phosphor::regular::CIRCLE,
                        format_pattern(&spec.input_pattern),
                    ))
                    .color(weak_color)
                    .small(),
                );
            });
            return;
        }

        let mut sorted_groups: Vec<_> = groups.into_iter().collect();
        sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));

        for ((src_node, dst_node), ports) in sorted_groups {
            // Node header
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(format!("{}  {}  {}", src_node, egui_phosphor::regular::ARROW_RIGHT, dst_node))
                        .small()
                        .strong(),
                );
            });

            // Port pairs
            for entry in &ports {
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new(egui_phosphor::regular::CIRCLE).small().color(green));
                    ui.label(RichText::new(&entry.out_port).small());
                    ui.label(RichText::new(egui_phosphor::regular::ARROW_RIGHT).small().weak());
                    ui.label(RichText::new(egui_phosphor::regular::CIRCLE).small().color(green));
                    ui.label(RichText::new(&entry.in_port).small());
                    if entry.linked {
                        ui.label(RichText::new(egui_phosphor::regular::CHECK).small().color(green));
                    } else {
                        ui.label(RichText::new("\u{2014}").small().color(weak_color));
                    }
                });
            }
        }
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

    #[test]
    fn test_format_pattern() {
        let p = MatchPattern::exact(Some("App"), "node", "port");
        assert_eq!(format_pattern(&p), "App:node:port");

        let p = MatchPattern::default();
        assert_eq!(format_pattern(&p), "*:*:*");
    }

    #[test]
    fn test_compute_match_info_empty_rule() {
        let rule = ConnectionRule::new("test");
        let graph = GraphState::default();
        let info = compute_match_info(&rule, &graph);
        assert_eq!(info.total_possible, 0);
        assert_eq!(info.total_active, 0);
        assert!(info.specs.is_empty());
    }
}
