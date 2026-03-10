//! PipeWire event types.
//!
//! Defines events that flow from the PipeWire thread to the main thread.

use crate::domain::audio::VolumeControl;
use crate::domain::graph::{AudioFormat, LinkState, MediaClass, NodeLayer, PortDirection};
use crate::util::id::{ClientId, DeviceId, LinkId, NodeId, PortId};

/// Events from PipeWire to the application.
#[derive(Debug, Clone)]
pub enum PwEvent {
    // Connection state
    /// Connected to PipeWire
    Connected,
    /// Disconnected from PipeWire
    Disconnected,
    /// Reconnecting to PipeWire (attempt n of max)
    Reconnecting { attempt: u32, max_attempts: u32 },
    /// Connection error
    Error(PwError),

    // Node events
    /// A node was added
    NodeAdded(NodeInfo),
    /// A node was removed
    NodeRemoved(NodeId),

    // Port events
    /// A port was added
    PortAdded(PortInfo),
    /// A port was removed
    PortRemoved(PortId),

    // Link events
    /// A link was created
    LinkAdded(LinkInfo),
    /// A link was removed
    LinkRemoved(LinkId),

    // Audio parameter events
    /// Volume changed
    VolumeChanged(NodeId, VolumeControl),
    /// Mute state changed
    MuteChanged(NodeId, bool),
    /// Volume control failed (node_id, error_message)
    VolumeControlFailed(NodeId, String),

    // Meter events (high frequency)
    /// Batch meter update (variant matched but never constructed via this channel)
    #[allow(dead_code)]
    MeterUpdate(Vec<MeterUpdate>),

    // Client events
    /// A client was added (client info is logged but not used beyond that)
    #[allow(dead_code)]
    ClientAdded(ClientInfo),

    // Device events
    /// A device was added (device info is logged but not used beyond that)
    #[allow(dead_code)]
    DeviceAdded(DeviceInfo),
}

/// Error from PipeWire.
#[derive(Debug, Clone)]
pub struct PwError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
}

impl std::fmt::Display for PwError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PipeWire error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for PwError {}

/// Information about a PipeWire node.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node ID
    pub id: NodeId,
    /// Node name
    pub name: String,
    /// Client ID
    pub client_id: Option<ClientId>,
    /// Media class
    pub media_class: Option<MediaClass>,
    /// Application name
    pub application_name: Option<String>,
    /// Node description
    pub description: Option<String>,
    /// Node nickname
    pub nick: Option<String>,
    /// Audio format
    pub format: Option<AudioFormat>,
    /// Detected layer in the PipeWire stack
    pub layer: NodeLayer,
    /// Factory name (e.g., "api.alsa.pcm.source" for hardware)
    pub factory_name: Option<String>,
    /// Device ID (present for hardware nodes)
    pub device_id: Option<u32>,
    /// Object path (for hardware nodes)
    pub object_path: Option<String>,
    /// Link group (for PipeWire split/channel nodes)
    pub link_group: Option<String>,
    /// Client API (e.g., "pipewire-pulse" for session layer)
    pub client_api: Option<String>,
    /// Target object ID (reference to another node, for session layer)
    pub target_object: Option<u32>,
}

impl NodeInfo {
    /// Creates a new NodeInfo from raw properties.
    pub fn from_properties(id: u32, props: &std::collections::HashMap<String, String>) -> Self {
        let media_class = props
            .get("media.class")
            .map(|s| MediaClass::from_pipewire_str(s));

        let format = Self::parse_format(props);

        // Extract layer detection properties
        let factory_name = props.get("factory.name").cloned();
        let device_id = props.get("device.id").and_then(|s| s.parse().ok());
        let object_path = props.get("object.path").cloned();
        let link_group = props.get("node.link-group").cloned();
        let client_api = props.get("client.api").cloned();
        let target_object = props.get("target.object").and_then(|s| s.parse().ok());

        // Detect the layer based on properties
        let layer = Self::detect_layer(
            &media_class,
            factory_name.as_deref(),
            device_id,
            link_group.as_deref(),
            client_api.as_deref(),
            target_object,
            props,
        );

        Self {
            id: NodeId::new(id),
            name: props
                .get("node.name")
                .cloned()
                .unwrap_or_else(|| format!("Node {}", id)),
            client_id: props
                .get("client.id")
                .and_then(|s| s.parse().ok())
                .map(ClientId::new),
            media_class,
            application_name: props.get("application.name").cloned(),
            description: props.get("node.description").cloned(),
            nick: props.get("node.nick").cloned(),
            format,
            layer,
            factory_name,
            device_id,
            object_path,
            link_group,
            client_api,
            target_object,
        }
    }

