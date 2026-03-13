//! Adapter for converting between domain types and protobuf messages.
//!
//! Provides bidirectional conversion between the internal Rust types
//! and the gRPC protocol buffer messages.

use crate::core::commands::AppCommand;
use crate::core::state::AppState;
use crate::domain::audio::VolumeControl;
use crate::domain::graph::{
    AudioFormat, Link, LinkState, MediaClass, Node, NodeLayer, Port, PortDirection,
};
use crate::domain::safety::{SafetyController, SafetyMode};
use crate::pipewire::events::{LinkInfo, MeterUpdate, NodeInfo, PortInfo, PwEvent};
use crate::util::id::{LinkId, NodeId, PortId};

use super::proto;

// ============================================================================
// Domain -> Proto conversions
// ============================================================================

impl From<&Node> for proto::Node {
    fn from(node: &Node) -> Self {
        proto::Node {
            id: node.id.raw(),
            name: node.name.clone(),
            client_id: node.client_id.map(|c| c.raw()),
            media_class: node.media_class.as_ref().map(media_class_to_string),
            application_name: node.application_name.clone(),
            description: node.description.clone(),
            nick: node.nick.clone(),
            format: node.format.as_ref().map(|f| f.into()),
            port_ids: node.port_ids.iter().map(|p| p.raw()).collect(),
            is_active: node.is_active,
            layer: node_layer_to_proto(&node.layer).into(),
            // Layer detection properties are not stored in Node, only in NodeInfo
            factory_name: None,
            device_id: None,
            object_path: None,
            link_group: None,
            client_api: None,
            target_object: None,
        }
    }
}

impl From<&Port> for proto::Port {
    fn from(port: &Port) -> Self {
        proto::Port {
            id: port.id.raw(),
            node_id: port.node_id.raw(),
            name: port.name.clone(),
            direction: match port.direction {
                PortDirection::Input => proto::PortDirection::Input.into(),
                PortDirection::Output => proto::PortDirection::Output.into(),
            },
            channel: port.channel,
            physical_path: port.physical_path.clone(),
            alias: port.alias.clone(),
            is_monitor: port.is_monitor,
            is_control: port.is_control,
        }
    }
}

impl From<&Link> for proto::Link {
    fn from(link: &Link) -> Self {
        proto::Link {
            id: link.id.raw(),
            output_port_id: link.output_port.raw(),
            input_port_id: link.input_port.raw(),
            output_node_id: link.output_node.raw(),
            input_node_id: link.input_node.raw(),
            is_active: link.is_active,
            state: link_state_to_proto(&link.state).into(),
        }
    }
}

impl From<&AudioFormat> for proto::AudioFormat {
    fn from(format: &AudioFormat) -> Self {
        proto::AudioFormat {
            sample_rate: format.sample_rate,
            channels: format.channels,
            format: format.format.clone(),
        }
    }
}

impl From<&VolumeControl> for proto::Volume {
    fn from(vol: &VolumeControl) -> Self {
        proto::Volume {
            master: vol.master,
            channels: vol.channels.clone(),
            muted: vol.muted,
        }
    }
}

impl From<&SafetyController> for proto::SafetyStatus {
    fn from(safety: &SafetyController) -> Self {
        proto::SafetyStatus {
            mode: safety_mode_to_proto(&safety.mode).into(),
            routing_locked: false, // Routing lock feature removed
            panic_active: false,   // Panic feature removed
        }
    }
}

// ============================================================================
// Proto -> Domain conversions
// ============================================================================

impl From<proto::SafetyMode> for SafetyMode {
    fn from(mode: proto::SafetyMode) -> Self {
        match mode {
            proto::SafetyMode::Normal => SafetyMode::Normal,
            proto::SafetyMode::ReadOnly => SafetyMode::ReadOnly,
            proto::SafetyMode::Stage => SafetyMode::Stage,
            proto::SafetyMode::Unspecified => SafetyMode::Normal,
        }
    }
}

// ============================================================================
// Command conversions
// ============================================================================

