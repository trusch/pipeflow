//! Human-readable explanations for graph elements.
//!
//! Provides functions to generate natural language descriptions of nodes,
//! suitable for tooltips and detail panels.

use crate::core::state::GraphState;
use crate::domain::audio::linear_to_db;
use crate::domain::graph::{MediaClass, Node, NodeLayer, PortDirection};

/// Special node type detected from naming patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialNodeType {
    /// A pipeflow meter node (monitors another node)
    Meter,
    /// A PipeWire channel split node
    ChannelSplit,
    /// A PipeWire adapter node (format conversion)
    Adapter,
    /// A MIDI bridge node
    MidiBridge,
    /// A loopback/virtual device node
    Loopback,
    /// A dummy/null sink node
    DummySink,
    /// A monitor source (captures output)
    Monitor,
    /// Not a special node type
    Regular,
}

impl SpecialNodeType {
    /// Detects the special node type from node properties.
    pub fn detect(node: &Node) -> Self {
        let name_lower = node.name.to_lowercase();
        let desc_lower = node.description.as_ref().map(|d| d.to_lowercase());

        // Pipeflow meter node
        if node.name.starts_with("pipeflow-meter-") {
            return Self::Meter;
        }

        // Check for channel split nodes (PipeWire creates these to split multi-channel)
        if name_lower.contains("channelmix") || name_lower.contains("channel-split") {
            return Self::ChannelSplit;
        }

        // Check for adapter nodes (format/rate conversion)
        if name_lower.contains("adapter") || name_lower.contains("audioconvert") {
            return Self::Adapter;
        }

        // Check for MIDI bridge
        if let Some(mc) = &node.media_class {
            if matches!(mc, MediaClass::MidiSource | MediaClass::MidiSink)
                && (name_lower.contains("bridge") || name_lower.contains("through"))
            {
                return Self::MidiBridge;
            }
        }

        // Check for loopback/virtual devices
        if name_lower.contains("loopback")
            || name_lower.contains("virtual")
            || desc_lower.as_ref().map(|d| d.contains("loopback")).unwrap_or(false)
        {
            return Self::Loopback;
        }

        // Check for dummy/null sinks
        if name_lower.contains("dummy")
            || name_lower.contains("null")
            || name_lower.contains("auto_null")
        {
            return Self::DummySink;
        }

        // Check for monitor sources
        if name_lower.contains("monitor")
            || desc_lower.as_ref().map(|d| d.contains("monitor")).unwrap_or(false)
        {
            return Self::Monitor;
        }

        Self::Regular
    }

    /// Returns a human-readable explanation of what this special node type is.
    pub fn explanation(&self) -> Option<&'static str> {
        match self {
            Self::Meter => Some(
                "This is a metering node created by Pipeflow to monitor audio levels. \
                 It captures the signal without affecting it."
            ),
            Self::ChannelSplit => Some(
                "This is a channel split node created by PipeWire to separate or combine \
                 audio channels (e.g., splitting stereo into left/right or combining mono to stereo)."
            ),
            Self::Adapter => Some(
                "This is an adapter node that converts between different audio formats \
                 or sample rates. PipeWire creates these automatically when needed."
            ),
            Self::MidiBridge => Some(
                "This is a MIDI bridge node that routes MIDI messages between applications \
                 or hardware. It can loop MIDI back to itself for routing purposes."
            ),
            Self::Loopback => Some(
                "This is a loopback/virtual device that routes audio internally. \
                 Useful for capturing application output or routing between apps."
            ),
            Self::DummySink => Some(
                "This is a dummy/null sink that discards audio. It's used as a placeholder \
                 when no real output device is available."
            ),
            Self::Monitor => Some(
                "This is a monitor source that captures audio from an output device. \
                 Use it to record or process what's being played through speakers."
            ),
            Self::Regular => None,
        }
    }
}

