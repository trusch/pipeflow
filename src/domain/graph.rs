//! Core graph data model.
//!
//! Defines the fundamental structures for representing PipeWire graphs:
//! nodes, ports, and links.

use crate::util::id::{ClientId, NodeId, PortId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classification of nodes into PipeWire stack layers.
///
/// PipeWire exposes nodes at three conceptual layers:
/// - **Hardware**: Physical device nodes backed by kernel drivers (ALSA, V4L2, etc.)
/// - **Pipewire**: Logical views created by PipeWire itself (channel splits, adapters)
/// - **Session**: Application-facing nodes managed by the session manager (WirePlumber)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum NodeLayer {
    /// Hardware/device layer - physical devices from kernel (ALSA PCM, etc.)
    Hardware,
    /// PipeWire layer - logical nodes like channel splits, adapters
    Pipewire,
    /// Session layer - WirePlumber-managed application nodes
    #[default]
    Session,
}

impl NodeLayer {
    /// Returns a human-readable name for the layer.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Hardware => "Hardware",
            Self::Pipewire => "PipeWire",
            Self::Session => "Session",
        }
    }

    /// Returns a short label for UI display.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Hardware => "HW",
            Self::Pipewire => "PW",
            Self::Session => "SM",
        }
    }

    /// Returns a description of what this layer represents.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Hardware => "Physical device nodes from kernel (ALSA, etc.)",
            Self::Pipewire => "Logical views created by PipeWire (splits, adapters)",
            Self::Session => "Application nodes managed by session manager",
        }
    }
}

/// Media class of a PipeWire node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaClass {
    /// Audio source (e.g., microphone)
    AudioSource,
    /// Audio sink (e.g., speakers)
    AudioSink,
    /// Audio stream from an application
    StreamOutputAudio,
    /// Audio stream to an application
    StreamInputAudio,
    /// Video source (e.g., camera)
    VideoSource,
    /// Video sink (e.g., display)
    VideoSink,
    /// MIDI source
    MidiSource,
    /// MIDI sink
    MidiSink,
    /// Audio/Video source
    AudioVideoSource,
    /// Audio device
    AudioDevice,
    /// Video device
    VideoDevice,
    /// Unknown or other media class
    Other(String),
}

impl MediaClass {
    /// Parses a media class from a PipeWire string.
    pub fn from_pipewire_str(s: &str) -> Self {
        match s {
            "Audio/Source" => Self::AudioSource,
            "Audio/Sink" => Self::AudioSink,
            "Stream/Output/Audio" => Self::StreamOutputAudio,
            "Stream/Input/Audio" => Self::StreamInputAudio,
            "Video/Source" => Self::VideoSource,
            "Video/Sink" => Self::VideoSink,
            "Midi/Source" | "Midi/Bridge" => Self::MidiSource,
            "Midi/Sink" => Self::MidiSink,
            "Audio/Video/Source" => Self::AudioVideoSource,
            "Audio/Device" => Self::AudioDevice,
            "Video/Device" => Self::VideoDevice,
            other => Self::Other(other.to_string()),
        }
    }

    /// Returns true if this is an audio-related media class.
    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::AudioSource
                | Self::AudioSink
                | Self::StreamOutputAudio
                | Self::StreamInputAudio
                | Self::AudioDevice
                | Self::AudioVideoSource
        )
    }

    /// Returns true if this is a video-related media class.
    pub fn is_video(&self) -> bool {
        matches!(
            self,
            Self::VideoSource | Self::VideoSink | Self::VideoDevice | Self::AudioVideoSource
        )
    }

    /// Returns true if this is a MIDI-related media class.
    pub fn is_midi(&self) -> bool {
        matches!(self, Self::MidiSource | Self::MidiSink)
    }

    /// Returns true if this is a source (produces output, should be on the left).
    /// Sources include: AudioSource, VideoSource, MidiSource, StreamOutputAudio (app outputs audio)
    pub fn is_source(&self) -> bool {
        matches!(
            self,
            Self::AudioSource
                | Self::VideoSource
                | Self::MidiSource
                | Self::StreamOutputAudio
                | Self::AudioVideoSource
        )
    }

    /// Returns true if this is a sink (receives input, should be on the right).
    /// Sinks include: AudioSink, VideoSink, MidiSink, StreamInputAudio (app receives audio)
    pub fn is_sink(&self) -> bool {
        matches!(
            self,
            Self::AudioSink | Self::VideoSink | Self::MidiSink | Self::StreamInputAudio
        )
    }

    /// Returns a layout column hint: -1 for sources (left), 1 for sinks (right), 0 for middle.
    pub fn layout_column(&self) -> i32 {
        if self.is_source() {
            -1
        } else if self.is_sink() {
            1
        } else {
            0
        }
    }

    /// Returns a human-readable name for the media class.
    pub fn display_name(&self) -> &str {
        match self {
            Self::AudioSource => "Audio Source",
            Self::AudioSink => "Audio Sink",
            Self::StreamOutputAudio => "Audio Output",
            Self::StreamInputAudio => "Audio Input",
            Self::VideoSource => "Video Source",
            Self::VideoSink => "Video Sink",
            Self::MidiSource => "MIDI Source",
            Self::MidiSink => "MIDI Sink",
            Self::AudioVideoSource => "A/V Source",
            Self::AudioDevice => "Audio Device",
            Self::VideoDevice => "Video Device",
            Self::Other(_) => "Other",
        }
    }
}