/// Converts a proto Command to an AppCommand.
///
/// Returns `None` for:
/// - Unknown commands
/// - UI-only commands (`SetSafetyMode`, `ToggleRoutingLock`) that are handled
///   directly by the gRPC server before reaching this function
pub fn command_from_proto(cmd: &proto::Command) -> Option<AppCommand> {
    cmd.command.as_ref().and_then(|c| match c {
        proto::command::Command::CreateLink(req) => Some(AppCommand::CreateLink {
            output_port: PortId::new(req.output_port_id),
            input_port: PortId::new(req.input_port_id),
        }),
        proto::command::Command::RemoveLink(req) => {
            Some(AppCommand::RemoveLink(LinkId::new(req.link_id)))
        }
        proto::command::Command::ToggleLink(req) => Some(AppCommand::ToggleLink {
            link_id: LinkId::new(req.link_id),
            active: req.active,
        }),
        proto::command::Command::SetVolume(req) => Some(AppCommand::SetVolume {
            node_id: NodeId::new(req.node_id),
            volume: VolumeControl {
                master: req.volume,
                channels: vec![req.volume, req.volume],
                muted: false,
                step: 0.05,
            },
        }),
        proto::command::Command::SetMute(req) => Some(AppCommand::SetMute {
            node_id: NodeId::new(req.node_id),
            muted: req.muted,
        }),
        proto::command::Command::SetChannelVolume(req) => Some(AppCommand::SetChannelVolume {
            node_id: NodeId::new(req.node_id),
            channel: req.channel as usize,
            volume: req.volume,
        }),
        // Panic commands are no longer supported
        proto::command::Command::PanicMute(_) | proto::command::Command::PanicRestore(_) => None,
        // These are UI-only commands handled directly by the gRPC server
        proto::command::Command::SetSafetyMode(_)
        | proto::command::Command::ToggleRoutingLock(_) => None,
    })
}

// ============================================================================
// Event conversions
// ============================================================================

/// Converts a PwEvent to a proto Event.
pub fn event_to_proto(event: &PwEvent, sequence: u64, timestamp_ms: u64) -> Option<proto::Event> {
    let event_payload = match event {
        PwEvent::Connected => Some(proto::event::Event::ConnectionStatus(
            proto::ConnectionStatus {
                state: proto::ConnectionState::Connected.into(),
                reconnect_attempt: 0,
                max_reconnect_attempts: 0,
                error_message: String::new(),
            },
        )),
        PwEvent::Disconnected => Some(proto::event::Event::ConnectionStatus(
            proto::ConnectionStatus {
                state: proto::ConnectionState::Disconnected.into(),
                reconnect_attempt: 0,
                max_reconnect_attempts: 0,
                error_message: String::new(),
            },
        )),
        PwEvent::Reconnecting {
            attempt,
            max_attempts,
        } => Some(proto::event::Event::ConnectionStatus(
            proto::ConnectionStatus {
                state: proto::ConnectionState::Reconnecting.into(),
                reconnect_attempt: *attempt,
                max_reconnect_attempts: *max_attempts,
                error_message: String::new(),
            },
        )),
        PwEvent::Error(err) => Some(proto::event::Event::ConnectionStatus(
            proto::ConnectionStatus {
                state: proto::ConnectionState::Error.into(),
                reconnect_attempt: 0,
                max_reconnect_attempts: 0,
                error_message: err.message.clone(),
            },
        )),
        PwEvent::NodeAdded(info) => Some(proto::event::Event::NodeAdded(proto::NodeAdded {
            node: Some(node_info_to_proto(info)),
        })),
        PwEvent::NodeRemoved(id) => Some(proto::event::Event::NodeRemoved(proto::NodeRemoved {
            node_id: id.raw(),
        })),
        PwEvent::PortAdded(info) => Some(proto::event::Event::PortAdded(proto::PortAdded {
            port: Some(port_info_to_proto(info)),
        })),
        PwEvent::PortRemoved(id) => Some(proto::event::Event::PortRemoved(proto::PortRemoved {
            port_id: id.raw(),
        })),
        PwEvent::LinkAdded(info) => Some(proto::event::Event::LinkAdded(proto::LinkAdded {
            link: Some(link_info_to_proto(info)),
        })),
        PwEvent::LinkRemoved(id) => Some(proto::event::Event::LinkRemoved(proto::LinkRemoved {
            link_id: id.raw(),
        })),
        PwEvent::VolumeChanged(node_id, volume) => {
            Some(proto::event::Event::VolumeChanged(proto::VolumeChanged {
                node_id: node_id.raw(),
                volume: Some(volume.into()),
            }))
        }
        PwEvent::MuteChanged(node_id, muted) => {
            Some(proto::event::Event::MuteChanged(proto::MuteChanged {
                node_id: node_id.raw(),
                muted: *muted,
            }))
        }
        // Meter updates are handled separately via streaming
        PwEvent::MeterUpdate(_) => None,
        // Client and device events are not exposed via network
        PwEvent::ClientAdded(_) | PwEvent::DeviceAdded(_) => None,
        // Volume control failures are local UI feedback, not network events
        PwEvent::VolumeControlFailed(_, _) => None,
    };

    event_payload.map(|e| proto::Event {
        sequence,
        timestamp_ms,
        event: Some(e),
    })
}

