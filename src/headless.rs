//! Headless application mode for running pipeflow as a gRPC server.
//!
//! This mode runs without a GUI and exposes the PipeWire graph
//! via gRPC for remote clients to connect to.

#[cfg(feature = "network")]
use std::net::SocketAddr;
#[cfg(feature = "network")]
use std::sync::Arc;

#[cfg(feature = "network")]
use tokio::signal;

#[cfg(feature = "network")]
use crate::core::commands::CommandHandler;
#[cfg(feature = "network")]
use crate::core::config::Config;
#[cfg(feature = "network")]
use crate::core::state::{create_shared_state, ConnectionState};
#[cfg(feature = "network")]
use crate::domain::graph::{Link, Node, Port};
#[cfg(feature = "network")]
use crate::network::server::GrpcServer;
#[cfg(feature = "network")]
use crate::pipewire::connection::PwConnection;
#[cfg(feature = "network")]
use crate::pipewire::events::PwEvent;
#[cfg(feature = "network")]
use crate::pipewire::meters::{MeterCollector, MeterConfig};
#[cfg(feature = "network")]
use crate::util::id::NodeIdentifier;
#[cfg(feature = "network")]
use crate::util::layout::SmartLayout;
#[cfg(feature = "network")]
use crate::util::spatial::Position;

/// Runs pipeflow in headless mode (gRPC server without GUI).
#[cfg(feature = "network")]
pub async fn run_headless(
    bind_addr: SocketAddr,
    token: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load configuration
    let config = Config::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config, using defaults: {}", e);
        Config::default()
    });

    // Create shared state
    let state = create_shared_state();

    // Initialize PipeWire connection
    let pw_connection =
        PwConnection::new().map_err(|e| format!("Failed to create PipeWire connection: {}", e))?;
    let command_handler = Arc::new(CommandHandler::new(pw_connection.command_tx.clone()));

    // Set initial safety mode from config
    {
        let mut state = state.write();
        state.safety.set_mode(config.behavior.startup_safety_mode);
    }

    // Create and start meter collector
    let meter_config = MeterConfig {
        enabled: config.meters.enabled,
        refresh_rate: config.meters.refresh_rate,
        buffer_size: 4,
    };
    let mut meter_collector = MeterCollector::new(meter_config);
    meter_collector.start();

    // Create gRPC server
    let grpc_server = GrpcServer::new(bind_addr, state.clone(), command_handler.clone(), token);

    tracing::info!("Starting headless pipeflow server on {}", bind_addr);

    // Spawn event processing task
    let state_clone = state.clone();
    let grpc_server_clone = grpc_server.clone();
    let config_meters_enabled = config.meters.enabled;

    let event_handle = tokio::task::spawn_blocking(move || {
        loop {
            // Process PipeWire events
            let events = pw_connection.drain_events();
            if !events.is_empty() {
                let mut state = state_clone.write();

                for event in events {
                    // Broadcast event to connected clients
                    grpc_server_clone.broadcast_event(&event);

                    // Process event locally
                    process_event(&mut state, &event);
                }
            }

            // Process meter updates
            if config_meters_enabled {
                let meter_batches = pw_connection.drain_meter_updates();
                for batch in meter_batches {
                    if !batch.is_empty() {
                        // Broadcast to clients
                        grpc_server_clone.broadcast_meters(&batch);

                        // Update local state
                        let mut state = state_clone.write();
                        for update in &batch {
                            if let Some(meter) = state.graph.meters.get_mut(&update.node_id) {
                                meter.update(update.peak.clone(), update.rms.clone());
                            }
                        }
                    }
                }
            }

            // Sleep to avoid busy loop (~60 Hz)
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    });

    // Spawn gRPC server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = grpc_server.run().await {
            tracing::error!("gRPC server error: {}", e);
        }
    });

    // Wait for shutdown signal
    tracing::info!("Headless server running. Press Ctrl+C to stop.");

    signal::ctrl_c().await?;

    tracing::info!("Shutting down headless server...");

    // Stop tasks
    event_handle.abort();
    server_handle.abort();

    Ok(())
}

