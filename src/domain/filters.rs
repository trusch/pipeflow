//! Graph filtering predicates.
//!
//! Provides filtering capabilities to reduce graph complexity.

use crate::domain::graph::{MediaClass, Node, Port, PortDirection};
use crate::util::id::PortId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A predicate for filtering nodes in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterPredicate {
    /// Match by media class
    MediaClass(MediaClass),
    /// Match by port direction (nodes with ports of this direction)
    Direction(PortDirection),
    /// Match by application name (substring match)
    ApplicationName(String),
    /// Match by node name (substring match)
    NodeName(String),
    /// Match active nodes only
    ActiveOnly,
    /// Match audio nodes only
    AudioOnly,
    /// Match video nodes only
    VideoOnly,
    /// Match MIDI nodes only
    MidiOnly,
    /// Custom predicate with a name
    Custom(String),
}

impl FilterPredicate {
    /// Returns a display name for this filter.
    pub fn display_name(&self) -> String {
        match self {
            Self::MediaClass(mc) => mc.display_name().to_string(),
            Self::Direction(dir) => match dir {
                PortDirection::Input => "Inputs".to_string(),
                PortDirection::Output => "Outputs".to_string(),
            },
            Self::ApplicationName(name) => format!("App: {}", name),
            Self::NodeName(name) => format!("Name: {}", name),
            Self::ActiveOnly => "Active Only".to_string(),
            Self::AudioOnly => "Audio".to_string(),
            Self::VideoOnly => "Video".to_string(),
            Self::MidiOnly => "MIDI".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }

    /// Tests if a node matches this predicate.
    pub fn matches(&self, node: &Node) -> bool {
        match self {
            Self::MediaClass(mc) => node.media_class.as_ref() == Some(mc),
            Self::Direction(_) => true, // Use matches_with_ports for accurate Direction check
            Self::ApplicationName(name) => node
                .application_name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&name.to_lowercase()))
                .unwrap_or(false),
            Self::NodeName(name) => {
                node.name.to_lowercase().contains(&name.to_lowercase())
                    || node
                        .display_name()
                        .to_lowercase()
                        .contains(&name.to_lowercase())
            }
            Self::ActiveOnly => node.is_active,
            Self::AudioOnly => node
                .media_class
                .as_ref()
                .map(|mc| mc.is_audio())
                .unwrap_or(false),
            Self::VideoOnly => node
                .media_class
                .as_ref()
                .map(|mc| mc.is_video())
                .unwrap_or(false),
            Self::MidiOnly => node
                .media_class
                .as_ref()
                .map(|mc| mc.is_midi())
                .unwrap_or(false),
            Self::Custom(_) => true, // Custom predicates need external logic
        }
    }

    /// Tests if a node matches this predicate, with port information for Direction filtering.
    pub fn matches_with_ports(&self, node: &Node, ports: &HashMap<PortId, Port>) -> bool {
        match self {
            Self::Direction(dir) => {
                // Check if the node has any ports with the specified direction
                node.port_ids
                    .iter()
                    .any(|pid| ports.get(pid).map(|p| p.direction == *dir).unwrap_or(false))
            }
            _ => self.matches(node),
        }
    }
}

/// A set of active filters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSet {
    /// Include filters (node must match at least one)
    pub include: Vec<FilterPredicate>,
    /// Exclude filters (node must not match any)
    pub exclude: Vec<FilterPredicate>,
    /// Search text (matches name or app name)
    pub search: Option<String>,
}

impl FilterSet {
    /// Creates an empty filter set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an include filter.
    pub fn add_include(&mut self, predicate: FilterPredicate) {
        if !self.include.contains(&predicate) {
            self.include.push(predicate);
        }
    }

    /// Removes an include filter.
    pub fn remove_include(&mut self, predicate: &FilterPredicate) {
        self.include.retain(|p| p != predicate);
    }

    /// Adds an exclude filter.
    #[cfg(test)]
    pub fn add_exclude(&mut self, predicate: FilterPredicate) {
        if !self.exclude.contains(&predicate) {
            self.exclude.push(predicate);
        }
    }

    /// Sets the search text.
    pub fn set_search(&mut self, text: Option<String>) {
        self.search = text.filter(|s| !s.is_empty());
    }

    /// Clears all filters.
    pub fn clear(&mut self) {
        self.include.clear();
        self.exclude.clear();
        self.search = None;
    }

