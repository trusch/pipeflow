//! gRPC server implementation for headless mode.
//!
//! Provides the Pipeflow gRPC service that allows remote clients
//! to monitor and control PipeWire.

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::core::commands::CommandHandler;
use crate::core::state::SharedState;
use crate::domain::safety::SafetyMode;
use crate::pipewire::events::{MeterUpdate, PwEvent};

use super::adapter::{command_from_proto, event_to_proto, meter_batch_to_proto, state_to_proto};
use super::proto;
use super::proto::pipeflow_server::{Pipeflow, PipeflowServer};
use super::PROTOCOL_VERSION;

/// gRPC server for remote pipeflow access.
#[derive(Clone)]
pub struct GrpcServer {
    /// Server address
    addr: SocketAddr,
    /// Shared application state
    state: SharedState,
    /// Command handler for sending commands to PipeWire
    command_handler: Arc<CommandHandler>,
    /// Authentication token (optional)
    token: Option<String>,
    /// Event broadcast channel
    event_tx: broadcast::Sender<proto::Event>,
    /// Meter broadcast channel
    meter_tx: broadcast::Sender<proto::MeterBatch>,
    /// Sequence counter for events
    sequence: Arc<AtomicU64>,
    /// Server start time for timestamps
    start_time: Instant,
}

impl GrpcServer {
    /// Creates a new gRPC server.
    pub fn new(
        addr: SocketAddr,
        state: SharedState,
        command_handler: Arc<CommandHandler>,
        token: Option<String>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (meter_tx, _) = broadcast::channel(64);

        Self {
            addr,
            state,
            command_handler,
            token,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        }
    }

    /// Broadcasts a PwEvent to all connected clients.
    pub fn broadcast_event(&self, event: &PwEvent) {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let timestamp_ms = self.start_time.elapsed().as_millis() as u64;

        if let Some(proto_event) = event_to_proto(event, seq, timestamp_ms) {
            // Ignore send errors (no subscribers)
            let _ = self.event_tx.send(proto_event);
        }
    }

    /// Broadcasts meter updates to all connected clients.
    pub fn broadcast_meters(&self, updates: &[MeterUpdate]) {
        if updates.is_empty() {
            return;
        }

        let timestamp_ms = self.start_time.elapsed().as_millis() as u32;
        let batch = meter_batch_to_proto(updates, timestamp_ms);

        // Ignore send errors (no subscribers)
        let _ = self.meter_tx.send(batch);
    }

    /// Runs the gRPC server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.addr;
        let service = PipeflowService {
            state: self.state.clone(),
            command_handler: self.command_handler.clone(),
            token: self.token.clone(),
            event_tx: self.event_tx.clone(),
            meter_tx: self.meter_tx.clone(),
            sequence: self.sequence.clone(),
        };

        tracing::info!("Starting gRPC server on {}", addr);

        tonic::transport::Server::builder()
            .add_service(PipeflowServer::new(service))
            .serve(addr)
            .await?;

        Ok(())
    }
}

/// The gRPC service implementation.
struct PipeflowService {
    state: SharedState,
    command_handler: Arc<CommandHandler>,
    token: Option<String>,
    event_tx: broadcast::Sender<proto::Event>,
    meter_tx: broadcast::Sender<proto::MeterBatch>,
    sequence: Arc<AtomicU64>,
}