    /// Detects the PipeWire stack layer for a node based on its properties.
    ///
    /// Layer detection heuristics:
    /// - **Hardware**: Has `device.id`, or factory name starts with `api.alsa`, `api.v4l2`, etc.
    /// - **PipeWire**: Has `node.link-group` (split nodes), or is an adapter/converter without device.id
    /// - **Session**: Created by session manager (WirePlumber), has `target.object`, or `client.api`
    fn detect_layer(
        media_class: &Option<MediaClass>,
        factory_name: Option<&str>,
        device_id: Option<u32>,
        link_group: Option<&str>,
        client_api: Option<&str>,
        target_object: Option<u32>,
        props: &std::collections::HashMap<String, String>,
    ) -> NodeLayer {
        // Hardware layer indicators:
        // - Has device.id (directly backed by a device)
        // - Factory name indicates kernel driver (api.alsa.*, api.v4l2.*, etc.)
        if device_id.is_some() {
            return NodeLayer::Hardware;
        }

        if let Some(factory) = factory_name {
            // Hardware driver factories
            if factory.starts_with("api.alsa")
                || factory.starts_with("api.v4l2")
                || factory.starts_with("api.bluez")
                || factory.starts_with("api.jack")
                || factory == "adapter"
            {
                // Adapters with device.id are hardware, without are PipeWire layer
                // (we already checked device_id above, so this is PipeWire layer)
                if factory == "adapter" {
                    return NodeLayer::Pipewire;
                }
                return NodeLayer::Hardware;
            }
        }

        // PipeWire layer indicators:
        // - Has node.link-group (split/channel nodes)
        // - Is a stream node without target.object or client.api (internal routing)
        if link_group.is_some() {
            return NodeLayer::Pipewire;
        }

        // Check for PipeWire-internal nodes
        if let Some(factory) = factory_name {
            if factory == "spa-node-factory"
                || factory == "audio.convert"
                || factory == "link-factory"
                || factory.starts_with("support.")
            {
                return NodeLayer::Pipewire;
            }
        }

        // Check for monitor.passthrough property (PipeWire internal)
        if props.get("monitor.passthrough").is_some() {
            return NodeLayer::Pipewire;
        }

        // Session layer indicators:
        // - Has target.object (routed by session manager)
        // - Has client.api (client connection type, e.g., "pipewire-pulse")
        // - Application streams (Stream/* media class)
        if target_object.is_some() || client_api.is_some() {
            return NodeLayer::Session;
        }

        // Stream nodes are typically session layer
        if let Some(mc) = media_class {
            match mc {
                MediaClass::StreamOutputAudio | MediaClass::StreamInputAudio => {
                    return NodeLayer::Session;
                }
                // Device nodes without device.id might be virtual devices (session layer)
                MediaClass::AudioDevice | MediaClass::VideoDevice => {
                    return NodeLayer::Session;
                }
                _ => {}
            }
        }

        // Default to session layer for application-facing nodes
        NodeLayer::Session
    }

    fn parse_format(props: &std::collections::HashMap<String, String>) -> Option<AudioFormat> {
        let sample_rate = props
            .get("audio.rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(48000);
        let channels = props
            .get("audio.channels")
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        let format = props
            .get("audio.format")
            .cloned()
            .unwrap_or_else(|| "F32LE".to_string());

        Some(AudioFormat {
            sample_rate,
            channels,
            format,
        })
    }

}

/// Information about a PipeWire port.
#[derive(Debug, Clone)]
pub struct PortInfo {
    /// Port ID
    pub id: PortId,
    /// Node ID
    pub node_id: NodeId,
    /// Port name
    pub name: String,
    /// Port direction
    pub direction: PortDirection,
    /// Channel index
    pub channel: Option<u32>,
    /// Physical port path
    pub physical_path: Option<String>,
    /// Alias name
    pub alias: Option<String>,
    /// Is this a monitor port
    pub is_monitor: bool,
    /// Is this a control port
    pub is_control: bool,
}

impl PortInfo {
    /// Creates PortInfo from raw properties.
    pub fn from_properties(id: u32, props: &std::collections::HashMap<String, String>) -> Self {
        let direction = props
            .get("port.direction")
            .and_then(|s| PortDirection::from_pw_str(s))
            .unwrap_or(PortDirection::Output);

        Self {
            id: PortId::new(id),
            node_id: NodeId::new(
                props
                    .get("node.id")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            ),
            name: props
                .get("port.name")
                .cloned()
                .unwrap_or_else(|| format!("Port {}", id)),
            direction,
            channel: props.get("audio.channel").and_then(|s| s.parse().ok()),
            physical_path: props.get("port.physical").cloned(),
            alias: props.get("port.alias").cloned(),
            is_monitor: props
                .get("port.monitor")
                .map(|s| s == "true")
                .unwrap_or(false),
            is_control: props
                .get("port.control")
                .map(|s| s == "true")
                .unwrap_or(false),
        }
    }

}

/// Information about a PipeWire link.
#[derive(Debug, Clone)]
pub struct LinkInfo {
    /// Link ID
    pub id: LinkId,
    /// Output port ID
    pub output_port: PortId,
    /// Input port ID
    pub input_port: PortId,
    /// Output node ID
    pub output_node: NodeId,
    /// Input node ID
    pub input_node: NodeId,
    /// Link state
    pub state: LinkState,
    /// Is the link active
    pub active: bool,
}

impl LinkInfo {
    /// Creates LinkInfo from raw properties.
    pub fn from_properties(id: u32, props: &std::collections::HashMap<String, String>) -> Self {
        Self {
            id: LinkId::new(id),
            output_port: PortId::new(
                props
                    .get("link.output.port")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            ),
            input_port: PortId::new(
                props
                    .get("link.input.port")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            ),
            output_node: NodeId::new(
                props
                    .get("link.output.node")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            ),
            input_node: NodeId::new(
                props
                    .get("link.input.node")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            ),
            state: LinkState::Init,
            active: true,
        }
    }
}

/// Meter update for a single node.
#[derive(Debug, Clone)]
pub struct MeterUpdate {
    /// Node ID
    pub node_id: NodeId,
    /// Peak levels per channel
    pub peak: Vec<f32>,
    /// RMS levels per channel
    pub rms: Vec<f32>,
}

/// Information about a PipeWire client.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClientInfo {
    /// Client ID
    pub id: ClientId,
    /// Client name
    pub name: String,
    /// Application name
    pub application_name: Option<String>,
    /// Process ID
    pub pid: Option<u32>,
}

