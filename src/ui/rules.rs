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
    matched_outputs: usize,
    matched_inputs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationStatus {
    Disabled,
    Draft,
    Ready,
    Active,
    Waiting,
    Partial,
}

impl AutomationStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::Draft => "Needs details",
            Self::Ready => "Ready",
            Self::Active => "Active",
            Self::Waiting => "Waiting for a match",
            Self::Partial => "Partially applied",
        }
    }

    fn color(self, ui: &Ui) -> Color32 {
        match self {
            Self::Disabled => ui.visuals().weak_text_color(),
            Self::Draft => Color32::from_rgb(150, 180, 255),
            Self::Ready => Color32::from_rgb(120, 200, 255),
            Self::Active => Color32::from_rgb(120, 220, 150),
            Self::Waiting => Color32::from_rgb(255, 200, 110),
            Self::Partial => ui.visuals().warn_fg_color,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Disabled => egui_phosphor::regular::PAUSE,
            Self::Draft => egui_phosphor::regular::PENCIL_SIMPLE_LINE,
            Self::Ready => egui_phosphor::regular::SPARKLE,
            Self::Active => egui_phosphor::regular::CHECK_CIRCLE,
            Self::Waiting => egui_phosphor::regular::HOURGLASS_LOW,
            Self::Partial => egui_phosphor::regular::WARNING,
        }
    }

    fn summary(self, info: &RuleMatchInfo) -> String {
        match self {
            Self::Disabled => "Paused".to_string(),
            Self::Draft => "No connections defined".to_string(),
            Self::Ready => format!("{} possible, none live", info.total_possible),
            Self::Active => format!("{}/{} live", info.total_active, info.total_possible),
            Self::Waiting => "Waiting for matching nodes".to_string(),
            Self::Partial => format!("{}/{} live", info.total_active, info.total_possible),
        }
    }
}

/// Format a MatchPattern as a compact display string.
fn format_pattern(p: &MatchPattern) -> String {
    let app = if p.app_name.is_empty() {
        "*"
    } else {
        &p.app_name
    };
    let node = if p.node_name.is_empty() {
        "*"
    } else {
        &p.node_name
    };
    let port = if p.port_name.is_empty() {
        "*"
    } else {
        &p.port_name
    };
    format!("{}:{}:{}", app, node, port)
}