/// Generates a human-readable explanation of a node.
///
/// The explanation covers:
/// - What the node is (type, application, layer)
/// - Signal flow direction (source/sink)
/// - Special node type explanations
/// - Audio format details (if applicable)
/// - Port configuration (inputs/outputs)
/// - Current connections
/// - Volume/mute state
pub fn explain_node(node: &Node, graph: &GraphState) -> String {
    let mut lines = Vec::new();

    // Primary description line
    lines.push(build_primary_description(node));

    // Signal flow direction
    if let Some(flow_desc) = describe_signal_flow(node) {
        lines.push(flow_desc);
    }

    // Special node type explanation
    let special_type = SpecialNodeType::detect(node);
    if let Some(explanation) = special_type.explanation() {
        lines.push(String::new()); // Empty line for separation
        lines.push(explanation.to_string());
    }

    // Layer explanation
    lines.push(String::new());
    lines.push(format!(
        "Layer: {} — {}",
        node.layer.display_name(),
        layer_explanation(node.layer)
    ));

    // Status
    if !node.is_active {
        lines.push(format!("{} Currently inactive (paused or suspended)", egui_phosphor::regular::WARNING));
    }

    // Audio format
    if let Some(fmt) = &node.format {
        lines.push(format!(
            "Audio format: {} Hz, {} channel{}, {} bit depth",
            fmt.sample_rate,
            fmt.channels,
            if fmt.channels == 1 { "" } else { "s" },
            format_bit_depth(&fmt.format)
        ));
    }

    // Ports summary
    let ports = graph.ports_for_node(&node.id);
    let input_count = ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input && !p.is_control)
        .count();
    let output_count = ports
        .iter()
        .filter(|p| p.direction == PortDirection::Output && !p.is_control)
        .count();
    let monitor_count = ports.iter().filter(|p| p.is_monitor).count();
    let control_count = ports.iter().filter(|p| p.is_control).count();

    if input_count > 0 || output_count > 0 {
        let mut port_parts = Vec::new();
        if input_count > 0 {
            port_parts.push(format!(
                "{} input{}",
                input_count,
                if input_count == 1 { "" } else { "s" }
            ));
        }
        if output_count > 0 {
            port_parts.push(format!(
                "{} output{}",
                output_count,
                if output_count == 1 { "" } else { "s" }
            ));
        }
        if monitor_count > 0 {
            port_parts.push(format!(
                "{} monitor{}",
                monitor_count,
                if monitor_count == 1 { "" } else { "s" }
            ));
        }
        if control_count > 0 {
            port_parts.push(format!("{} control", control_count));
        }
        lines.push(format!("Ports: {}", port_parts.join(", ")));
    }

    // Connections
    let links = graph.links_for_node(&node.id);
    if !links.is_empty() {
        let outgoing: Vec<_> = links.iter().filter(|l| l.output_node == node.id).collect();
        let incoming: Vec<_> = links.iter().filter(|l| l.input_node == node.id).collect();

        if !outgoing.is_empty() {
            let targets: Vec<String> = outgoing
                .iter()
                .filter_map(|l| graph.get_node(&l.input_node))
                .map(|n| n.display_name().to_string())
                .collect();
            let unique_targets: Vec<_> = dedup_strings(targets);
            if unique_targets.len() <= 3 {
                lines.push(format!("Sends audio to: {}", unique_targets.join(", ")));
            } else {
                lines.push(format!(
                    "Sends audio to: {} and {} others",
                    unique_targets[..2].join(", "),
                    unique_targets.len() - 2
                ));
            }
        }

        if !incoming.is_empty() {
            let sources: Vec<String> = incoming
                .iter()
                .filter_map(|l| graph.get_node(&l.output_node))
                .map(|n| n.display_name().to_string())
                .collect();
            let unique_sources: Vec<_> = dedup_strings(sources);
            if unique_sources.len() <= 3 {
                lines.push(format!("Receives audio from: {}", unique_sources.join(", ")));
            } else {
                lines.push(format!(
                    "Receives audio from: {} and {} others",
                    unique_sources[..2].join(", "),
                    unique_sources.len() - 2
                ));
            }
        }
    } else {
        lines.push("Not connected to anything".to_string());
    }

    // Volume state
    if let Some(vol) = graph.volumes.get(&node.id) {
        let db = linear_to_db(vol.master);
        let db_str = if db == f32::NEG_INFINITY {
            "-∞ dB".to_string()
        } else {
            format!("{:+.1} dB", db)
        };

        let mut vol_parts = vec![format!("{}%", (vol.master * 100.0) as i32), db_str];
        if vol.muted {
            vol_parts.push("MUTED".to_string());
        }
        lines.push(format!("Volume: {}", vol_parts.join(" / ")));
    }

    // Volume control error
    if let Some(err) = graph.volume_control_failed.get(&node.id) {
        lines.push(format!("{} Volume control unavailable: {}", egui_phosphor::regular::WARNING, err));
    }

    lines.join("\n")
}