    /// Returns true if no filters are active.
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty() && self.search.is_none()
    }

    /// Tests if a node passes all filters (without port info for Direction filtering).
    #[cfg(test)]
    pub fn matches(&self, node: &Node) -> bool {
        self.matches_with_ports(node, &HashMap::new())
    }

    /// Tests if a node passes all filters, with port information for Direction filtering.
    pub fn matches_with_ports(&self, node: &Node, ports: &HashMap<PortId, Port>) -> bool {
        // Check exclude filters first
        for predicate in &self.exclude {
            if predicate.matches_with_ports(node, ports) {
                return false;
            }
        }

        // Check include filters (if any)
        if !self.include.is_empty() {
            let matches_any = self
                .include
                .iter()
                .any(|p| p.matches_with_ports(node, ports));
            if !matches_any {
                return false;
            }
        }

        // Check search text
        if let Some(ref search) = self.search {
            let search_lower = search.to_lowercase();
            let name_match = node.name.to_lowercase().contains(&search_lower)
                || node.display_name().to_lowercase().contains(&search_lower);
            let app_match = node
                .application_name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&search_lower))
                .unwrap_or(false);

            if !name_match && !app_match {
                return false;
            }
        }

        true
    }

    /// Returns a description of the active filters.
    pub fn description(&self) -> String {
        let mut parts = Vec::new();

        if !self.include.is_empty() {
            let names: Vec<_> = self.include.iter().map(|p| p.display_name()).collect();
            parts.push(format!("Include: {}", names.join(", ")));
        }

        if !self.exclude.is_empty() {
            let names: Vec<_> = self.exclude.iter().map(|p| p.display_name()).collect();
            parts.push(format!("Exclude: {}", names.join(", ")));
        }

        if let Some(ref search) = self.search {
            parts.push(format!("Search: \"{}\"", search));
        }

        if parts.is_empty() {
            "No filters".to_string()
        } else {
            parts.join(" | ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::id::NodeId;

    fn make_node(name: &str, media_class: Option<MediaClass>, app_name: Option<&str>) -> Node {
        let mut node = Node::new(NodeId::new(1), name.to_string());
        node.media_class = media_class;
        node.application_name = app_name.map(String::from);
        node
    }

    #[test]
    fn test_filter_media_class() {
        let predicate = FilterPredicate::MediaClass(MediaClass::AudioSink);

        let sink = make_node("Speakers", Some(MediaClass::AudioSink), None);
        let source = make_node("Mic", Some(MediaClass::AudioSource), None);

        assert!(predicate.matches(&sink));
        assert!(!predicate.matches(&source));
    }

    #[test]
    fn test_filter_application_name() {
        let predicate = FilterPredicate::ApplicationName("fire".to_string());

        let firefox = make_node("Firefox", None, Some("Firefox"));
        let chrome = make_node("Chrome", None, Some("Google Chrome"));

        assert!(predicate.matches(&firefox));
        assert!(!predicate.matches(&chrome));
    }

    #[test]
    fn test_filter_audio_only() {
        let predicate = FilterPredicate::AudioOnly;

        let audio = make_node("Audio", Some(MediaClass::AudioSink), None);
        let video = make_node("Video", Some(MediaClass::VideoSource), None);

        assert!(predicate.matches(&audio));
        assert!(!predicate.matches(&video));
    }

    #[test]
    fn test_filter_set_include() {
        let mut filters = FilterSet::new();
        filters.add_include(FilterPredicate::AudioOnly);

        let audio = make_node("Audio", Some(MediaClass::AudioSink), None);
        let video = make_node("Video", Some(MediaClass::VideoSource), None);

        assert!(filters.matches(&audio));
        assert!(!filters.matches(&video));
    }

    #[test]
    fn test_filter_set_exclude() {
        let mut filters = FilterSet::new();
        filters.add_exclude(FilterPredicate::VideoOnly);

        let audio = make_node("Audio", Some(MediaClass::AudioSink), None);
        let video = make_node("Video", Some(MediaClass::VideoSource), None);

        assert!(filters.matches(&audio));
        assert!(!filters.matches(&video));
    }

    #[test]
    fn test_filter_set_search() {
        let mut filters = FilterSet::new();
        filters.set_search(Some("speak".to_string()));

        let speakers = make_node("Speakers", None, None);
        let mic = make_node("Microphone", None, None);

        assert!(filters.matches(&speakers));
        assert!(!filters.matches(&mic));
    }

    #[test]
    fn test_filter_set_combined() {
        let mut filters = FilterSet::new();
        filters.add_include(FilterPredicate::AudioOnly);
        filters.add_exclude(FilterPredicate::ApplicationName("ignored".to_string()));

        let good = make_node("Good", Some(MediaClass::AudioSink), Some("Wanted App"));
        let ignored = make_node("Ignored", Some(MediaClass::AudioSink), Some("Ignored App"));
        let video = make_node("Video", Some(MediaClass::VideoSource), None);

        assert!(filters.matches(&good));
        assert!(!filters.matches(&ignored));
        assert!(!filters.matches(&video));
    }

    #[test]
    fn test_filter_empty_passes_all() {
        let filters = FilterSet::new();
        assert!(filters.is_empty());
        let node = make_node("Anything", None, None);
        assert!(filters.matches(&node));
    }

    #[test]
    fn test_filter_set_clear() {
        let mut filters = FilterSet::new();
        filters.add_include(FilterPredicate::AudioOnly);
        filters.set_search(Some("test".into()));
        assert!(!filters.is_empty());
        filters.clear();
        assert!(filters.is_empty());
    }

    #[test]
    fn test_filter_description_formats() {
        let mut filters = FilterSet::new();
        assert_eq!(filters.description(), "No filters");

        filters.add_include(FilterPredicate::AudioOnly);
        assert!(filters.description().contains("Include"));

        filters.add_exclude(FilterPredicate::VideoOnly);
        assert!(filters.description().contains("Exclude"));

        filters.set_search(Some("test".into()));
        assert!(filters.description().contains("Search"));
    }

    #[test]
    fn test_filter_add_include_deduplication() {
        let mut filters = FilterSet::new();
        filters.add_include(FilterPredicate::AudioOnly);
        filters.add_include(FilterPredicate::AudioOnly);
        assert_eq!(filters.include.len(), 1);
    }

    #[test]
    fn test_filter_set_search_empty_becomes_none() {
        let mut filters = FilterSet::new();
        filters.set_search(Some("".into()));
        assert!(filters.search.is_none());
    }

    #[test]
    fn test_filter_search_case_insensitive() {
        let mut filters = FilterSet::new();
        filters.set_search(Some("SPEAK".into()));
        let node = make_node("speakers", None, None);
        assert!(filters.matches(&node));
    }

    #[test]
    fn test_filter_search_matches_app_name() {
        let mut filters = FilterSet::new();
        filters.set_search(Some("fire".into()));
        let node = make_node("some_node", None, Some("Firefox"));
        assert!(filters.matches(&node));
    }

    #[test]
    fn test_filter_node_name_predicate() {
        let predicate = FilterPredicate::NodeName("speak".to_string());
        let matching = make_node("Speakers", None, None);
        let non_matching = make_node("Microphone", None, None);
        assert!(predicate.matches(&matching));
        assert!(!predicate.matches(&non_matching));
    }

    #[test]
    fn test_filter_active_only() {
        let predicate = FilterPredicate::ActiveOnly;
        let mut active_node = make_node("Active", None, None);
        active_node.is_active = true;
        let mut inactive_node = make_node("Inactive", None, None);
        inactive_node.is_active = false;
        assert!(predicate.matches(&active_node));
        assert!(!predicate.matches(&inactive_node));
    }

    #[test]
    fn test_filter_midi_only() {
        let predicate = FilterPredicate::MidiOnly;
        let midi = make_node("MIDI", Some(MediaClass::MidiSource), None);
        let audio = make_node("Audio", Some(MediaClass::AudioSink), None);
        assert!(predicate.matches(&midi));
        assert!(!predicate.matches(&audio));
    }

    #[test]
    fn test_filter_video_only() {
        let predicate = FilterPredicate::VideoOnly;
        let video = make_node("Video", Some(MediaClass::VideoSource), None);
        let audio = make_node("Audio", Some(MediaClass::AudioSink), None);
        assert!(predicate.matches(&video));
        assert!(!predicate.matches(&audio));
    }

    #[test]
    fn test_filter_remove_include() {
        let mut filters = FilterSet::new();
        filters.add_include(FilterPredicate::AudioOnly);
        filters.add_include(FilterPredicate::VideoOnly);
        assert_eq!(filters.include.len(), 2);
        filters.remove_include(&FilterPredicate::AudioOnly);
        assert_eq!(filters.include.len(), 1);
    }

    #[test]
    fn test_filter_direction_with_ports() {
        let predicate = FilterPredicate::Direction(PortDirection::Input);
        let mut node = Node::new(NodeId::new(1), "test".into());
        node.port_ids = vec![PortId::new(10)];

        let mut ports = HashMap::new();
        ports.insert(
            PortId::new(10),
            Port {
                id: PortId::new(10),
                node_id: NodeId::new(1),
                name: "in".into(),
                direction: PortDirection::Input,
                channel: None,
                physical_path: None,
                alias: None,
                is_monitor: false,
                is_control: false,
            },
        );

        assert!(predicate.matches_with_ports(&node, &ports));

        // Without ports, Direction always returns true from matches()
        assert!(predicate.matches(&node));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::domain::graph::{Node, NodeLayer};
    use crate::util::id::NodeId;
    use proptest::prelude::*;

    fn arb_node() -> impl Strategy<Value = Node> {
        (
            1u32..10000,
            "[a-zA-Z ]{1,20}",
            proptest::option::of("[a-zA-Z ]{1,20}"),
            proptest::bool::ANY,
        )
            .prop_map(|(id, name, app_name, is_active)| {
                let mut node = Node::new(NodeId::new(id), name);
                node.application_name = app_name;
                node.is_active = is_active;
                node.layer = NodeLayer::Session;
                node
            })
    }

    proptest! {
        #[test]
        fn empty_filter_passes_any_node(node in arb_node()) {
            let filters = FilterSet::new();
            assert!(filters.matches(&node));
        }

        #[test]
        fn active_only_filter_correct(node in arb_node()) {
            let mut filters = FilterSet::new();
            filters.add_include(FilterPredicate::ActiveOnly);
            assert_eq!(filters.matches(&node), node.is_active);
        }

        #[test]
        fn search_never_panics(
            node in arb_node(),
            search in "[a-zA-Z]{0,10}",
        ) {
            let mut filters = FilterSet::new();
            filters.set_search(Some(search));
            // Should not panic regardless of input
            let _ = filters.matches(&node);
        }
    }
}