/// Compute live match info for a rule against the current graph.
fn compute_match_info(rule: &ConnectionRule, graph: &GraphState) -> RuleMatchInfo {
    let mut specs = Vec::with_capacity(rule.connections.len());
    let mut total_possible: usize = 0;
    let mut total_active: usize = 0;
    let mut matched_outputs: usize = 0;
    let mut matched_inputs: usize = 0;

    for conn in &rule.connections {
        let matched_outputs_for_spec =
            find_matching_ports(&conn.output_pattern, PortDirection::Output, graph);
        let matched_inputs_for_spec =
            find_matching_ports(&conn.input_pattern, PortDirection::Input, graph);

        let mut active_links = HashSet::new();
        for out in &matched_outputs_for_spec {
            for inp in &matched_inputs_for_spec {
                let linked = graph
                    .links
                    .values()
                    .any(|l| l.output_port == out.port_id && l.input_port == inp.port_id);
                if linked {
                    active_links.insert((out.port_id, inp.port_id));
                }
            }
        }

        let possible = matched_outputs_for_spec.len() * matched_inputs_for_spec.len();
        let active = active_links.len();
        total_possible += possible;
        total_active += active;
        matched_outputs += matched_outputs_for_spec.len();
        matched_inputs += matched_inputs_for_spec.len();

        specs.push(SpecMatchInfo {
            output_pattern: conn.output_pattern.clone(),
            input_pattern: conn.input_pattern.clone(),
            matched_outputs: matched_outputs_for_spec,
            matched_inputs: matched_inputs_for_spec,
            active_links,
        });
    }

    RuleMatchInfo {
        specs,
        total_possible,
        total_active,
        matched_outputs,
        matched_inputs,
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
            if pattern.matches_runtime(node.application_name.as_deref(), &node.name, &p.name) {
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

        if ui.button("+ New Automation").clicked() {
            let rule = ConnectionRule::new("New automation");
            let id = rules.add_rule(rule);
            self.editing_rule = Some(id);
            self.edit_buffer = "New automation".to_string();
        }

        ui.add_space(4.0);
        ui.separator();

        if rules.is_empty() {
            ui.weak("No automations defined yet");
        } else {
            let mut rules_to_remove = Vec::new();
            let match_infos: Vec<_> = rules
                .rules
                .iter()
                .map(|rule| compute_match_info(rule, graph))
                .collect();

            for (rule, info) in rules.rules.iter_mut().zip(match_infos.iter()) {
                ui.push_id(rule.id.raw(), |ui| {
                    self.show_rule_entry(ui, rule, info, &mut response, &mut rules_to_remove);
                });
                ui.add_space(6.0);
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
        let status = automation_status(rule, info);
        let when_text = describe_trigger(rule);
        let connect_text = describe_rule_target(rule);
        let accent = status.color(ui);

        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.8)))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.checkbox(&mut rule.enabled, "")
                        .on_hover_text(if rule.enabled {
                            "Automation enabled"
                        } else {
                            "Automation disabled"
                        });

                    ui.vertical(|ui| {
                        if is_editing {
                            let text_edit = ui.add(
                                egui::TextEdit::singleline(&mut self.edit_buffer)
                                    .desired_width(240.0),
                            );

                            if text_edit.lost_focus()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                if !self.edit_buffer.trim().is_empty() {
                                    rule.name = self.edit_buffer.trim().to_string();
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
                            let name_response = ui.add(
                                egui::Label::new(RichText::new(&rule.name).strong().size(15.0))
                                    .sense(egui::Sense::click())
                                    .truncate(),
                            );
                            if name_response.clicked() {
                                toggle_expanded(&mut self.expanded_rules, rule_id);
                            }
                            name_response.on_hover_text("Click to expand or collapse details");
                        }

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            status_badge(ui, status);
                            ui.weak(status.summary(info));
                        });
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui
                            .small_button(egui_phosphor::regular::TRASH)
                            .on_hover_text("Delete automation")
                            .clicked()
                        {
                            rules_to_remove.push(rule_id);
                        }

                        if ui
                            .small_button(egui_phosphor::regular::PLAY)
                            .on_hover_text("Apply now")
                            .clicked()
                        {
                            panel_response.apply_rule = Some(rule_id);
                        }

                        if ui
                            .small_button(egui_phosphor::regular::PENCIL_SIMPLE)
                            .on_hover_text("Rename automation")
                            .clicked()
                        {
                            self.editing_rule = Some(rule_id);
                            self.edit_buffer = rule.name.clone();
                        }

                        if ui
                            .small_button(if is_expanded {
                                egui_phosphor::regular::CARET_UP
                            } else {
                                egui_phosphor::regular::CARET_DOWN
                            })
                            .on_hover_text("Show match details")
                            .clicked()
                        {
                            toggle_expanded(&mut self.expanded_rules, rule_id);
                        }
                    });
                });

                ui.add_space(8.0);
                plain_language_row(ui, "When", &when_text);
                plain_language_row(ui, "Connect", &connect_text);

                if is_expanded {
                    ui.add_space(8.0);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Match details").strong());
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(format!("{} source match(es)", info.matched_outputs));
                                ui.weak(format!("{} target match(es)", info.matched_inputs));
                                ui.weak(format!("{} potential link(s)", info.total_possible));
                                ui.weak(format!("{} live", info.total_active));
                            });
                            ui.add_space(6.0);
                            for spec_info in &info.specs {
                                self.show_spec_detail(ui, spec_info);
                            }
                        });
                }
            });
    }

    /// Show detail for a single ConnectionSpec.
    fn show_spec_detail(&self, ui: &mut Ui, spec: &SpecMatchInfo) {
        let green = Color32::from_rgb(0, 180, 120);
        let weak_color = ui.visuals().weak_text_color();

        if spec.matched_outputs.is_empty() && spec.matched_inputs.is_empty() {
            ui.horizontal(|ui| {
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
                ui.label(
                    RichText::new("(waiting for both sides)")
                        .weak()
                        .small()
                        .italics(),
                );
            });
            return;
        }

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

        if spec.matched_outputs.is_empty() {
            ui.horizontal(|ui| {
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
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}  {}  {}",
                        src_node,
                        egui_phosphor::regular::ARROW_RIGHT,
                        dst_node
                    ))
                    .small()
                    .strong(),
                );
            });

            for entry in &ports {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(egui_phosphor::regular::CIRCLE)
                            .small()
                            .color(green),
                    );
                    ui.label(RichText::new(&entry.out_port).small());
                    ui.label(
                        RichText::new(egui_phosphor::regular::ARROW_RIGHT)
                            .small()
                            .weak(),
                    );
                    ui.label(
                        RichText::new(egui_phosphor::regular::CIRCLE)
                            .small()
                            .color(green),
                    );
                    ui.label(RichText::new(&entry.in_port).small());
                    if entry.linked {
                        ui.label(
                            RichText::new(egui_phosphor::regular::CHECK)
                                .small()
                                .color(green),
                        );
                    } else {
                        ui.label(RichText::new("—").small().color(weak_color));
                    }
                });
            }
            ui.add_space(4.0);
        }
    }
}