impl std::str::FromStr for MediaClass {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_pipewire_str(s))
    }
}

/// Direction of a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    /// Input port (receives data)
    Input,
    /// Output port (sends data)
    Output,
}

impl PortDirection {
    /// Parses a port direction from a PipeWire string.
    pub fn from_pw_str(s: &str) -> Option<Self> {
        match s {
            "in" => Some(Self::Input),
            "out" => Some(Self::Output),
            _ => None,
        }
    }

    /// Returns the opposite direction.
    #[allow(dead_code)]
    pub fn opposite(&self) -> Self {
        match self {
            Self::Input => Self::Output,
            Self::Output => Self::Input,
        }
    }
}

/// Audio format information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u32,
    /// Format name (e.g., "F32LE")
    pub format: String,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            format: "F32LE".to_string(),
        }
    }
}

/// A port on a PipeWire node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    /// Unique port ID
    pub id: PortId,
    /// ID of the node this port belongs to
    pub node_id: NodeId,
    /// Port name
    pub name: String,
    /// Port direction (input/output)
    pub direction: PortDirection,
    /// Channel index for multi-channel ports
    pub channel: Option<u32>,
    /// Physical port path (if applicable)
    pub physical_path: Option<String>,
    /// Alias name (user-friendly name)
    pub alias: Option<String>,
    /// Whether this port is a monitor port
    pub is_monitor: bool,
    /// Whether this port is a control port
    pub is_control: bool,
}

impl Port {
    /// Creates a new port with the given parameters.
    #[allow(dead_code)]
    pub fn new(id: PortId, node_id: NodeId, name: String, direction: PortDirection) -> Self {
        Self {
            id,
            node_id,
            name,
            direction,
            channel: None,
            physical_path: None,
            alias: None,
            is_monitor: false,
            is_control: false,
        }
    }

    fn compact_port_name(name: &str) -> &str {
        name.rsplit(':').next().unwrap_or(name)
    }

    /// Returns the UI display name for this port.
    ///
    /// PipeWire often prefixes the actual port name with a node/device path
    /// (`node.name:playback_FL`). For visual display we only want the final
    /// port segment.
    pub fn display_name(&self) -> &str {
        let raw = self.alias.as_deref().unwrap_or(&self.name);
        Self::compact_port_name(raw)
    }

    /// Returns the full raw port name (or alias if present) for tooltips/details.
    pub fn full_display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Checks if this port can connect to another port.
    pub fn can_connect_to(&self, other: &Port) -> bool {
        // Ports must have opposite directions (input to output or vice versa)
        // Self-links (same node) are allowed for nodes like MIDI bridges
        self.direction != other.direction
    }
}

/// A PipeWire node (represents an audio source, sink, or application).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Unique node ID
    pub id: NodeId,
    /// Node name
    pub name: String,
    /// Client ID that owns this node
    pub client_id: Option<ClientId>,
    /// Media class
    pub media_class: Option<MediaClass>,
    /// Application name
    pub application_name: Option<String>,
    /// Node description
    pub description: Option<String>,
    /// Node nickname
    pub nick: Option<String>,
    /// Audio format (if applicable)
    pub format: Option<AudioFormat>,
    /// IDs of ports belonging to this node
    pub port_ids: Vec<PortId>,
    /// Whether this node is currently active
    pub is_active: bool,
    /// The PipeWire stack layer this node belongs to
    pub layer: NodeLayer,
}

