//! gRPC client for connecting to remote pipeflow instances.
//!
//! Provides the RemoteConnection type that can be used as an alternative
//! to the local PipeWire connection.

use crossbeam::channel::{self, Receiver, Sender};
use tokio::sync::mpsc;

use crate::core::commands::AppCommand;
use crate::domain::audio::VolumeControl;
use crate::domain::graph::{AudioFormat, LinkState, MediaClass, NodeLayer, PortDirection};
use crate::pipewire::events::{LinkInfo, MeterUpdate, NodeInfo, PortInfo, PwError, PwEvent};
use crate::util::id::{ClientId, LinkId, NodeId, PortId};

use super::proto;
use super::proto::pipeflow_client::PipeflowClient;
use super::PROTOCOL_VERSION;

/// A connection to a remote pipeflow instance via gRPC.
pub struct RemoteConnection {
    /// Event receiver (same interface as PwConnection)
    event_rx: Receiver<PwEvent>,
    /// Command sender (same interface as PwConnection)
    pub command_tx: Sender<AppCommand>,
    /// Meter update receiver
    meter_rx: Receiver<Vec<MeterUpdate>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl RemoteConnection {
    /// Connects to a remote pipeflow server.
    pub async fn connect(
        addr: &str,
        token: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = format!("http://{}", addr);
        let mut client = PipeflowClient::connect(endpoint).await?;

        // Authenticate
        let connect_req = proto::ConnectRequest {
            protocol_version: PROTOCOL_VERSION,
            token: token.unwrap_or_default(),
            client_id: format!("pipeflow-client-{}", std::process::id()),
        };

        let response = client.authenticate(connect_req).await?.into_inner();

        if !response.accepted {
            return Err(format!("Connection rejected: {}", response.error).into());
        }

        tracing::info!("Connected to remote server: {}", response.server_id);

        // Create channels
        let (event_tx, event_rx) = channel::unbounded();
        let (command_tx, command_rx) = channel::unbounded::<AppCommand>();
        let (meter_tx, meter_rx) = channel::bounded(16);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // Get initial state
        let initial_state = client.get_state(proto::Empty {}).await?.into_inner();

        // Convert initial state to events
        Self::emit_initial_state(&event_tx, &initial_state);

        // Send connected event
        let _ = event_tx.send(PwEvent::Connected);

        // Clone client for event subscription
        let mut event_client = client.clone();
        let event_tx_clone = event_tx.clone();

        // Spawn event subscription task
        tokio::spawn(async move {
            let subscribe_req = proto::SubscribeRequest {
                topics: vec![proto::SubscriptionTopic::TopicAll as i32],
            };

            match event_client.subscribe_events(subscribe_req).await {
                Ok(response) => {
                    let mut stream = response.into_inner();

                    loop {
                        tokio::select! {
                            event = stream.message() => {
                                match event {
                                    Ok(Some(proto_event)) => {
                                        if let Some(pw_event) = Self::proto_event_to_pw(&proto_event) {
                                            if event_tx_clone.send(pw_event).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        // Stream ended
                                        let _ = event_tx_clone.send(PwEvent::Disconnected);
                                        break;
                                    }
                                    Err(e) => {
                                        tracing::error!("Event stream error: {}", e);
                                        let _ = event_tx_clone.send(PwEvent::Disconnected);
                                        break;
                                    }
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to subscribe to events: {}", e);
                    let _ = event_tx_clone.send(PwEvent::Error(PwError {
                        code: -1,
                        message: e.to_string(),
                    }));
                }
            }
        });

        // Clone client for meter subscription
        let mut meter_client = client.clone();

        // Spawn meter subscription task
        tokio::spawn(async move {
            let meter_req = proto::MeterSubscription {
                level: proto::MeterLevel::Full as i32,
                max_rate_hz: 30,
                threshold: 0.01,
            };

            match meter_client.subscribe_meters(meter_req).await {
                Ok(response) => {
                    let mut stream = response.into_inner();

                    loop {
                        match stream.message().await {
                            Ok(Some(batch)) => {
                                let updates: Vec<MeterUpdate> = batch
                                    .entries
                                    .iter()
                                    .map(|e| MeterUpdate {
                                        node_id: NodeId::new(e.node_id),
                                        peak: e.peak.clone(),
                                        rms: e.rms.clone(),
                                    })
                                    .collect();

                                if !updates.is_empty() {
                                    // Use try_send to avoid blocking
                                    let _ = meter_tx.try_send(updates);
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                tracing::warn!("Meter stream error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to subscribe to meters: {}", e);
                }
            }
        });

        // Spawn command handling task
        let mut cmd_client = client;
        tokio::spawn(async move {
            while let Ok(cmd) = command_rx.recv() {
                let proto_cmd = Self::app_command_to_proto(&cmd);
                if let Err(e) = cmd_client.execute_command(proto_cmd).await {
                    tracing::error!("Failed to execute command: {}", e);
                }
            }
        });

        Ok(Self {
            event_rx,
            command_tx,
            meter_rx,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Drains all pending events.
    pub fn drain_events(&self) -> Vec<PwEvent> {
        self.event_rx.try_iter().collect()
    }

    /// Drains all pending meter updates.
    pub fn drain_meter_updates(&self) -> Vec<Vec<MeterUpdate>> {
        self.meter_rx.try_iter().collect()
    }

    /// Emits events for the initial state snapshot.
    fn emit_initial_state(event_tx: &Sender<PwEvent>, state: &proto::GraphState) {
        // Emit nodes
        for node in &state.nodes {
            if let Some(info) = Self::proto_node_to_info(node) {
                let _ = event_tx.send(PwEvent::NodeAdded(info));
            }
        }

        // Emit ports
        for port in &state.ports {
            if let Some(info) = Self::proto_port_to_info(port) {
                let _ = event_tx.send(PwEvent::PortAdded(info));
            }
        }

        // Emit links
        for link in &state.links {
            if let Some(info) = Self::proto_link_to_info(link) {
                let _ = event_tx.send(PwEvent::LinkAdded(info));
            }
        }

        // Emit volumes
        for (node_id, volume) in &state.volumes {
            let vol = Self::proto_volume_to_control(volume);
            let _ = event_tx.send(PwEvent::VolumeChanged(NodeId::new(*node_id), vol));
        }
    }

    /// Converts a proto Event to a PwEvent.
    fn proto_event_to_pw(event: &proto::Event) -> Option<PwEvent> {
        event.event.as_ref().and_then(|e| match e {
            proto::event::Event::ConnectionStatus(status) => {
                let state = proto::ConnectionState::try_from(status.state).ok()?;
                Some(match state {
                    proto::ConnectionState::Connected => PwEvent::Connected,
                    proto::ConnectionState::Disconnected => PwEvent::Disconnected,
                    proto::ConnectionState::Reconnecting => PwEvent::Reconnecting {
                        attempt: status.reconnect_attempt,
                        max_attempts: status.max_reconnect_attempts,
                    },
                    proto::ConnectionState::Error => PwEvent::Error(PwError {
                        code: -1,
                        message: status.error_message.clone(),
                    }),
                    _ => return None,
                })
            }
            proto::event::Event::NodeAdded(added) => added
                .node
                .as_ref()
                .and_then(Self::proto_node_to_info)
                .map(PwEvent::NodeAdded),
            proto::event::Event::NodeRemoved(removed) => {
                Some(PwEvent::NodeRemoved(NodeId::new(removed.node_id)))
            }
            proto::event::Event::PortAdded(added) => added
                .port
                .as_ref()
                .and_then(Self::proto_port_to_info)
                .map(PwEvent::PortAdded),
            proto::event::Event::PortRemoved(removed) => {
                Some(PwEvent::PortRemoved(PortId::new(removed.port_id)))
            }
            proto::event::Event::LinkAdded(added) => added
                .link
                .as_ref()
                .and_then(Self::proto_link_to_info)
                .map(PwEvent::LinkAdded),
            proto::event::Event::LinkRemoved(removed) => {
                Some(PwEvent::LinkRemoved(LinkId::new(removed.link_id)))
            }
            proto::event::Event::VolumeChanged(changed) => changed.volume.as_ref().map(|v| {
                PwEvent::VolumeChanged(
                    NodeId::new(changed.node_id),
                    Self::proto_volume_to_control(v),
                )
            }),
            proto::event::Event::MuteChanged(changed) => Some(PwEvent::MuteChanged(
                NodeId::new(changed.node_id),
                changed.muted,
            )),
            proto::event::Event::SafetyStatus(_) => {
                // Safety status is handled at the app level, not as PwEvent
                None
            }
        })
    }

    /// Converts a proto Node to NodeInfo.
    fn proto_node_to_info(node: &proto::Node) -> Option<NodeInfo> {
        // Convert proto layer to domain layer
        let layer = match proto::NodeLayer::try_from(node.layer).ok() {
            Some(proto::NodeLayer::Hardware) => NodeLayer::Hardware,
            Some(proto::NodeLayer::Pipewire) => NodeLayer::Pipewire,
            Some(proto::NodeLayer::Session) => NodeLayer::Session,
            _ => NodeLayer::Session, // Default to session layer
        };

        Some(NodeInfo {
            id: NodeId::new(node.id),
            name: node.name.clone(),
            client_id: node.client_id.map(ClientId::new),
            media_class: node
                .media_class
                .as_ref()
                .map(|s| MediaClass::from_pipewire_str(s)),
            application_name: node.application_name.clone(),
            description: node.description.clone(),
            nick: node.nick.clone(),
            format: node.format.as_ref().map(|f| AudioFormat {
                sample_rate: f.sample_rate,
                channels: f.channels,
                format: f.format.clone(),
            }),
            layer,
            factory_name: node.factory_name.clone(),
            device_id: node.device_id,
            object_path: node.object_path.clone(),
            link_group: node.link_group.clone(),
            client_api: node.client_api.clone(),
            target_object: node.target_object,
        })
    }

    /// Converts a proto Port to PortInfo.
    fn proto_port_to_info(port: &proto::Port) -> Option<PortInfo> {
        let direction = match proto::PortDirection::try_from(port.direction).ok()? {
            proto::PortDirection::Input => PortDirection::Input,
            proto::PortDirection::Output => PortDirection::Output,
            _ => return None,
        };

        Some(PortInfo {
            id: PortId::new(port.id),
            node_id: NodeId::new(port.node_id),
            name: port.name.clone(),
            direction,
            channel: port.channel,
            physical_path: port.physical_path.clone(),
            alias: port.alias.clone(),
            is_monitor: port.is_monitor,
            is_control: port.is_control,
        })
    }

    /// Converts a proto Link to LinkInfo.
    fn proto_link_to_info(link: &proto::Link) -> Option<LinkInfo> {
        let state = match proto::LinkState::try_from(link.state).ok()? {
            proto::LinkState::Init => LinkState::Init,
            proto::LinkState::Negotiating => LinkState::Negotiating,
            proto::LinkState::Allocating => LinkState::Allocating,
            proto::LinkState::Paused => LinkState::Paused,
            proto::LinkState::Active => LinkState::Active,
            proto::LinkState::Error => LinkState::Error,
            proto::LinkState::Unlinked => LinkState::Unlinked,
            _ => LinkState::Init,
        };

        Some(LinkInfo {
            id: LinkId::new(link.id),
            output_port: PortId::new(link.output_port_id),
            input_port: PortId::new(link.input_port_id),
            output_node: NodeId::new(link.output_node_id),
            input_node: NodeId::new(link.input_node_id),
            state,
            active: link.is_active,
        })
    }

    /// Converts a proto Volume to VolumeControl.
    fn proto_volume_to_control(vol: &proto::Volume) -> VolumeControl {
        VolumeControl {
            master: vol.master,
            channels: vol.channels.clone(),
            muted: vol.muted,
            step: 0.05,
        }
    }

    /// Converts an AppCommand to a proto Command.
    fn app_command_to_proto(cmd: &AppCommand) -> proto::Command {
        let command = match cmd {
            AppCommand::CreateLink {
                output_port,
                input_port,
            } => Some(proto::command::Command::CreateLink(
                proto::CreateLinkCommand {
                    output_port_id: output_port.raw(),
                    input_port_id: input_port.raw(),
                },
            )),
            AppCommand::RemoveLink(link_id) => Some(proto::command::Command::RemoveLink(
                proto::RemoveLinkCommand {
                    link_id: link_id.raw(),
                },
            )),
            AppCommand::ToggleLink { link_id, active } => Some(
                proto::command::Command::ToggleLink(proto::ToggleLinkCommand {
                    link_id: link_id.raw(),
                    active: *active,
                }),
            ),
            AppCommand::SetVolume { node_id, volume } => Some(proto::command::Command::SetVolume(
                proto::SetVolumeCommand {
                    node_id: node_id.raw(),
                    volume: volume.master,
                },
            )),
            AppCommand::SetMute { node_id, muted } => {
                Some(proto::command::Command::SetMute(proto::SetMuteCommand {
                    node_id: node_id.raw(),
                    muted: *muted,
                }))
            }
            AppCommand::SetChannelVolume {
                node_id,
                channel,
                volume,
            } => Some(proto::command::Command::SetChannelVolume(
                proto::SetChannelVolumeCommand {
                    node_id: node_id.raw(),
                    channel: *channel as u32,
                    volume: *volume,
                },
            )),
            // Mixer node commands are local-only (not networked)
            AppCommand::CreateMixerNode { .. }
            | AppCommand::RemoveMixerNode(_)
            | AppCommand::SetMixerStripGain { .. }
            | AppCommand::SetMixerStripMute { .. }
            | AppCommand::SetMixerMasterGain { .. }
            | AppCommand::SetMixerMasterMute { .. } => None,
            // These commands don't go through the network
            AppCommand::Disconnect | AppCommand::StartAllMeters | AppCommand::StopAllMeters => None,
        };

        proto::Command { command }
    }
}

impl Drop for RemoteConnection {
    fn drop(&mut self) {
        // Signal shutdown
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.blocking_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Proto to domain conversion tests
    // ========================================================================

    #[test]
    fn test_proto_node_to_info() {
        let proto_node = proto::Node {
            id: 42,
            name: "Test Node".to_string(),
            client_id: Some(10),
            media_class: Some("Audio/Sink".to_string()),
            application_name: Some("TestApp".to_string()),
            description: Some("A test node".to_string()),
            nick: Some("test".to_string()),
            format: Some(proto::AudioFormat {
                sample_rate: 48000,
                channels: 2,
                format: "F32LE".to_string(),
            }),
            port_ids: vec![1, 2],
            is_active: true,
            layer: proto::NodeLayer::Session as i32,
            factory_name: Some("spa-node-factory".to_string()),
            device_id: Some(5),
            object_path: Some("/devices/0".to_string()),
            link_group: None,
            client_api: Some("pipewire".to_string()),
            target_object: None,
        };

        let info = RemoteConnection::proto_node_to_info(&proto_node).unwrap();

        assert_eq!(info.id.raw(), 42);
        assert_eq!(info.name, "Test Node");
        assert_eq!(info.client_id.unwrap().raw(), 10);
        assert!(matches!(info.media_class, Some(MediaClass::AudioSink)));
        assert_eq!(info.application_name, Some("TestApp".to_string()));
        assert_eq!(info.description, Some("A test node".to_string()));
        assert_eq!(info.nick, Some("test".to_string()));

        let fmt = info.format.unwrap();
        assert_eq!(fmt.sample_rate, 48000);
        assert_eq!(fmt.channels, 2);
        assert_eq!(fmt.format, "F32LE");

        assert!(matches!(info.layer, NodeLayer::Session));
        assert_eq!(info.factory_name, Some("spa-node-factory".to_string()));
        assert_eq!(info.device_id, Some(5));
    }

    #[test]
    fn test_proto_node_to_info_minimal() {
        let proto_node = proto::Node {
            id: 1,
            name: "Minimal".to_string(),
            client_id: None,
            media_class: None,
            application_name: None,
            description: None,
            nick: None,
            format: None,
            port_ids: vec![],
            is_active: false,
            layer: proto::NodeLayer::Hardware as i32,
            factory_name: None,
            device_id: None,
            object_path: None,
            link_group: None,
            client_api: None,
            target_object: None,
        };

        let info = RemoteConnection::proto_node_to_info(&proto_node).unwrap();

        assert_eq!(info.id.raw(), 1);
        assert_eq!(info.name, "Minimal");
        assert!(info.client_id.is_none());
        assert!(info.media_class.is_none());
        assert!(info.format.is_none());
        assert!(matches!(info.layer, NodeLayer::Hardware));
    }

    #[test]
    fn test_proto_node_layers() {
        let test_cases = [
            (proto::NodeLayer::Hardware as i32, NodeLayer::Hardware),
            (proto::NodeLayer::Pipewire as i32, NodeLayer::Pipewire),
            (proto::NodeLayer::Session as i32, NodeLayer::Session),
            (proto::NodeLayer::Unspecified as i32, NodeLayer::Session), // Default
        ];

        for (proto_layer, expected_layer) in test_cases {
            let proto_node = proto::Node {
                id: 1,
                name: "Test".to_string(),
                layer: proto_layer,
                ..Default::default()
            };

            let info = RemoteConnection::proto_node_to_info(&proto_node).unwrap();
            assert!(
                matches!((&info.layer, &expected_layer), (a, b) if std::mem::discriminant(a) == std::mem::discriminant(b)),
                "Layer mismatch for proto value {}: got {:?}, expected {:?}",
                proto_layer,
                info.layer,
                expected_layer
            );
        }
    }

    #[test]
    fn test_proto_port_to_info_input() {
        let proto_port = proto::Port {
            id: 100,
            node_id: 42,
            name: "input_FL".to_string(),
            direction: proto::PortDirection::Input as i32,
            channel: Some(0),
            physical_path: Some("/dev/snd/pcm0".to_string()),
            alias: Some("Front Left".to_string()),
            is_monitor: false,
            is_control: false,
        };

        let info = RemoteConnection::proto_port_to_info(&proto_port).unwrap();

        assert_eq!(info.id.raw(), 100);
        assert_eq!(info.node_id.raw(), 42);
        assert_eq!(info.name, "input_FL");
        assert!(matches!(info.direction, PortDirection::Input));
        assert_eq!(info.channel, Some(0));
        assert_eq!(info.physical_path, Some("/dev/snd/pcm0".to_string()));
        assert_eq!(info.alias, Some("Front Left".to_string()));
        assert!(!info.is_monitor);
        assert!(!info.is_control);
    }

    #[test]
    fn test_proto_port_to_info_output() {
        let proto_port = proto::Port {
            id: 200,
            node_id: 50,
            name: "output_FR".to_string(),
            direction: proto::PortDirection::Output as i32,
            channel: Some(1),
            physical_path: None,
            alias: None,
            is_monitor: true,
            is_control: true,
        };

        let info = RemoteConnection::proto_port_to_info(&proto_port).unwrap();

        assert_eq!(info.id.raw(), 200);
        assert!(matches!(info.direction, PortDirection::Output));
        assert!(info.is_monitor);
        assert!(info.is_control);
    }

    #[test]
    fn test_proto_port_to_info_invalid_direction() {
        let proto_port = proto::Port {
            id: 1,
            node_id: 1,
            name: "test".to_string(),
            direction: proto::PortDirection::Unspecified as i32,
            channel: None,
            physical_path: None,
            alias: None,
            is_monitor: false,
            is_control: false,
        };

        // Invalid direction should return None
        assert!(RemoteConnection::proto_port_to_info(&proto_port).is_none());
    }

    #[test]
    fn test_proto_link_to_info() {
        let proto_link = proto::Link {
            id: 500,
            output_port_id: 100,
            input_port_id: 200,
            output_node_id: 10,
            input_node_id: 20,
            is_active: true,
            state: proto::LinkState::Active as i32,
        };

        let info = RemoteConnection::proto_link_to_info(&proto_link).unwrap();

        assert_eq!(info.id.raw(), 500);
        assert_eq!(info.output_port.raw(), 100);
        assert_eq!(info.input_port.raw(), 200);
        assert_eq!(info.output_node.raw(), 10);
        assert_eq!(info.input_node.raw(), 20);
        assert!(info.active);
        assert!(matches!(info.state, LinkState::Active));
    }

    #[test]
    fn test_proto_link_states() {
        let test_cases = [
            (proto::LinkState::Init as i32, LinkState::Init),
            (proto::LinkState::Negotiating as i32, LinkState::Negotiating),
            (proto::LinkState::Allocating as i32, LinkState::Allocating),
            (proto::LinkState::Paused as i32, LinkState::Paused),
            (proto::LinkState::Active as i32, LinkState::Active),
            (proto::LinkState::Error as i32, LinkState::Error),
            (proto::LinkState::Unlinked as i32, LinkState::Unlinked),
        ];

        for (proto_state, expected_state) in test_cases {
            let proto_link = proto::Link {
                id: 1,
                output_port_id: 1,
                input_port_id: 2,
                output_node_id: 1,
                input_node_id: 2,
                is_active: false,
                state: proto_state,
            };

            let info = RemoteConnection::proto_link_to_info(&proto_link).unwrap();
            assert!(
                matches!((&info.state, &expected_state), (a, b) if std::mem::discriminant(a) == std::mem::discriminant(b)),
                "State mismatch for proto value {}: got {:?}, expected {:?}",
                proto_state,
                info.state,
                expected_state
            );
        }
    }

    #[test]
    fn test_proto_volume_to_control() {
        let proto_vol = proto::Volume {
            master: 0.75,
            channels: vec![0.8, 0.7],
            muted: false,
        };

        let control = RemoteConnection::proto_volume_to_control(&proto_vol);

        assert!((control.master - 0.75).abs() < f32::EPSILON);
        assert_eq!(control.channels.len(), 2);
        assert!((control.channels[0] - 0.8).abs() < f32::EPSILON);
        assert!((control.channels[1] - 0.7).abs() < f32::EPSILON);
        assert!(!control.muted);
        assert!((control.step - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn test_proto_volume_muted() {
        let proto_vol = proto::Volume {
            master: 0.5,
            channels: vec![],
            muted: true,
        };

        let control = RemoteConnection::proto_volume_to_control(&proto_vol);
        assert!(control.muted);
    }

    // ========================================================================
    // App command to proto conversion tests
    // ========================================================================

    #[test]
    fn test_app_command_to_proto_create_link() {
        let cmd = AppCommand::CreateLink {
            output_port: PortId::new(100),
            input_port: PortId::new(200),
        };

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::CreateLink(create) => {
                assert_eq!(create.output_port_id, 100);
                assert_eq!(create.input_port_id, 200);
            }
            _ => panic!("Expected CreateLink command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_remove_link() {
        let cmd = AppCommand::RemoveLink(LinkId::new(500));

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::RemoveLink(remove) => {
                assert_eq!(remove.link_id, 500);
            }
            _ => panic!("Expected RemoveLink command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_toggle_link() {
        let cmd = AppCommand::ToggleLink {
            link_id: LinkId::new(500),
            active: false,
        };

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::ToggleLink(toggle) => {
                assert_eq!(toggle.link_id, 500);
                assert!(!toggle.active);
            }
            _ => panic!("Expected ToggleLink command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_set_volume() {
        let cmd = AppCommand::SetVolume {
            node_id: NodeId::new(42),
            volume: VolumeControl {
                master: 0.75,
                channels: vec![0.75, 0.75],
                muted: false,
                step: 0.05,
            },
        };

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::SetVolume(set) => {
                assert_eq!(set.node_id, 42);
                assert!((set.volume - 0.75).abs() < f32::EPSILON);
            }
            _ => panic!("Expected SetVolume command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_set_mute() {
        let cmd = AppCommand::SetMute {
            node_id: NodeId::new(42),
            muted: true,
        };

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::SetMute(set) => {
                assert_eq!(set.node_id, 42);
                assert!(set.muted);
            }
            _ => panic!("Expected SetMute command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_set_channel_volume() {
        let cmd = AppCommand::SetChannelVolume {
            node_id: NodeId::new(42),
            channel: 1,
            volume: 0.5,
        };

        let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);

        match proto_cmd.command.unwrap() {
            proto::command::Command::SetChannelVolume(set) => {
                assert_eq!(set.node_id, 42);
                assert_eq!(set.channel, 1);
                assert!((set.volume - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("Expected SetChannelVolume command"),
        }
    }

    #[test]
    fn test_app_command_to_proto_local_only_commands() {
        // These commands don't get sent over the network
        let local_commands = [
            AppCommand::Disconnect,
            AppCommand::StartAllMeters,
            AppCommand::StopAllMeters,
        ];

        for cmd in local_commands {
            let proto_cmd = RemoteConnection::app_command_to_proto(&cmd);
            assert!(
                proto_cmd.command.is_none(),
                "Local command should not produce proto: {:?}",
                cmd
            );
        }
    }

    // ========================================================================
    // Proto event to PwEvent conversion tests
    // ========================================================================

    #[test]
    fn test_proto_event_to_pw_connected() {
        let proto_event = proto::Event {
            sequence: 1,
            timestamp_ms: 1000,
            event: Some(proto::event::Event::ConnectionStatus(
                proto::ConnectionStatus {
                    state: proto::ConnectionState::Connected as i32,
                    reconnect_attempt: 0,
                    max_reconnect_attempts: 0,
                    error_message: String::new(),
                },
            )),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        assert!(matches!(pw_event, PwEvent::Connected));
    }

    #[test]
    fn test_proto_event_to_pw_disconnected() {
        let proto_event = proto::Event {
            sequence: 2,
            timestamp_ms: 2000,
            event: Some(proto::event::Event::ConnectionStatus(
                proto::ConnectionStatus {
                    state: proto::ConnectionState::Disconnected as i32,
                    reconnect_attempt: 0,
                    max_reconnect_attempts: 0,
                    error_message: String::new(),
                },
            )),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        assert!(matches!(pw_event, PwEvent::Disconnected));
    }

    #[test]
    fn test_proto_event_to_pw_reconnecting() {
        let proto_event = proto::Event {
            sequence: 3,
            timestamp_ms: 3000,
            event: Some(proto::event::Event::ConnectionStatus(
                proto::ConnectionStatus {
                    state: proto::ConnectionState::Reconnecting as i32,
                    reconnect_attempt: 2,
                    max_reconnect_attempts: 5,
                    error_message: String::new(),
                },
            )),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::Reconnecting {
                attempt,
                max_attempts,
            } => {
                assert_eq!(attempt, 2);
                assert_eq!(max_attempts, 5);
            }
            _ => panic!("Expected Reconnecting event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_error() {
        let proto_event = proto::Event {
            sequence: 4,
            timestamp_ms: 4000,
            event: Some(proto::event::Event::ConnectionStatus(
                proto::ConnectionStatus {
                    state: proto::ConnectionState::Error as i32,
                    reconnect_attempt: 0,
                    max_reconnect_attempts: 0,
                    error_message: "Test error".to_string(),
                },
            )),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::Error(err) => {
                assert_eq!(err.message, "Test error");
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_node_added() {
        let proto_event = proto::Event {
            sequence: 5,
            timestamp_ms: 5000,
            event: Some(proto::event::Event::NodeAdded(proto::NodeAdded {
                node: Some(proto::Node {
                    id: 42,
                    name: "Test Node".to_string(),
                    ..Default::default()
                }),
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::NodeAdded(info) => {
                assert_eq!(info.id.raw(), 42);
                assert_eq!(info.name, "Test Node");
            }
            _ => panic!("Expected NodeAdded event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_node_removed() {
        let proto_event = proto::Event {
            sequence: 6,
            timestamp_ms: 6000,
            event: Some(proto::event::Event::NodeRemoved(proto::NodeRemoved {
                node_id: 42,
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::NodeRemoved(id) => {
                assert_eq!(id.raw(), 42);
            }
            _ => panic!("Expected NodeRemoved event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_port_added() {
        let proto_event = proto::Event {
            sequence: 7,
            timestamp_ms: 7000,
            event: Some(proto::event::Event::PortAdded(proto::PortAdded {
                port: Some(proto::Port {
                    id: 100,
                    node_id: 42,
                    name: "test_port".to_string(),
                    direction: proto::PortDirection::Output as i32,
                    channel: None,
                    physical_path: None,
                    alias: None,
                    is_monitor: false,
                    is_control: false,
                }),
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::PortAdded(info) => {
                assert_eq!(info.id.raw(), 100);
                assert_eq!(info.node_id.raw(), 42);
            }
            _ => panic!("Expected PortAdded event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_port_removed() {
        let proto_event = proto::Event {
            sequence: 8,
            timestamp_ms: 8000,
            event: Some(proto::event::Event::PortRemoved(proto::PortRemoved {
                port_id: 100,
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::PortRemoved(id) => {
                assert_eq!(id.raw(), 100);
            }
            _ => panic!("Expected PortRemoved event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_link_added() {
        let proto_event = proto::Event {
            sequence: 9,
            timestamp_ms: 9000,
            event: Some(proto::event::Event::LinkAdded(proto::LinkAdded {
                link: Some(proto::Link {
                    id: 500,
                    output_port_id: 100,
                    input_port_id: 200,
                    output_node_id: 10,
                    input_node_id: 20,
                    is_active: true,
                    state: proto::LinkState::Active as i32,
                }),
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::LinkAdded(info) => {
                assert_eq!(info.id.raw(), 500);
                assert!(info.active);
            }
            _ => panic!("Expected LinkAdded event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_link_removed() {
        let proto_event = proto::Event {
            sequence: 10,
            timestamp_ms: 10000,
            event: Some(proto::event::Event::LinkRemoved(proto::LinkRemoved {
                link_id: 500,
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::LinkRemoved(id) => {
                assert_eq!(id.raw(), 500);
            }
            _ => panic!("Expected LinkRemoved event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_volume_changed() {
        let proto_event = proto::Event {
            sequence: 11,
            timestamp_ms: 11000,
            event: Some(proto::event::Event::VolumeChanged(proto::VolumeChanged {
                node_id: 42,
                volume: Some(proto::Volume {
                    master: 0.75,
                    channels: vec![0.8, 0.7],
                    muted: false,
                }),
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::VolumeChanged(node_id, vol) => {
                assert_eq!(node_id.raw(), 42);
                assert!((vol.master - 0.75).abs() < f32::EPSILON);
            }
            _ => panic!("Expected VolumeChanged event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_mute_changed() {
        let proto_event = proto::Event {
            sequence: 12,
            timestamp_ms: 12000,
            event: Some(proto::event::Event::MuteChanged(proto::MuteChanged {
                node_id: 42,
                muted: true,
            })),
        };

        let pw_event = RemoteConnection::proto_event_to_pw(&proto_event).unwrap();
        match pw_event {
            PwEvent::MuteChanged(node_id, muted) => {
                assert_eq!(node_id.raw(), 42);
                assert!(muted);
            }
            _ => panic!("Expected MuteChanged event"),
        }
    }

    #[test]
    fn test_proto_event_to_pw_safety_status_returns_none() {
        // Safety status events are handled at app level, not as PwEvent
        let proto_event = proto::Event {
            sequence: 13,
            timestamp_ms: 13000,
            event: Some(proto::event::Event::SafetyStatus(proto::SafetyStatus {
                mode: proto::SafetyMode::Normal as i32,
                routing_locked: false,
                panic_active: false,
            })),
        };

        assert!(RemoteConnection::proto_event_to_pw(&proto_event).is_none());
    }

    #[test]
    fn test_proto_event_to_pw_empty_event() {
        let proto_event = proto::Event {
            sequence: 14,
            timestamp_ms: 14000,
            event: None,
        };

        assert!(RemoteConnection::proto_event_to_pw(&proto_event).is_none());
    }
}