fn automation_status(rule: &ConnectionRule, info: &RuleMatchInfo) -> AutomationStatus {
    if !rule.enabled {
        return AutomationStatus::Disabled;
    }
    if rule.connections.is_empty() {
        return AutomationStatus::Draft;
    }
    if info.total_possible == 0 {
        return AutomationStatus::Waiting;
    }
    if info.total_active == 0 {
        return AutomationStatus::Ready;
    }
    if info.total_active == info.total_possible {
        return AutomationStatus::Active;
    }
    AutomationStatus::Partial
}

fn describe_trigger(rule: &ConnectionRule) -> String {
    let subject = rule
        .primary_node
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            rule.connections
                .first()
                .map(|spec| first_named_side(&spec.output_pattern))
                .unwrap_or("the matching source")
        });

    match rule.trigger {
        RuleTrigger::OnSourceAppear => format!("{} appears", subject),
        RuleTrigger::OnTargetAppear => format!(
            "{} appears",
            rule.connections
                .first()
                .map(|spec| first_named_side(&spec.input_pattern))
                .unwrap_or("the target")
        ),
        RuleTrigger::OnBothPresent => format!("Both {} and target present", subject),
        RuleTrigger::ManualOnly => "Manual only".to_string(),
    }
}

fn describe_rule_target(rule: &ConnectionRule) -> String {
    if rule.connections.is_empty() {
        return "No ports selected".to_string();
    }

    let destinations: Vec<_> = rule
        .connections
        .iter()
        .map(|spec| first_named_side(&spec.input_pattern).to_string())
        .collect();

    let destination_text = join_human(&destinations);
    if rule.connections.len() == 1 {
        format!("To {}", destination_text)
    } else {
        format!("To {} ({} steps)", destination_text, rule.connections.len())
    }
}

fn first_named_side(pattern: &MatchPattern) -> &str {
    if !pattern.app_name.is_empty() {
        &pattern.app_name
    } else if !pattern.node_name.is_empty() {
        &pattern.node_name
    } else if !pattern.port_name.is_empty() {
        &pattern.port_name
    } else {
        "anything that matches"
    }
}

fn join_human(items: &[String]) -> String {
    match items.len() {
        0 => "nothing yet".to_string(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let mut parts = items.to_vec();
            let last = parts.pop().unwrap();
            format!("{}, and {}", parts.join(", "), last)
        }
    }
}

fn plain_language_row(ui: &mut Ui, label: &str, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{}:", label)).strong());
        ui.label(text);
    });
}

fn status_badge(ui: &mut Ui, status: AutomationStatus) {
    let color = status.color(ui);
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            28,
        ))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(255)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(status.icon()).color(color).small());
                ui.label(RichText::new(status.label()).color(color).small().strong());
            });
        });
}

fn toggle_expanded(expanded_rules: &mut HashSet<RuleId>, rule_id: RuleId) {
    if expanded_rules.contains(&rule_id) {
        expanded_rules.remove(&rule_id);
    } else {
        expanded_rules.insert(rule_id);
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

    #[test]
    fn test_automation_status_disabled() {
        let mut rule = ConnectionRule::new("test");
        rule.enabled = false;
        let info = RuleMatchInfo {
            specs: vec![],
            total_possible: 0,
            total_active: 0,
            matched_outputs: 0,
            matched_inputs: 0,
        };
        assert_eq!(automation_status(&rule, &info), AutomationStatus::Disabled);
    }

    #[test]
    fn test_join_human() {
        assert_eq!(join_human(&[]), "nothing yet");
        assert_eq!(join_human(&["A".into()]), "A");
        assert_eq!(join_human(&["A".into(), "B".into()]), "A and B");
    }
}