impl Node {
    /// Creates a new node with the given parameters.
    #[allow(dead_code)]
    pub fn new(id: NodeId, name: String) -> Self {
        Self {
            id,
            name,
            client_id: None,
            media_class: None,
            application_name: None,
            description: None,
            nick: None,
            format: None,
            port_ids: Vec::new(),
            is_active: true,
            layer: NodeLayer::default(),
        }
    }

    /// Returns the display name for this node.
    pub fn display_name(&self) -> &str {
        self.description.as_deref().unwrap_or(&self.name)
    }

    /// Returns input ports for this node.
    #[allow(dead_code)]
    pub fn input_ports<'a>(&'a self, ports: &'a HashMap<PortId, Port>) -> Vec<&'a Port> {
        self.port_ids
            .iter()
            .filter_map(|id| ports.get(id))
            .filter(|p| p.direction == PortDirection::Input)
            .collect()
    }

    /// Returns output ports for this node.
    #[allow(dead_code)]
    pub fn output_ports<'a>(&'a self, ports: &'a HashMap<PortId, Port>) -> Vec<&'a Port> {
        self.port_ids
            .iter()
            .filter_map(|id| ports.get(id))
            .filter(|p| p.direction == PortDirection::Output)
            .collect()
    }
}

/// A link between two ports in the PipeWire graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Unique link ID
    pub id: crate::util::id::LinkId,
    /// Output port ID
    pub output_port: PortId,
    /// Input port ID
    pub input_port: PortId,
    /// Output node ID
    pub output_node: NodeId,
    /// Input node ID
    pub input_node: NodeId,
    /// Whether the link is currently active
    pub is_active: bool,
    /// Link state
    pub state: LinkState,
}

/// State of a link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkState {
    /// Link is being set up
    #[default]
    Init,
    /// Link is being negotiated
    Negotiating,
    /// Link is allocating buffers
    Allocating,
    /// Link is paused
    Paused,
    /// Link is active
    Active,
    /// Link encountered an error
    Error,
    /// Link is unlinked
    Unlinked,
}

impl LinkState {
    /// Returns true if the link is in a healthy state.
    #[allow(dead_code)]
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }

    /// Returns a human-readable name for the state.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Init => "Initializing",
            Self::Negotiating => "Negotiating",
            Self::Allocating => "Allocating",
            Self::Paused => "Paused",
            Self::Active => "Active",
            Self::Error => "Error",
            Self::Unlinked => "Unlinked",
        }
    }
}