impl ClientInfo {
    /// Creates ClientInfo from raw properties.
    pub fn from_properties(id: u32, props: &std::collections::HashMap<String, String>) -> Self {
        Self {
            id: ClientId::new(id),
            name: props
                .get("client.name")
                .cloned()
                .unwrap_or_else(|| format!("Client {}", id)),
            application_name: props.get("application.name").cloned(),
            pid: props.get("application.process.id").and_then(|s| s.parse().ok()),
        }
    }
}

/// Information about a PipeWire device.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DeviceInfo {
    /// Device ID
    pub id: DeviceId,
    /// Device name
    pub name: String,
    /// Device description
    pub description: Option<String>,
    /// Device nick
    pub nick: Option<String>,
}

impl DeviceInfo {
    /// Creates DeviceInfo from raw properties.
    pub fn from_properties(id: u32, props: &std::collections::HashMap<String, String>) -> Self {
        Self {
            id: DeviceId::new(id),
            name: props
                .get("device.name")
                .cloned()
                .unwrap_or_else(|| format!("Device {}", id)),
            description: props.get("device.description").cloned(),
            nick: props.get("device.nick").cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_node_info_from_properties() {
        let mut props = HashMap::new();
        props.insert("node.name".to_string(), "My Node".to_string());
        props.insert("media.class".to_string(), "Audio/Sink".to_string());
        props.insert("node.description".to_string(), "My Description".to_string());
        props.insert("application.name".to_string(), "My App".to_string());

        let info = NodeInfo::from_properties(42, &props);

        assert_eq!(info.id.raw(), 42);
        assert_eq!(info.name, "My Node");
        assert_eq!(info.media_class, Some(MediaClass::AudioSink));
        assert_eq!(info.description, Some("My Description".to_string()));
    }

    #[test]
    fn test_port_info_from_properties() {
        let mut props = HashMap::new();
        props.insert("port.name".to_string(), "output_FL".to_string());
        props.insert("port.direction".to_string(), "out".to_string());
        props.insert("node.id".to_string(), "10".to_string());
        props.insert("audio.channel".to_string(), "0".to_string());

        let info = PortInfo::from_properties(100, &props);

        assert_eq!(info.id.raw(), 100);
        assert_eq!(info.node_id.raw(), 10);
        assert_eq!(info.name, "output_FL");
        assert_eq!(info.direction, PortDirection::Output);
        assert_eq!(info.channel, Some(0));
    }

    #[test]
    fn test_link_info_from_properties() {
        let mut props = HashMap::new();
        props.insert("link.output.port".to_string(), "10".to_string());
        props.insert("link.input.port".to_string(), "20".to_string());
        props.insert("link.output.node".to_string(), "1".to_string());
        props.insert("link.input.node".to_string(), "2".to_string());

        let info = LinkInfo::from_properties(200, &props);

        assert_eq!(info.id.raw(), 200);
        assert_eq!(info.output_port.raw(), 10);
        assert_eq!(info.input_port.raw(), 20);
        assert_eq!(info.output_node.raw(), 1);
        assert_eq!(info.input_node.raw(), 2);
    }
}
