//! Connection rule definitions and management.
//!
//! Allows users to define persistent connection patterns that auto-apply
//! when matching nodes/ports appear.

use crate::util::id::{LinkId, PortId, RuleId};
use serde::{Deserialize, Serialize};

/// When a rule should attempt to create connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RuleTrigger {
    /// Trigger when the source application appears
    OnSourceAppear,
    /// Trigger when the target application appears
    OnTargetAppear,
    /// Trigger only when both source and target are present
    #[default]
    OnBothPresent,
    /// Never auto-trigger; only apply manually
    ManualOnly,
}

impl RuleTrigger {
    /// Returns a human-readable display name for the trigger.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OnSourceAppear => "Source Appears",
            Self::OnTargetAppear => "Target Appears",
            Self::OnBothPresent => "Both Present",
            Self::ManualOnly => "Manual Only",
        }
    }

    /// Returns all available trigger options.
    pub fn all() -> &'static [RuleTrigger] {
        &[
            Self::OnSourceAppear,
            Self::OnTargetAppear,
            Self::OnBothPresent,
            Self::ManualOnly,
        ]
    }
}

/// A glob-style pattern for matching node/port identifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MatchPattern {
    /// Pattern for app_name (e.g., "*DAW*", "Firefox", "")
    /// Empty string means "any"
    pub app_name: String,
    /// Pattern for node_name (e.g., "alsa_output.*")
    pub node_name: String,
    /// Pattern for port_name (e.g., "playback_FL", "monitor_*")
    pub port_name: String,
}

impl MatchPattern {
    /// Creates a new match pattern.
    #[cfg(test)]
    pub fn new(
        app_name: impl Into<String>,
        node_name: impl Into<String>,
        port_name: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            node_name: node_name.into(),
            port_name: port_name.into(),
        }
    }

    /// Creates a pattern that matches a specific port exactly.
    pub fn exact(app_name: Option<&str>, node_name: &str, port_name: &str) -> Self {
        Self {
            app_name: app_name.unwrap_or("").to_string(),
            node_name: node_name.to_string(),
            port_name: port_name.to_string(),
        }
    }

    /// Tests if this pattern matches the given identifiers.
    /// Uses glob-style matching (* = any characters, ? = single char).
    pub fn matches(&self, app_name: Option<&str>, node_name: &str, port_name: &str) -> bool {
        Self::glob_match(&self.app_name, app_name.unwrap_or(""))
            && Self::glob_match(&self.node_name, node_name)
            && Self::glob_match(&self.port_name, port_name)
    }

    /// Simple glob matching (supports * and ?).
    fn glob_match(pattern: &str, text: &str) -> bool {
        if pattern.is_empty() {
            return true; // Empty pattern matches anything
        }
        glob_match::glob_match(pattern, text)
    }

}


/// A single port-to-port connection specification within a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionSpec {
    /// Pattern for the output (source) port
    pub output_pattern: MatchPattern,
    /// Pattern for the input (sink) port
    pub input_pattern: MatchPattern,
}

impl ConnectionSpec {
    /// Creates a new connection specification.
    pub fn new(output_pattern: MatchPattern, input_pattern: MatchPattern) -> Self {
        Self {
            output_pattern,
            input_pattern,
        }
    }
}

/// A connection rule defining a set of connections to establish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRule {
    /// Unique rule ID
    pub id: RuleId,
    /// Human-readable rule name
    pub name: String,
    /// Whether the rule is enabled
    pub enabled: bool,
    /// When to trigger auto-connection
    pub trigger: RuleTrigger,
    /// The connections this rule creates
    pub connections: Vec<ConnectionSpec>,
    /// If true, remove all other connections from matched nodes when rule is applied
    #[serde(default)]
    pub exclusive: bool,
    /// The primary node/app this rule was created for (for display purposes)
    #[serde(default)]
    pub primary_node: Option<String>,
}

impl ConnectionRule {
    /// Creates a new empty rule with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: RuleId::new(),
            name: name.into(),
            enabled: true,
            trigger: RuleTrigger::default(),
            connections: Vec::new(),
            exclusive: false,
            primary_node: None,
        }
    }

    /// Creates a rule from a snapshot of current connections.
    pub fn from_snapshot(
        name: impl Into<String>,
        connections: Vec<ConnectionSpec>,
        primary_node: Option<String>,
    ) -> Self {
        Self {
            id: RuleId::new(),
            name: name.into(),
            enabled: true,
            trigger: RuleTrigger::OnBothPresent,
            connections,
            exclusive: false,
            primary_node,
        }
    }
}

/// Pending connection to be created by a rule.
#[derive(Debug, Clone)]
pub struct PendingConnection {
    /// Output port ID
    pub output_port: PortId,
    /// Input port ID
    pub input_port: PortId,
    /// Rule that triggered this connection (for diagnostics/logging)
    #[allow(dead_code)]
    pub rule_id: RuleId,
}

/// Manager for connection rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleManager {
    /// All rules
    pub rules: Vec<ConnectionRule>,
    /// Counter for generating default names
    next_rule_number: usize,
    /// Pending connections to be created (runtime only, not serialized)
    #[serde(skip)]
    pub pending_connections: Vec<PendingConnection>,
    /// Pending links to be removed (for exclusive rules, runtime only)
    #[serde(skip)]
    pub pending_disconnections: Vec<LinkId>,
}