/// Generates a short one-line summary for tooltips.
pub fn explain_node_short(node: &Node, graph: &GraphState) -> String {
    let mut parts = Vec::new();

    // Type with source/sink indicator
    if let Some(mc) = &node.media_class {
        let type_name = mc.display_name();
        let direction = if mc.is_source() {
            " (source)"
        } else if mc.is_sink() {
            " (sink)"
        } else {
            ""
        };
        parts.push(format!("{}{}", type_name, direction));
    }

    // App name if different from display name
    if let Some(app) = &node.application_name {
        if Some(app.as_str()) != node.description.as_deref() {
            parts.push(format!("from {}", app));
        }
    }

    // Layer indicator for non-session nodes
    if node.layer != NodeLayer::Session {
        parts.push(format!("[{}]", node.layer.short_label()));
    }

    // Connection count
    let links = graph.links_for_node(&node.id);
    if !links.is_empty() {
        parts.push(format!(
            "{} connection{}",
            links.len(),
            if links.len() == 1 { "" } else { "s" }
        ));
    }

    // Active status
    if !node.is_active {
        parts.push("(inactive)".to_string());
    }

    if parts.is_empty() {
        node.display_name().to_string()
    } else {
        parts.join(" · ")
    }
}

/// Builds the primary description line.
fn build_primary_description(node: &Node) -> String {
    let mut parts = Vec::new();

    // Media class as primary type with source/sink indicator
    if let Some(mc) = &node.media_class {
        let type_name = mc.display_name();
        let direction = if mc.is_source() {
            " (produces audio)"
        } else if mc.is_sink() {
            " (receives audio)"
        } else {
            ""
        };
        parts.push(format!("{}{}", type_name, direction));
    } else {
        parts.push("Node".to_string());
    }

    // Application name
    if let Some(app) = &node.application_name {
        parts.push(format!("— {}", app));
    }

    parts.join(" ")
}

/// Describes the signal flow direction for a node.
fn describe_signal_flow(node: &Node) -> Option<String> {
    let mc = node.media_class.as_ref()?;

    let desc = match mc {
        MediaClass::AudioSource | MediaClass::VideoSource | MediaClass::MidiSource => {
            "This is a SOURCE: it generates signal that flows to other nodes."
        }
        MediaClass::AudioSink | MediaClass::VideoSink | MediaClass::MidiSink => {
            "This is a SINK: it receives signal from other nodes (final destination)."
        }
        MediaClass::StreamOutputAudio => {
            "This is an APPLICATION OUTPUT: an app sending audio to be played or processed."
        }
        MediaClass::StreamInputAudio => {
            "This is an APPLICATION INPUT: an app receiving audio (recording, VoIP, etc.)."
        }
        MediaClass::AudioDevice | MediaClass::VideoDevice => {
            "This is a DEVICE: a hardware endpoint that can both send and receive signal."
        }
        MediaClass::AudioVideoSource => {
            "This is an A/V SOURCE: produces both audio and video signal."
        }
        MediaClass::Other(_) => return None,
    };

    Some(desc.to_string())
}

/// Returns a human-readable explanation of the node layer.
fn layer_explanation(layer: NodeLayer) -> &'static str {
    match layer {
        NodeLayer::Hardware => {
            "Physical hardware managed by kernel drivers (soundcards, USB devices, etc.)"
        }
        NodeLayer::Pipewire => {
            "Internal PipeWire node (format adapters, channel mixers, virtual routing)"
        }
        NodeLayer::Session => {
            "Application or service managed by the session manager (WirePlumber)"
        }
    }
}

/// Extracts a human-readable bit depth from format string.
fn format_bit_depth(format: &str) -> &str {
    if format.contains("32") || format.contains("F32") {
        "32-bit float"
    } else if format.contains("24") {
        "24-bit"
    } else if format.contains("16") {
        "16-bit"
    } else if format.contains("8") {
        "8-bit"
    } else {
        format
    }
}