#[tonic::async_trait]
impl Pipeflow for PipeflowService {
    async fn authenticate(
        &self,
        request: Request<proto::ConnectRequest>,
    ) -> Result<Response<proto::ConnectResponse>, Status> {
        let req = request.into_inner();

        // Check protocol version
        if req.protocol_version != 0 && req.protocol_version != PROTOCOL_VERSION {
            return Ok(Response::new(proto::ConnectResponse {
                accepted: false,
                protocol_version: PROTOCOL_VERSION,
                error: format!(
                    "Protocol version mismatch: client={}, server={}",
                    req.protocol_version, PROTOCOL_VERSION
                ),
                server_id: String::new(),
            }));
        }

        // Validate token if required
        if let Some(ref expected_token) = self.token {
            if req.token != *expected_token {
                return Ok(Response::new(proto::ConnectResponse {
                    accepted: false,
                    protocol_version: PROTOCOL_VERSION,
                    error: "Invalid authentication token".to_string(),
                    server_id: String::new(),
                }));
            }
        }

        tracing::info!("Client connected: {}", req.client_id);

        Ok(Response::new(proto::ConnectResponse {
            accepted: true,
            protocol_version: PROTOCOL_VERSION,
            error: String::new(),
            server_id: format!("pipeflow-{}", std::process::id()),
        }))
    }