/// Processes a PipeWire event and updates the application state.
#[cfg(feature = "network")]
fn process_event(state: &mut crate::core::state::AppState, event: &PwEvent) {
    match event {
        PwEvent::Connected => {
            state.connection = ConnectionState::Connected;
            tracing::info!("Connected to PipeWire");
        }
        PwEvent::Disconnected => {
            state.connection = ConnectionState::Disconnected;
            state.clear_graph();
            tracing::warn!("Disconnected from PipeWire");
        }
        PwEvent::Reconnecting {
            attempt,
            max_attempts,
        } => {
            state.connection = ConnectionState::Connecting;
            tracing::info!(
                "Reconnecting to PipeWire (attempt {}/{})",
                attempt,
                max_attempts
            );
        }
        PwEvent::Error(err) => {
            state.connection = ConnectionState::Error;
            tracing::error!("PipeWire error: {}", err);
        }
        PwEvent::NodeAdded(info) => {
            let node = Node {
                id: info.id,
                name: info.name.clone(),
                client_id: info.client_id,
                media_class: info.media_class.clone(),
                application_name: info.application_name.clone(),
                description: info.description.clone(),
                nick: info.nick.clone(),
                format: info.format.clone(),
                port_ids: Vec::new(),
                is_active: true,
                layer: info.layer,
            };
            state.graph.add_node(node);

            // Create stable identifier
            let identifier = NodeIdentifier::new(
                info.name.clone(),
                info.application_name.clone(),
                info.media_class
                    .as_ref()
                    .map(|mc| mc.display_name().to_string()),
            );

            // Restore position if saved, otherwise calculate using centralized layout
            if !state.ui.restore_position_for_node(info.id, &identifier) {
                // No saved position, calculate a new one using smart layout
                let layout = SmartLayout::new();
                let position = layout.calculate_new_node_position(
                    info.id,
                    &state.graph,
                    &state.ui.node_positions,
                    Position::zero(), // Center at origin for headless mode
                );
                state.ui.node_positions.insert(info.id, position);
                state
                    .ui
                    .persistent_positions
                    .insert(identifier.clone(), position);
            }

            state
                .ui
                .restore_uninteresting_for_node(info.id, &identifier);
            state.ui.groups.reconcile_node(info.id, &identifier);
        }
        PwEvent::NodeRemoved(id) => {
            state.graph.remove_node(id);
            state.ui.selected_nodes.remove(id);
        }
        PwEvent::PortAdded(info) => {
            let port = Port {
                id: info.id,
                node_id: info.node_id,
                name: info.name.clone(),
                direction: info.direction,
                channel: info.channel,
                physical_path: info.physical_path.clone(),
                alias: info.alias.clone(),
                is_monitor: info.is_monitor,
                is_control: info.is_control,
            };
            state.graph.add_port(port);
        }
        PwEvent::PortRemoved(id) => {
            state.graph.remove_port(id);
        }
        PwEvent::LinkAdded(info) => {
            let link = Link {
                id: info.id,
                output_port: info.output_port,
                input_port: info.input_port,
                output_node: info.output_node,
                input_node: info.input_node,
                is_active: info.active,
                state: info.state,
            };
            state.graph.add_link(link);
        }
        PwEvent::LinkRemoved(id) => {
            state.graph.remove_link(id);
        }
        PwEvent::VolumeChanged(node_id, volume) => {
            state.graph.volumes.insert(*node_id, volume.clone());
        }
        PwEvent::MuteChanged(node_id, muted) => {
            if let Some(vol) = state.graph.volumes.get_mut(node_id) {
                vol.muted = *muted;
            }
        }
        PwEvent::MeterUpdate(updates) => {
            for update in updates {
                if let Some(meter) = state.graph.meters.get_mut(&update.node_id) {
                    meter.update(update.peak.clone(), update.rms.clone());
                }
            }
        }
        _ => {}
    }
}

/// Stub function when network feature is disabled.
#[cfg(not(feature = "network"))]
pub async fn run_headless(
    _bind_addr: std::net::SocketAddr,
    _token: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err("Network feature is not enabled. Rebuild with --features network".into())
}