/// Deduplicates strings while preserving order.
fn dedup_strings(strings: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    strings
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::{AudioFormat, Port};
    use crate::util::id::{NodeId, PortId};

    fn create_test_node(id: u32, name: &str) -> Node {
        let mut node = Node::new(NodeId::new(id), name.to_string());
        node.description = Some(format!("{} Description", name));
        node
    }

    #[test]
    fn test_explain_basic_node() {
        let node = create_test_node(1, "TestNode");
        let graph = GraphState::default();

        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("Node"));
        assert!(explanation.contains("Session"));
        assert!(explanation.contains("Not connected"));
    }

    #[test]
    fn test_explain_audio_source() {
        let mut node = create_test_node(1, "Microphone");
        node.media_class = Some(MediaClass::AudioSource);
        node.application_name = Some("Firefox".to_string());
        node.format = Some(AudioFormat {
            sample_rate: 48000,
            channels: 2,
            format: "F32LE".to_string(),
        });

        let graph = GraphState::default();
        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("Audio Source"));
        assert!(explanation.contains("produces audio"));
        assert!(explanation.contains("SOURCE"));
        assert!(explanation.contains("Firefox"));
        assert!(explanation.contains("48000 Hz"));
        assert!(explanation.contains("2 channels"));
    }

    #[test]
    fn test_explain_audio_sink() {
        let mut node = create_test_node(1, "Speakers");
        node.media_class = Some(MediaClass::AudioSink);

        let graph = GraphState::default();
        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("Audio Sink"));
        assert!(explanation.contains("receives audio"));
        assert!(explanation.contains("SINK"));
    }

    #[test]
    fn test_explain_stream_output() {
        let mut node = create_test_node(1, "Firefox");
        node.media_class = Some(MediaClass::StreamOutputAudio);

        let graph = GraphState::default();
        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("APPLICATION OUTPUT"));
    }

    #[test]
    fn test_explain_node_with_ports() {
        let node = create_test_node(1, "Mixer");
        let mut graph = GraphState::default();
        graph.add_node(node.clone());

        // Add input ports
        for i in 0..4 {
            graph.add_port(Port::new(
                PortId::new(10 + i),
                NodeId::new(1),
                format!("in_{}", i),
                PortDirection::Input,
            ));
        }
        // Add output ports
        for i in 0..2 {
            graph.add_port(Port::new(
                PortId::new(20 + i),
                NodeId::new(1),
                format!("out_{}", i),
                PortDirection::Output,
            ));
        }

        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("4 inputs"));
        assert!(explanation.contains("2 outputs"));
    }

    #[test]
    fn test_explain_short_source() {
        let mut node = create_test_node(1, "Mic");
        node.media_class = Some(MediaClass::AudioSource);

        let graph = GraphState::default();
        let short = explain_node_short(&node, &graph);

        assert!(short.contains("Audio Source"));
        assert!(short.contains("(source)"));
    }

    #[test]
    fn test_explain_short_sink() {
        let mut node = create_test_node(1, "Speaker");
        node.media_class = Some(MediaClass::AudioSink);

        let graph = GraphState::default();
        let short = explain_node_short(&node, &graph);

        assert!(short.contains("Audio Sink"));
        assert!(short.contains("(sink)"));
    }

    #[test]
    fn test_explain_inactive_node() {
        let mut node = create_test_node(1, "Disabled");
        node.is_active = false;

        let graph = GraphState::default();
        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("inactive"));
    }

    #[test]
    fn test_explain_hardware_layer() {
        let mut node = create_test_node(1, "ALSA Device");
        node.layer = NodeLayer::Hardware;

        let graph = GraphState::default();
        let explanation = explain_node(&node, &graph);

        assert!(explanation.contains("Hardware"));
        assert!(explanation.contains("kernel drivers"));
    }

    #[test]
    fn test_special_node_meter() {
        let node = create_test_node(1, "pipeflow-meter-123");
        assert_eq!(SpecialNodeType::detect(&node), SpecialNodeType::Meter);

        let explanation = SpecialNodeType::Meter.explanation();
        assert!(explanation.is_some());
        assert!(explanation.unwrap().contains("metering"));
    }

    #[test]
    fn test_special_node_loopback() {
        let node = create_test_node(1, "loopback-device");
        assert_eq!(SpecialNodeType::detect(&node), SpecialNodeType::Loopback);
    }

    #[test]
    fn test_special_node_dummy() {
        let node = create_test_node(1, "auto_null");
        assert_eq!(SpecialNodeType::detect(&node), SpecialNodeType::DummySink);
    }

    #[test]
    fn test_special_node_regular() {
        let node = create_test_node(1, "Firefox");
        assert_eq!(SpecialNodeType::detect(&node), SpecialNodeType::Regular);
    }
}