/// Converts meter updates to a proto MeterBatch.
pub fn meter_batch_to_proto(updates: &[MeterUpdate], timestamp_ms: u32) -> proto::MeterBatch {
    proto::MeterBatch {
        timestamp_ms,
        entries: updates
            .iter()
            .map(|u| proto::MeterEntry {
                node_id: u.node_id.raw(),
                peak: u.peak.clone(),
                rms: u.rms.clone(),
            })
            .collect(),
    }
}

// ============================================================================
// State snapshot
// ============================================================================

/// Creates a full graph state snapshot for initial sync.
pub fn state_to_proto(state: &AppState, sequence: u64) -> proto::GraphState {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    proto::GraphState {
        nodes: state.graph.nodes.values().map(|n| n.into()).collect(),
        ports: state.graph.ports.values().map(|p| p.into()).collect(),
        links: state.graph.links.values().map(|l| l.into()).collect(),
        volumes: state
            .graph
            .volumes
            .iter()
            .map(|(id, vol)| (id.raw(), vol.into()))
            .collect(),
        safety: Some((&state.safety).into()),
        connection: Some(connection_state_to_proto(&state.connection)),
        sequence,
        timestamp_ms,
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn media_class_to_string(mc: &MediaClass) -> String {
    match mc {
        MediaClass::AudioSource => "Audio/Source".to_string(),
        MediaClass::AudioSink => "Audio/Sink".to_string(),
        MediaClass::StreamOutputAudio => "Stream/Output/Audio".to_string(),
        MediaClass::StreamInputAudio => "Stream/Input/Audio".to_string(),
        MediaClass::VideoSource => "Video/Source".to_string(),
        MediaClass::VideoSink => "Video/Sink".to_string(),
        MediaClass::MidiSource => "Midi/Source".to_string(),
        MediaClass::MidiSink => "Midi/Sink".to_string(),
        MediaClass::AudioVideoSource => "Audio/Video/Source".to_string(),
        MediaClass::AudioDevice => "Audio/Device".to_string(),
        MediaClass::VideoDevice => "Video/Device".to_string(),
        MediaClass::Other(s) => s.clone(),
    }
}

fn link_state_to_proto(state: &LinkState) -> proto::LinkState {
    match state {
        LinkState::Init => proto::LinkState::Init,
        LinkState::Negotiating => proto::LinkState::Negotiating,
        LinkState::Allocating => proto::LinkState::Allocating,
        LinkState::Paused => proto::LinkState::Paused,
        LinkState::Active => proto::LinkState::Active,
        LinkState::Error => proto::LinkState::Error,
        LinkState::Unlinked => proto::LinkState::Unlinked,
    }
}

fn safety_mode_to_proto(mode: &SafetyMode) -> proto::SafetyMode {
    match mode {
        SafetyMode::Normal => proto::SafetyMode::Normal,
        SafetyMode::ReadOnly => proto::SafetyMode::ReadOnly,
        SafetyMode::Stage => proto::SafetyMode::Stage,
    }
}

fn node_info_to_proto(info: &NodeInfo) -> proto::Node {
    proto::Node {
        id: info.id.raw(),
        name: info.name.clone(),
        client_id: info.client_id.map(|c| c.raw()),
        media_class: info.media_class.as_ref().map(media_class_to_string),
        application_name: info.application_name.clone(),
        description: info.description.clone(),
        nick: info.nick.clone(),
        format: info.format.as_ref().map(|f| f.into()),
        port_ids: vec![],
        is_active: true,
        layer: node_layer_to_proto(&info.layer).into(),
        factory_name: info.factory_name.clone(),
        device_id: info.device_id,
        object_path: info.object_path.clone(),
        link_group: info.link_group.clone(),
        client_api: info.client_api.clone(),
        target_object: info.target_object,
    }
}

fn node_layer_to_proto(layer: &NodeLayer) -> proto::NodeLayer {
    match layer {
        NodeLayer::Hardware => proto::NodeLayer::Hardware,
        NodeLayer::Pipewire => proto::NodeLayer::Pipewire,
        NodeLayer::Session => proto::NodeLayer::Session,
    }
}

fn port_info_to_proto(info: &PortInfo) -> proto::Port {
    proto::Port {
        id: info.id.raw(),
        node_id: info.node_id.raw(),
        name: info.name.clone(),
        direction: match info.direction {
            PortDirection::Input => proto::PortDirection::Input.into(),
            PortDirection::Output => proto::PortDirection::Output.into(),
        },
        channel: info.channel,
        physical_path: info.physical_path.clone(),
        alias: info.alias.clone(),
        is_monitor: info.is_monitor,
        is_control: info.is_control,
    }
}

fn link_info_to_proto(info: &LinkInfo) -> proto::Link {
    proto::Link {
        id: info.id.raw(),
        output_port_id: info.output_port.raw(),
        input_port_id: info.input_port.raw(),
        output_node_id: info.output_node.raw(),
        input_node_id: info.input_node.raw(),
        is_active: info.active,
        state: link_state_to_proto(&info.state).into(),
    }
}

fn connection_state_to_proto(
    state: &crate::core::state::ConnectionState,
) -> proto::ConnectionStatus {
    use crate::core::state::ConnectionState;

    let proto_state = match state {
        ConnectionState::Disconnected => proto::ConnectionState::Disconnected,
        ConnectionState::Connecting => proto::ConnectionState::Reconnecting,
        ConnectionState::Connected => proto::ConnectionState::Connected,
        ConnectionState::Error => proto::ConnectionState::Error,
    };

    proto::ConnectionStatus {
        state: proto_state.into(),
        reconnect_attempt: 0,
        max_reconnect_attempts: 0,
        error_message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::id::ClientId;

    // ========================================================================
    // Node conversion tests
    // ========================================================================

    #[test]
    fn test_node_to_proto() {
        let node = Node {
            id: NodeId::new(42),
            name: "Test Node".to_string(),
            client_id: Some(ClientId::new(10)),
            media_class: Some(MediaClass::AudioSink),
            application_name: Some("TestApp".to_string()),
            description: Some("A test node".to_string()),
            nick: Some("test".to_string()),
            format: Some(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                format: "F32LE".to_string(),
            }),
            port_ids: vec![PortId::new(1), PortId::new(2)],
            is_active: true,
            layer: NodeLayer::Session,
        };

        let proto_node: proto::Node = (&node).into();

        assert_eq!(proto_node.id, 42);
        assert_eq!(proto_node.name, "Test Node");
        assert_eq!(proto_node.client_id, Some(10));
        assert_eq!(proto_node.media_class, Some("Audio/Sink".to_string()));
        assert_eq!(proto_node.application_name, Some("TestApp".to_string()));
        assert_eq!(proto_node.description, Some("A test node".to_string()));
        assert_eq!(proto_node.nick, Some("test".to_string()));
        assert!(proto_node.format.is_some());
        let fmt = proto_node.format.unwrap();
        assert_eq!(fmt.sample_rate, 48000);
        assert_eq!(fmt.channels, 2);
        assert_eq!(fmt.format, "F32LE");
        assert_eq!(proto_node.port_ids, vec![1, 2]);
        assert!(proto_node.is_active);
        assert_eq!(proto_node.layer, proto::NodeLayer::Session as i32);
    }

    #[test]
    fn test_node_to_proto_minimal() {
        let node = Node {
            id: NodeId::new(1),
            name: "Minimal".to_string(),
            client_id: None,
            media_class: None,
            application_name: None,
            description: None,
            nick: None,
            format: None,
            port_ids: vec![],
            is_active: false,
            layer: NodeLayer::Hardware,
        };

        let proto_node: proto::Node = (&node).into();

        assert_eq!(proto_node.id, 1);
        assert_eq!(proto_node.name, "Minimal");
        assert_eq!(proto_node.client_id, None);
        assert_eq!(proto_node.media_class, None);
        assert!(proto_node.port_ids.is_empty());
        assert!(!proto_node.is_active);
        assert_eq!(proto_node.layer, proto::NodeLayer::Hardware as i32);
    }

    // ========================================================================
    // Port conversion tests
    // ========================================================================

    #[test]
    fn test_port_to_proto_input() {
        let port = Port {
            id: PortId::new(100),
            node_id: NodeId::new(42),
            name: "input_FL".to_string(),
            direction: PortDirection::Input,
            channel: Some(0),
            physical_path: Some("/dev/snd/pcm0".to_string()),
            alias: Some("Front Left".to_string()),
            is_monitor: false,
            is_control: false,
        };

        let proto_port: proto::Port = (&port).into();

        assert_eq!(proto_port.id, 100);
        assert_eq!(proto_port.node_id, 42);
        assert_eq!(proto_port.name, "input_FL");
        assert_eq!(proto_port.direction, proto::PortDirection::Input as i32);
        assert_eq!(proto_port.channel, Some(0));
        assert_eq!(proto_port.physical_path, Some("/dev/snd/pcm0".to_string()));
        assert_eq!(proto_port.alias, Some("Front Left".to_string()));
        assert!(!proto_port.is_monitor);
        assert!(!proto_port.is_control);
    }

    #[test]
    fn test_port_to_proto_output() {
        let port = Port {
            id: PortId::new(200),
            node_id: NodeId::new(50),
            name: "output_FR".to_string(),
            direction: PortDirection::Output,
            channel: Some(1),
            physical_path: None,
            alias: None,
            is_monitor: true,
            is_control: false,
        };

        let proto_port: proto::Port = (&port).into();

        assert_eq!(proto_port.direction, proto::PortDirection::Output as i32);
        assert!(proto_port.is_monitor);
    }

    // ========================================================================
    // Link conversion tests
    // ========================================================================

    #[test]
    fn test_link_to_proto() {
        let link = Link {
            id: LinkId::new(500),
            output_port: PortId::new(100),
            input_port: PortId::new(200),
            output_node: NodeId::new(10),
            input_node: NodeId::new(20),
            is_active: true,
            state: LinkState::Active,
        };

        let proto_link: proto::Link = (&link).into();

        assert_eq!(proto_link.id, 500);
        assert_eq!(proto_link.output_port_id, 100);
        assert_eq!(proto_link.input_port_id, 200);
        assert_eq!(proto_link.output_node_id, 10);
        assert_eq!(proto_link.input_node_id, 20);
        assert!(proto_link.is_active);
        assert_eq!(proto_link.state, proto::LinkState::Active as i32);
    }

    #[test]
    fn test_link_states() {
        let states = [
            (LinkState::Init, proto::LinkState::Init),
            (LinkState::Negotiating, proto::LinkState::Negotiating),
            (LinkState::Allocating, proto::LinkState::Allocating),
            (LinkState::Paused, proto::LinkState::Paused),
            (LinkState::Active, proto::LinkState::Active),
            (LinkState::Error, proto::LinkState::Error),
            (LinkState::Unlinked, proto::LinkState::Unlinked),
        ];

        for (domain_state, expected_proto) in states {
            let link = Link {
                id: LinkId::new(1),
                output_port: PortId::new(1),
                input_port: PortId::new(2),
                output_node: NodeId::new(1),
                input_node: NodeId::new(2),
                is_active: false,
                state: domain_state,
            };

            let proto_link: proto::Link = (&link).into();
            assert_eq!(proto_link.state, expected_proto as i32);
        }
    }

    // ========================================================================
    // Volume conversion tests
    // ========================================================================

    #[test]
    fn test_volume_to_proto() {
        let volume = VolumeControl {
            master: 0.75,
            channels: vec![0.8, 0.7],
            muted: false,
            step: 0.05,
        };

        let proto_vol: proto::Volume = (&volume).into();

        assert!((proto_vol.master - 0.75).abs() < f32::EPSILON);
        assert_eq!(proto_vol.channels.len(), 2);
        assert!((proto_vol.channels[0] - 0.8).abs() < f32::EPSILON);
        assert!((proto_vol.channels[1] - 0.7).abs() < f32::EPSILON);
        assert!(!proto_vol.muted);
    }

    #[test]
    fn test_volume_muted() {
        let volume = VolumeControl {
            master: 0.5,
            channels: vec![],
            muted: true,
            step: 0.05,
        };

        let proto_vol: proto::Volume = (&volume).into();
        assert!(proto_vol.muted);
    }

    // ========================================================================
    // Safety conversion tests
    // ========================================================================

    #[test]
    fn test_safety_controller_to_proto() {
        let mut safety = SafetyController::default();
        safety.set_mode(SafetyMode::ReadOnly);

        let proto_safety: proto::SafetyStatus = (&safety).into();

        assert_eq!(proto_safety.mode, proto::SafetyMode::ReadOnly as i32);
        assert!(!proto_safety.routing_locked); // Routing lock feature removed
        assert!(!proto_safety.panic_active);
    }

    #[test]
    fn test_safety_mode_from_proto() {
        assert_eq!(
            SafetyMode::from(proto::SafetyMode::Normal),
            SafetyMode::Normal
        );
        assert_eq!(
            SafetyMode::from(proto::SafetyMode::ReadOnly),
            SafetyMode::ReadOnly
        );
        assert_eq!(
            SafetyMode::from(proto::SafetyMode::Stage),
            SafetyMode::Stage
        );
        assert_eq!(
            SafetyMode::from(proto::SafetyMode::Unspecified),
            SafetyMode::Normal
        );
    }

    // ========================================================================
    // Command conversion tests
    // ========================================================================

    #[test]
    fn test_command_from_proto_create_link() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::CreateLink(
                proto::CreateLinkCommand {
                    output_port_id: 100,
                    input_port_id: 200,
                },
            )),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::CreateLink {
                output_port,
                input_port,
            } => {
                assert_eq!(output_port.raw(), 100);
                assert_eq!(input_port.raw(), 200);
            }
            _ => panic!("Expected CreateLink command"),
        }
    }

    #[test]
    fn test_command_from_proto_remove_link() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::RemoveLink(
                proto::RemoveLinkCommand { link_id: 500 },
            )),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::RemoveLink(link_id) => {
                assert_eq!(link_id.raw(), 500);
            }
            _ => panic!("Expected RemoveLink command"),
        }
    }

    #[test]
    fn test_command_from_proto_toggle_link() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::ToggleLink(
                proto::ToggleLinkCommand {
                    link_id: 500,
                    active: false,
                },
            )),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::ToggleLink { link_id, active } => {
                assert_eq!(link_id.raw(), 500);
                assert!(!active);
            }
            _ => panic!("Expected ToggleLink command"),
        }
    }

    #[test]
    fn test_command_from_proto_set_volume() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::SetVolume(
                proto::SetVolumeCommand {
                    node_id: 42,
                    volume: 0.75,
                },
            )),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::SetVolume { node_id, volume } => {
                assert_eq!(node_id.raw(), 42);
                assert!((volume.master - 0.75).abs() < f32::EPSILON);
            }
            _ => panic!("Expected SetVolume command"),
        }
    }

    #[test]
    fn test_command_from_proto_set_mute() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::SetMute(proto::SetMuteCommand {
                node_id: 42,
                muted: true,
            })),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::SetMute { node_id, muted } => {
                assert_eq!(node_id.raw(), 42);
                assert!(muted);
            }
            _ => panic!("Expected SetMute command"),
        }
    }

    #[test]
    fn test_command_from_proto_set_channel_volume() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::SetChannelVolume(
                proto::SetChannelVolumeCommand {
                    node_id: 42,
                    channel: 1,
                    volume: 0.5,
                },
            )),
        };

        let app_cmd = command_from_proto(&cmd).unwrap();
        match app_cmd {
            AppCommand::SetChannelVolume {
                node_id,
                channel,
                volume,
            } => {
                assert_eq!(node_id.raw(), 42);
                assert_eq!(channel, 1);
                assert!((volume - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("Expected SetChannelVolume command"),
        }
    }

    #[test]
    fn test_command_from_proto_panic_commands_return_none() {
        // Panic commands are no longer supported
        let cmd = proto::Command {
            command: Some(proto::command::Command::PanicMute(
                proto::PanicMuteCommand {},
            )),
        };
        assert!(command_from_proto(&cmd).is_none());

        let cmd = proto::Command {
            command: Some(proto::command::Command::PanicRestore(
                proto::PanicRestoreCommand {},
            )),
        };
        assert!(command_from_proto(&cmd).is_none());
    }

    #[test]
    fn test_command_from_proto_safety_mode_returns_none() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::SetSafetyMode(
                proto::SetSafetyModeCommand {
                    mode: proto::SafetyMode::ReadOnly as i32,
                },
            )),
        };

        // UI-only commands should return None
        assert!(command_from_proto(&cmd).is_none());
    }

    #[test]
    fn test_command_from_proto_routing_lock_returns_none() {
        let cmd = proto::Command {
            command: Some(proto::command::Command::ToggleRoutingLock(
                proto::ToggleRoutingLockCommand {},
            )),
        };

        // UI-only commands should return None
        assert!(command_from_proto(&cmd).is_none());
    }

    #[test]
    fn test_command_from_proto_empty_returns_none() {
        let cmd = proto::Command { command: None };
        assert!(command_from_proto(&cmd).is_none());
    }

    // ========================================================================
    // Event conversion tests
    // ========================================================================

    #[test]
    fn test_event_to_proto_connected() {
        let event = PwEvent::Connected;
        let proto_event = event_to_proto(&event, 1, 1000).unwrap();

        assert_eq!(proto_event.sequence, 1);
        assert_eq!(proto_event.timestamp_ms, 1000);

        match proto_event.event.unwrap() {
            proto::event::Event::ConnectionStatus(status) => {
                assert_eq!(status.state, proto::ConnectionState::Connected as i32);
            }
            _ => panic!("Expected ConnectionStatus event"),
        }
    }

    #[test]
    fn test_event_to_proto_disconnected() {
        let event = PwEvent::Disconnected;
        let proto_event = event_to_proto(&event, 2, 2000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::ConnectionStatus(status) => {
                assert_eq!(status.state, proto::ConnectionState::Disconnected as i32);
            }
            _ => panic!("Expected ConnectionStatus event"),
        }
    }

    #[test]
    fn test_event_to_proto_reconnecting() {
        let event = PwEvent::Reconnecting {
            attempt: 2,
            max_attempts: 5,
        };
        let proto_event = event_to_proto(&event, 3, 3000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::ConnectionStatus(status) => {
                assert_eq!(status.state, proto::ConnectionState::Reconnecting as i32);
                assert_eq!(status.reconnect_attempt, 2);
                assert_eq!(status.max_reconnect_attempts, 5);
            }
            _ => panic!("Expected ConnectionStatus event"),
        }
    }

    #[test]
    fn test_event_to_proto_error() {
        let event = PwEvent::Error(crate::pipewire::events::PwError {
            code: -1,
            message: "Test error".to_string(),
        });
        let proto_event = event_to_proto(&event, 4, 4000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::ConnectionStatus(status) => {
                assert_eq!(status.state, proto::ConnectionState::Error as i32);
                assert_eq!(status.error_message, "Test error");
            }
            _ => panic!("Expected ConnectionStatus event"),
        }
    }

    #[test]
    fn test_event_to_proto_node_added() {
        let node_info = NodeInfo {
            id: NodeId::new(42),
            name: "Test Node".to_string(),
            client_id: Some(ClientId::new(10)),
            media_class: Some(MediaClass::AudioSource),
            application_name: Some("TestApp".to_string()),
            description: None,
            nick: None,
            format: None,
            layer: NodeLayer::Session,
            factory_name: None,
            device_id: None,
            object_path: None,
            link_group: None,
            client_api: None,
            target_object: None,
        };
        let event = PwEvent::NodeAdded(node_info);
        let proto_event = event_to_proto(&event, 5, 5000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::NodeAdded(added) => {
                let node = added.node.unwrap();
                assert_eq!(node.id, 42);
                assert_eq!(node.name, "Test Node");
            }
            _ => panic!("Expected NodeAdded event"),
        }
    }

    #[test]
    fn test_event_to_proto_node_removed() {
        let event = PwEvent::NodeRemoved(NodeId::new(42));
        let proto_event = event_to_proto(&event, 6, 6000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::NodeRemoved(removed) => {
                assert_eq!(removed.node_id, 42);
            }
            _ => panic!("Expected NodeRemoved event"),
        }
    }

    #[test]
    fn test_event_to_proto_port_added() {
        let port_info = PortInfo {
            id: PortId::new(100),
            node_id: NodeId::new(42),
            name: "output_FL".to_string(),
            direction: PortDirection::Output,
            channel: Some(0),
            physical_path: None,
            alias: None,
            is_monitor: false,
            is_control: false,
        };
        let event = PwEvent::PortAdded(port_info);
        let proto_event = event_to_proto(&event, 7, 7000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::PortAdded(added) => {
                let port = added.port.unwrap();
                assert_eq!(port.id, 100);
                assert_eq!(port.node_id, 42);
            }
            _ => panic!("Expected PortAdded event"),
        }
    }

    #[test]
    fn test_event_to_proto_port_removed() {
        let event = PwEvent::PortRemoved(PortId::new(100));
        let proto_event = event_to_proto(&event, 8, 8000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::PortRemoved(removed) => {
                assert_eq!(removed.port_id, 100);
            }
            _ => panic!("Expected PortRemoved event"),
        }
    }

    #[test]
    fn test_event_to_proto_link_added() {
        let link_info = LinkInfo {
            id: LinkId::new(500),
            output_port: PortId::new(100),
            input_port: PortId::new(200),
            output_node: NodeId::new(10),
            input_node: NodeId::new(20),
            state: LinkState::Active,
            active: true,
        };
        let event = PwEvent::LinkAdded(link_info);
        let proto_event = event_to_proto(&event, 9, 9000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::LinkAdded(added) => {
                let link = added.link.unwrap();
                assert_eq!(link.id, 500);
                assert!(link.is_active);
            }
            _ => panic!("Expected LinkAdded event"),
        }
    }

    #[test]
    fn test_event_to_proto_link_removed() {
        let event = PwEvent::LinkRemoved(LinkId::new(500));
        let proto_event = event_to_proto(&event, 10, 10000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::LinkRemoved(removed) => {
                assert_eq!(removed.link_id, 500);
            }
            _ => panic!("Expected LinkRemoved event"),
        }
    }

    #[test]
    fn test_event_to_proto_volume_changed() {
        let volume = VolumeControl {
            master: 0.8,
            channels: vec![0.8, 0.8],
            muted: false,
            step: 0.05,
        };
        let event = PwEvent::VolumeChanged(NodeId::new(42), volume);
        let proto_event = event_to_proto(&event, 11, 11000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::VolumeChanged(changed) => {
                assert_eq!(changed.node_id, 42);
                let vol = changed.volume.unwrap();
                assert!((vol.master - 0.8).abs() < f32::EPSILON);
            }
            _ => panic!("Expected VolumeChanged event"),
        }
    }

    #[test]
    fn test_event_to_proto_mute_changed() {
        let event = PwEvent::MuteChanged(NodeId::new(42), true);
        let proto_event = event_to_proto(&event, 12, 12000).unwrap();

        match proto_event.event.unwrap() {
            proto::event::Event::MuteChanged(changed) => {
                assert_eq!(changed.node_id, 42);
                assert!(changed.muted);
            }
            _ => panic!("Expected MuteChanged event"),
        }
    }

    #[test]
    fn test_event_to_proto_meter_update_returns_none() {
        // Meter updates are handled via separate streaming, not as events
        let event = PwEvent::MeterUpdate(vec![]);
        assert!(event_to_proto(&event, 13, 13000).is_none());
    }

    // ========================================================================
    // Meter batch conversion tests
    // ========================================================================

    #[test]
    fn test_meter_batch_to_proto() {
        let updates = vec![
            MeterUpdate {
                node_id: NodeId::new(42),
                peak: vec![0.9, 0.85],
                rms: vec![0.5, 0.45],
            },
            MeterUpdate {
                node_id: NodeId::new(43),
                peak: vec![0.7],
                rms: vec![0.3],
            },
        ];

        let batch = meter_batch_to_proto(&updates, 1234);

        assert_eq!(batch.timestamp_ms, 1234);
        assert_eq!(batch.entries.len(), 2);

        assert_eq!(batch.entries[0].node_id, 42);
        assert_eq!(batch.entries[0].peak, vec![0.9, 0.85]);
        assert_eq!(batch.entries[0].rms, vec![0.5, 0.45]);

        assert_eq!(batch.entries[1].node_id, 43);
        assert_eq!(batch.entries[1].peak, vec![0.7]);
        assert_eq!(batch.entries[1].rms, vec![0.3]);
    }

    #[test]
    fn test_meter_batch_to_proto_empty() {
        let updates = vec![];
        let batch = meter_batch_to_proto(&updates, 0);

        assert_eq!(batch.timestamp_ms, 0);
        assert!(batch.entries.is_empty());
    }

    // ========================================================================
    // Media class conversion tests
    // ========================================================================

    #[test]
    fn test_media_class_strings() {
        let classes = [
            (MediaClass::AudioSource, "Audio/Source"),
            (MediaClass::AudioSink, "Audio/Sink"),
            (MediaClass::StreamOutputAudio, "Stream/Output/Audio"),
            (MediaClass::StreamInputAudio, "Stream/Input/Audio"),
            (MediaClass::VideoSource, "Video/Source"),
            (MediaClass::VideoSink, "Video/Sink"),
            (MediaClass::MidiSource, "Midi/Source"),
            (MediaClass::MidiSink, "Midi/Sink"),
            (MediaClass::AudioVideoSource, "Audio/Video/Source"),
            (MediaClass::AudioDevice, "Audio/Device"),
            (MediaClass::VideoDevice, "Video/Device"),
            (
                MediaClass::Other("Custom/Class".to_string()),
                "Custom/Class",
            ),
        ];

        for (mc, expected) in classes {
            assert_eq!(media_class_to_string(&mc), expected);
        }
    }

    // ========================================================================
    // Node layer conversion tests
    // ========================================================================

    #[test]
    fn test_node_layer_to_proto() {
        assert_eq!(
            node_layer_to_proto(&NodeLayer::Hardware),
            proto::NodeLayer::Hardware
        );
        assert_eq!(
            node_layer_to_proto(&NodeLayer::Pipewire),
            proto::NodeLayer::Pipewire
        );
        assert_eq!(
            node_layer_to_proto(&NodeLayer::Session),
            proto::NodeLayer::Session
        );
    }
}