impl Link {
    /// Creates a new link between two ports.
    #[allow(dead_code)]
    pub fn new(
        id: crate::util::id::LinkId,
        output_port: PortId,
        input_port: PortId,
        output_node: NodeId,
        input_node: NodeId,
    ) -> Self {
        Self {
            id,
            output_port,
            input_port,
            output_node,
            input_node,
            is_active: true,
            state: LinkState::Init,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_class_parsing() {
        // Test using the FromStr trait via .parse()
        assert_eq!(
            "Audio/Source".parse::<MediaClass>().unwrap(),
            MediaClass::AudioSource
        );
        assert_eq!(
            "Audio/Sink".parse::<MediaClass>().unwrap(),
            MediaClass::AudioSink
        );
        assert_eq!(
            "Stream/Output/Audio".parse::<MediaClass>().unwrap(),
            MediaClass::StreamOutputAudio
        );
        assert!(matches!(
            "Custom/Class".parse::<MediaClass>().unwrap(),
            MediaClass::Other(_)
        ));

        // Test using from_pipewire_str directly
        assert_eq!(
            MediaClass::from_pipewire_str("Audio/Source"),
            MediaClass::AudioSource
        );
    }

    #[test]
    fn test_media_class_categories() {
        assert!(MediaClass::AudioSource.is_audio());
        assert!(!MediaClass::AudioSource.is_video());
        assert!(MediaClass::VideoSource.is_video());
        assert!(MediaClass::MidiSource.is_midi());
    }

    #[test]
    fn test_port_direction() {
        let input = PortDirection::from_pw_str("in").unwrap();
        let output = PortDirection::from_pw_str("out").unwrap();

        assert_eq!(input, PortDirection::Input);
        assert_eq!(output, PortDirection::Output);
        assert_eq!(input.opposite(), PortDirection::Output);
        assert_eq!(output.opposite(), PortDirection::Input);
    }

    #[test]
    fn test_port_can_connect() {
        let port1 = Port::new(
            PortId::new(1),
            NodeId::new(10),
            "out".to_string(),
            PortDirection::Output,
        );
        let port2 = Port::new(
            PortId::new(2),
            NodeId::new(20),
            "in".to_string(),
            PortDirection::Input,
        );
        let port3 = Port::new(
            PortId::new(3),
            NodeId::new(10), // Same node as port1
            "in".to_string(),
            PortDirection::Input,
        );
        let port4 = Port::new(
            PortId::new(4),
            NodeId::new(30),
            "out".to_string(),
            PortDirection::Output,
        );

        assert!(port1.can_connect_to(&port2)); // Different nodes, opposite directions
        assert!(port1.can_connect_to(&port3)); // Same node, opposite directions (self-link allowed)
        assert!(!port1.can_connect_to(&port4)); // Same direction (not allowed)
    }

    #[test]
    fn test_node_display_name() {
        let mut node = Node::new(NodeId::new(1), "raw_name".to_string());
        assert_eq!(node.display_name(), "raw_name");

        // Description takes priority over name
        node.description = Some("Description".to_string());
        assert_eq!(node.display_name(), "Description");

        // Other fields don't affect display name
        node.application_name = Some("App Name".to_string());
        node.nick = Some("Nickname".to_string());
        assert_eq!(node.display_name(), "Description");
    }

    #[test]
    fn test_link_state_healthy() {
        assert!(LinkState::Active.is_healthy());
        assert!(LinkState::Paused.is_healthy());
        assert!(!LinkState::Error.is_healthy());
        assert!(!LinkState::Init.is_healthy());
    }

    #[test]
    fn test_media_class_layout_column() {
        assert_eq!(MediaClass::AudioSource.layout_column(), -1);
        assert_eq!(MediaClass::AudioSink.layout_column(), 1);
        assert_eq!(MediaClass::AudioDevice.layout_column(), 0);
        assert_eq!(MediaClass::StreamOutputAudio.layout_column(), -1);
        assert_eq!(MediaClass::StreamInputAudio.layout_column(), 1);
    }

    #[test]
    fn test_media_class_all_variants_have_display_name() {
        let classes = vec![
            MediaClass::AudioSource,
            MediaClass::AudioSink,
            MediaClass::StreamOutputAudio,
            MediaClass::StreamInputAudio,
            MediaClass::VideoSource,
            MediaClass::VideoSink,
            MediaClass::MidiSource,
            MediaClass::MidiSink,
            MediaClass::AudioVideoSource,
            MediaClass::AudioDevice,
            MediaClass::VideoDevice,
            MediaClass::Other("Custom".into()),
        ];
        for mc in &classes {
            assert!(!mc.display_name().is_empty());
        }
    }

    #[test]
    fn test_audio_video_source_is_both() {
        assert!(MediaClass::AudioVideoSource.is_audio());
        assert!(MediaClass::AudioVideoSource.is_video());
        assert!(!MediaClass::AudioVideoSource.is_midi());
    }

    #[test]
    fn test_node_layer_all_variants() {
        assert_eq!(NodeLayer::Hardware.display_name(), "Hardware");
        assert_eq!(NodeLayer::Pipewire.display_name(), "PipeWire");
        assert_eq!(NodeLayer::Session.display_name(), "Session");
        assert_eq!(NodeLayer::Hardware.short_label(), "HW");
        assert_eq!(NodeLayer::Pipewire.short_label(), "PW");
        assert_eq!(NodeLayer::Session.short_label(), "SM");
        for layer in &[NodeLayer::Hardware, NodeLayer::Pipewire, NodeLayer::Session] {
            assert!(!layer.description().is_empty());
        }
    }

    #[test]
    fn test_port_display_name_with_alias() {
        let mut port = Port::new(
            PortId::new(1),
            NodeId::new(1),
            "raw".to_string(),
            PortDirection::Input,
        );
        assert_eq!(port.display_name(), "raw");
        port.alias = Some("Friendly Name".to_string());
        assert_eq!(port.display_name(), "Friendly Name");
    }

    #[test]
    fn test_link_creation_and_state() {
        let link = Link::new(
            crate::util::id::LinkId::new(1),
            PortId::new(1),
            PortId::new(2),
            NodeId::new(10),
            NodeId::new(20),
        );
        assert!(link.is_active);
        assert_eq!(link.state, LinkState::Init);
        assert_eq!(link.state.display_name(), "Initializing");
    }

    #[test]
    fn test_link_state_display_names_unique() {
        let states = [
            LinkState::Init,
            LinkState::Negotiating,
            LinkState::Allocating,
            LinkState::Paused,
            LinkState::Active,
            LinkState::Error,
            LinkState::Unlinked,
        ];
        let names: std::collections::HashSet<_> = states.iter().map(|s| s.display_name()).collect();
        assert_eq!(
            names.len(),
            states.len(),
            "All link state display names should be unique"
        );
    }

    #[test]
    fn test_audio_format_default() {
        let fmt = AudioFormat::default();
        assert_eq!(fmt.sample_rate, 48000);
        assert_eq!(fmt.channels, 2);
        assert_eq!(fmt.format, "F32LE");
    }

    #[test]
    fn test_port_direction_invalid_input() {
        assert!(PortDirection::from_pw_str("invalid").is_none());
        assert!(PortDirection::from_pw_str("").is_none());
        assert!(PortDirection::from_pw_str("IN").is_none()); // Case-sensitive
    }

    #[test]
    fn test_node_input_output_ports() {
        let mut ports_map = HashMap::new();
        let p1 = Port::new(
            PortId::new(1),
            NodeId::new(10),
            "in_L".into(),
            PortDirection::Input,
        );
        let p2 = Port::new(
            PortId::new(2),
            NodeId::new(10),
            "in_R".into(),
            PortDirection::Input,
        );
        let p3 = Port::new(
            PortId::new(3),
            NodeId::new(10),
            "out_L".into(),
            PortDirection::Output,
        );
        ports_map.insert(p1.id, p1);
        ports_map.insert(p2.id, p2);
        ports_map.insert(p3.id, p3);

        let mut node = Node::new(NodeId::new(10), "test".into());
        node.port_ids = vec![PortId::new(1), PortId::new(2), PortId::new(3)];

        assert_eq!(node.input_ports(&ports_map).len(), 2);
        assert_eq!(node.output_ports(&ports_map).len(), 1);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_media_class_str() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("Audio/Source".to_string()),
            Just("Audio/Sink".to_string()),
            Just("Stream/Output/Audio".to_string()),
            Just("Stream/Input/Audio".to_string()),
            Just("Video/Source".to_string()),
            Just("Video/Sink".to_string()),
            Just("Midi/Source".to_string()),
            Just("Midi/Sink".to_string()),
            Just("Audio/Video/Source".to_string()),
            Just("Audio/Device".to_string()),
            Just("Video/Device".to_string()),
            "[a-zA-Z/]{1,30}",
        ]
    }

    proptest! {
        #[test]
        fn media_class_roundtrip_never_panics(s in arb_media_class_str()) {
            let mc = MediaClass::from_pipewire_str(&s);
            // display_name should never panic
            let _ = mc.display_name();
            let _ = mc.is_audio();
            let _ = mc.is_video();
            let _ = mc.is_midi();
            let _ = mc.layout_column();
        }

        #[test]
        fn port_can_connect_is_symmetric_for_same_direction(
            id1 in 1u32..1000,
            id2 in 1u32..1000,
        ) {
            let p1 = Port::new(PortId::new(id1), NodeId::new(1), "a".into(), PortDirection::Output);
            let p2 = Port::new(PortId::new(id2), NodeId::new(2), "b".into(), PortDirection::Output);
            // Same direction should never connect
            assert!(!p1.can_connect_to(&p2));
            assert!(!p2.can_connect_to(&p1));
        }

        #[test]
        fn port_can_connect_opposite_always_works(
            id1 in 1u32..1000,
            id2 in 1u32..1000,
        ) {
            let p1 = Port::new(PortId::new(id1), NodeId::new(1), "a".into(), PortDirection::Output);
            let p2 = Port::new(PortId::new(id2), NodeId::new(2), "b".into(), PortDirection::Input);
            assert!(p1.can_connect_to(&p2));
            assert!(p2.can_connect_to(&p1));
        }
    }
}