impl RuleManager {
    /// Creates a new empty rule manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new rule and returns its ID.
    pub fn add_rule(&mut self, rule: ConnectionRule) -> RuleId {
        let id = rule.id;
        self.rules.push(rule);
        id
    }

    /// Creates a rule from a snapshot and adds it.
    pub fn create_from_snapshot(
        &mut self,
        name: Option<String>,
        connections: Vec<ConnectionSpec>,
        primary_node: Option<String>,
    ) -> RuleId {
        self.next_rule_number += 1;
        let name = name.unwrap_or_else(|| format!("Rule {}", self.next_rule_number));
        let rule = ConnectionRule::from_snapshot(name, connections, primary_node);
        self.add_rule(rule)
    }

    /// Removes a rule by ID.
    pub fn remove_rule(&mut self, id: &RuleId) -> Option<ConnectionRule> {
        if let Some(pos) = self.rules.iter().position(|r| r.id == *id) {
            Some(self.rules.remove(pos))
        } else {
            None
        }
    }

    /// Gets a rule by ID.
    pub fn get_rule(&self, id: &RuleId) -> Option<&ConnectionRule> {
        self.rules.iter().find(|r| r.id == *id)
    }

    /// Returns all enabled rules.
    pub fn enabled_rules(&self) -> impl Iterator<Item = &ConnectionRule> {
        self.rules.iter().filter(|r| r.enabled)
    }

    /// Toggles a rule's enabled state.
    #[cfg(test)]
    pub fn toggle_enabled(&mut self, id: &RuleId) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == *id) {
            rule.enabled = !rule.enabled;
        }
    }

    /// Returns true if there are no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Returns the number of rules.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Queues a pending connection to be created.
    pub fn queue_connection(&mut self, output_port: PortId, input_port: PortId, rule_id: RuleId) {
        self.pending_connections.push(PendingConnection {
            output_port,
            input_port,
            rule_id,
        });
    }

    /// Takes all pending connections, clearing the queue.
    pub fn take_pending_connections(&mut self) -> Vec<PendingConnection> {
        std::mem::take(&mut self.pending_connections)
    }

    /// Queues a link to be removed (for exclusive rules).
    pub fn queue_disconnection(&mut self, link_id: LinkId) {
        if !self.pending_disconnections.contains(&link_id) {
            self.pending_disconnections.push(link_id);
        }
    }

    /// Takes all pending disconnections, clearing the queue.
    pub fn take_pending_disconnections(&mut self) -> Vec<LinkId> {
        std::mem::take(&mut self.pending_disconnections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_pattern_exact() {
        let pattern = MatchPattern::exact(Some("Firefox"), "firefox", "playback_FL");

        assert!(pattern.matches(Some("Firefox"), "firefox", "playback_FL"));
        assert!(!pattern.matches(Some("Chrome"), "firefox", "playback_FL"));
        assert!(!pattern.matches(Some("Firefox"), "chrome", "playback_FL"));
        assert!(!pattern.matches(Some("Firefox"), "firefox", "playback_FR"));
    }

    #[test]
    fn test_match_pattern_glob() {
        let pattern = MatchPattern::new("*DAW*", "alsa_*", "playback_*");

        assert!(pattern.matches(Some("MyDAWApp"), "alsa_output", "playback_FL"));
        assert!(pattern.matches(Some("DAW"), "alsa_input", "playback_FR"));
        assert!(!pattern.matches(Some("Firefox"), "alsa_output", "playback_FL"));
        assert!(!pattern.matches(Some("MyDAWApp"), "pulse_output", "playback_FL"));
    }

    #[test]
    fn test_match_pattern_empty() {
        let pattern = MatchPattern::new("", "", "");

        // Empty pattern matches anything
        assert!(pattern.matches(Some("Any"), "anything", "whatever"));
        assert!(pattern.matches(None, "node", "port"));
    }

    #[test]
    fn test_rule_manager_crud() {
        let mut manager = RuleManager::new();

        // Create rule
        let rule = ConnectionRule::new("Test Rule");
        let id = manager.add_rule(rule);

        assert_eq!(manager.len(), 1);
        assert!(manager.get_rule(&id).is_some());

        // Toggle enabled
        assert!(manager.get_rule(&id).unwrap().enabled);
        manager.toggle_enabled(&id);
        assert!(!manager.get_rule(&id).unwrap().enabled);

        // Remove rule
        let removed = manager.remove_rule(&id);
        assert!(removed.is_some());
        assert!(manager.is_empty());
    }

    #[test]
    fn test_rule_manager_snapshot() {
        let mut manager = RuleManager::new();

        let connections = vec![ConnectionSpec::new(
            MatchPattern::exact(Some("App"), "node1", "out"),
            MatchPattern::exact(Some("App2"), "node2", "in"),
        )];

        let id = manager.create_from_snapshot(None, connections, Some("MyApp".to_string()));

        let rule = manager.get_rule(&id).unwrap();
        assert_eq!(rule.name, "Rule 1");
        assert_eq!(rule.connections.len(), 1);
        assert!(rule.enabled);
        assert_eq!(rule.trigger, RuleTrigger::OnBothPresent);
        assert_eq!(rule.primary_node, Some("MyApp".to_string()));
    }
}