    async fn get_state(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::GraphState>, Status> {
        let state = self.state.read();
        let seq = self.sequence.load(Ordering::SeqCst);
        let proto_state = state_to_proto(&state, seq);

        Ok(Response::new(proto_state))
    }

    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<proto::Event, Status>> + Send + 'static>>;

    async fn subscribe_events(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let _req = request.into_inner();
        let mut rx = self.event_tx.subscribe();

        let (tx, rx_out) = mpsc::channel(128);

        // Spawn a task to forward events
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if tx.send(Ok(event)).await.is_err() {
                            // Client disconnected
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Event subscriber lagged by {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx_out))))
    }

    type SubscribeMetersStream =
        Pin<Box<dyn Stream<Item = Result<proto::MeterBatch, Status>> + Send + 'static>>;

    async fn subscribe_meters(
        &self,
        request: Request<proto::MeterSubscription>,
    ) -> Result<Response<Self::SubscribeMetersStream>, Status> {
        let subscription = request.into_inner();
        let mut rx = self.meter_tx.subscribe();

        let (tx, rx_out) = mpsc::channel(32);

        // Apply rate limiting if requested
        let max_rate_hz = subscription.max_rate_hz;
        let min_interval = if max_rate_hz > 0 {
            Duration::from_secs_f64(1.0 / max_rate_hz as f64)
        } else {
            Duration::ZERO
        };

        // Spawn a task to forward meter updates
        tokio::spawn(async move {
            let mut last_send = Instant::now();

            loop {
                match rx.recv().await {
                    Ok(batch) => {
                        // Apply rate limiting
                        if min_interval > Duration::ZERO {
                            let elapsed = last_send.elapsed();
                            if elapsed < min_interval {
                                continue;
                            }
                        }

                        if tx.send(Ok(batch)).await.is_err() {
                            // Client disconnected
                            break;
                        }
                        last_send = Instant::now();
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Meter data can be dropped without issue
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx_out))))
    }

    async fn execute_command(
        &self,
        request: Request<proto::Command>,
    ) -> Result<Response<proto::CommandResult>, Status> {
        let cmd = request.into_inner();

        // Handle safety mode and routing lock separately (they modify local state)
        if let Some(ref command) = cmd.command {
            match command {
                proto::command::Command::SetSafetyMode(req) => {
                    let mode =
                        SafetyMode::from(proto::SafetyMode::try_from(req.mode).unwrap_or_default());
                    let mut state = self.state.write();
                    state.safety.set_mode(mode);
                    return Ok(Response::new(proto::CommandResult {
                        success: true,
                        error: String::new(),
                    }));
                }
                proto::command::Command::ToggleRoutingLock(_) => {
                    // Routing lock feature removed - ignore command for backwards compatibility
                    return Ok(Response::new(proto::CommandResult {
                        success: true,
                        error: String::new(),
                    }));
                }
                _ => {}
            }
        }

        // Convert to AppCommand and execute
        let app_cmd = match command_from_proto(&cmd) {
            Some(c) => c,
            None => {
                return Ok(Response::new(proto::CommandResult {
                    success: false,
                    error: "Unknown command".to_string(),
                }));
            }
        };

        // Validate against safety controller
        let safety = {
            let state = self.state.read();
            state.safety.clone()
        };

        match self.command_handler.execute(app_cmd, &safety) {
            Ok(()) => Ok(Response::new(proto::CommandResult {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(proto::CommandResult {
                success: false,
                error: e.to_string(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::create_shared_state;
    use crate::pipewire::events::PwEvent;
    use crossbeam::channel;

    fn create_test_server() -> GrpcServer {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));

        GrpcServer::new("127.0.0.1:0".parse().unwrap(), state, command_handler, None)
    }

    fn create_test_server_with_token(token: &str) -> GrpcServer {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));

        GrpcServer::new(
            "127.0.0.1:0".parse().unwrap(),
            state,
            command_handler,
            Some(token.to_string()),
        )
    }

    // ========================================================================
    // Server creation tests
    // ========================================================================

    #[test]
    fn test_server_with_token() {
        let server = create_test_server_with_token("secret123");
        assert!(server.token.is_some());
        assert_eq!(server.token.as_ref().unwrap(), "secret123");
    }

    // ========================================================================
    // Event broadcasting tests
    // ========================================================================

    #[test]
    fn test_broadcast_event_increments_sequence() {
        let server = create_test_server();

        // Subscribe to receive events
        let mut rx = server.event_tx.subscribe();

        server.broadcast_event(&PwEvent::Connected);
        let event1 = rx.try_recv().unwrap();
        assert_eq!(event1.sequence, 0);

        server.broadcast_event(&PwEvent::Disconnected);
        let event2 = rx.try_recv().unwrap();
        assert_eq!(event2.sequence, 1);
    }

    #[test]
    fn test_broadcast_event_sets_timestamp() {
        let server = create_test_server();
        let mut rx = server.event_tx.subscribe();

        // Wait a tiny bit to ensure timestamp > 0
        std::thread::sleep(std::time::Duration::from_millis(1));

        server.broadcast_event(&PwEvent::Connected);
        let event = rx.try_recv().unwrap();

        // Timestamp should be > 0 since we waited
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn test_broadcast_event_connected() {
        let server = create_test_server();
        let mut rx = server.event_tx.subscribe();

        server.broadcast_event(&PwEvent::Connected);
        let event = rx.try_recv().unwrap();

        match event.event.unwrap() {
            proto::event::Event::ConnectionStatus(status) => {
                assert_eq!(status.state, proto::ConnectionState::Connected as i32);
            }
            _ => panic!("Expected ConnectionStatus event"),
        }
    }

    #[test]
    fn test_broadcast_event_no_subscribers() {
        let server = create_test_server();
        // No subscribers - this should not panic
        server.broadcast_event(&PwEvent::Connected);
    }

    // ========================================================================
    // Meter broadcasting tests
    // ========================================================================

    #[test]
    fn test_broadcast_meters_empty() {
        let server = create_test_server();
        let mut rx = server.meter_tx.subscribe();

        // Empty updates should not be broadcast
        server.broadcast_meters(&[]);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_broadcast_meters() {
        use crate::pipewire::events::MeterUpdate;
        use crate::util::id::NodeId;

        let server = create_test_server();
        let mut rx = server.meter_tx.subscribe();

        let updates = vec![MeterUpdate {
            node_id: NodeId::new(42),
            peak: vec![0.8, 0.7],
            rms: vec![0.5, 0.4],
        }];

        server.broadcast_meters(&updates);
        let batch = rx.try_recv().unwrap();

        assert_eq!(batch.entries.len(), 1);
        assert_eq!(batch.entries[0].node_id, 42);
        assert_eq!(batch.entries[0].peak, vec![0.8, 0.7]);
    }

    // ========================================================================
    // Authentication tests (using PipeflowService directly)
    // ========================================================================

    #[tokio::test]
    async fn test_authenticate_no_token_required() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: None, // No token required
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::ConnectRequest {
            protocol_version: PROTOCOL_VERSION,
            token: String::new(),
            client_id: "test-client".to_string(),
        });

        let response = service.authenticate(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.accepted);
        assert!(inner.error.is_empty());
        assert_eq!(inner.protocol_version, PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn test_authenticate_correct_token() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: Some("secret123".to_string()),
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::ConnectRequest {
            protocol_version: PROTOCOL_VERSION,
            token: "secret123".to_string(),
            client_id: "test-client".to_string(),
        });

        let response = service.authenticate(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.accepted);
    }

    #[tokio::test]
    async fn test_authenticate_wrong_token() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: Some("secret123".to_string()),
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::ConnectRequest {
            protocol_version: PROTOCOL_VERSION,
            token: "wrong_token".to_string(),
            client_id: "test-client".to_string(),
        });

        let response = service.authenticate(request).await.unwrap();
        let inner = response.into_inner();

        assert!(!inner.accepted);
        assert_eq!(inner.error, "Invalid authentication token");
    }

    #[tokio::test]
    async fn test_authenticate_wrong_protocol_version() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::ConnectRequest {
            protocol_version: 999, // Wrong version
            token: String::new(),
            client_id: "test-client".to_string(),
        });

        let response = service.authenticate(request).await.unwrap();
        let inner = response.into_inner();

        assert!(!inner.accepted);
        assert!(inner.error.contains("Protocol version mismatch"));
    }

    #[tokio::test]
    async fn test_authenticate_version_zero_accepted() {
        // Protocol version 0 should be accepted (for compatibility)
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::ConnectRequest {
            protocol_version: 0, // Version 0 should be accepted
            token: String::new(),
            client_id: "test-client".to_string(),
        });

        let response = service.authenticate(request).await.unwrap();
        assert!(response.into_inner().accepted);
    }

    // ========================================================================
    // GetState tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_state() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(42)),
        };

        let request = Request::new(proto::Empty {});
        let response = service.get_state(request).await.unwrap();
        let state = response.into_inner();

        // Initial state should be empty
        assert!(state.nodes.is_empty());
        assert!(state.ports.is_empty());
        assert!(state.links.is_empty());
        assert_eq!(state.sequence, 42);
    }

    // ========================================================================
    // ExecuteCommand tests
    // ========================================================================

    #[tokio::test]
    async fn test_execute_command_set_safety_mode() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state: state.clone(),
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::Command {
            command: Some(proto::command::Command::SetSafetyMode(
                proto::SetSafetyModeCommand {
                    mode: proto::SafetyMode::ReadOnly as i32,
                },
            )),
        });

        let response = service.execute_command(request).await.unwrap();
        assert!(response.into_inner().success);

        // Verify state was updated
        let state = state.read();
        assert_eq!(state.safety.mode, SafetyMode::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_command_toggle_routing_lock_is_noop() {
        // Routing lock feature was removed - command should succeed but do nothing
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state: state.clone(),
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::Command {
            command: Some(proto::command::Command::ToggleRoutingLock(
                proto::ToggleRoutingLockCommand {},
            )),
        });

        // Command should succeed for backwards compatibility
        let response = service.execute_command(request).await.unwrap();
        assert!(response.into_inner().success);
    }

    #[tokio::test]
    async fn test_execute_command_unknown() {
        let state = create_shared_state();
        let (tx, _rx) = channel::unbounded();
        let command_handler = Arc::new(CommandHandler::new(tx));
        let (event_tx, _) = broadcast::channel(16);
        let (meter_tx, _) = broadcast::channel(16);

        let service = PipeflowService {
            state,
            command_handler,
            token: None,
            event_tx,
            meter_tx,
            sequence: Arc::new(AtomicU64::new(0)),
        };

        let request = Request::new(proto::Command { command: None });
        let response = service.execute_command(request).await.unwrap();
        let result = response.into_inner();

        assert!(!result.success);
        assert_eq!(result.error, "Unknown command");
    }
}
