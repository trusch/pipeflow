//! PipeWire connection management.
//!
//! Handles the connection to the PipeWire daemon and thread management.
//! Provides deep integration including:
//! - Node proxy binding for param access
//! - Volume and mute control via SPA params
//! - Property change listeners for external updates
//! - Link state change monitoring

use crate::core::commands::AppCommand;
use crate::domain::audio::VolumeControl;
use crate::pipewire::events::{MeterUpdate, PwEvent};
use crate::pipewire::meter_stream::MeterStreamManager;
use crate::util::id::{LinkId, NodeId};
use crossbeam::channel::{bounded, Receiver, Sender};
use libspa::pod::Pod;
use pipewire::node::{Node, NodeListener};
use pipewire::properties::properties;
use pipewire::proxy::{ProxyListener, ProxyT};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::thread::JoinHandle;

// SPA Props key constants (from spa/param/props.h)
// SPA_PROP_START_Audio = 0x10000
const SPA_PROP_VOLUME: u32 = 0x10003;        // volume (Float)
const SPA_PROP_MUTE: u32 = 0x10004;          // mute (Bool)
const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008; // channelVolumes (Array of Float)

/// Channel bridge for communication between threads.
pub struct ChannelBridge {
    /// Send events from PipeWire thread to main thread
    pub event_tx: Sender<PwEvent>,
    /// Receive events on main thread
    pub event_rx: Receiver<PwEvent>,
    /// Send commands from main thread to PipeWire thread
    pub command_tx: Sender<AppCommand>,
    /// Receive commands on PipeWire thread
    pub command_rx: Receiver<AppCommand>,
}

/// Channel capacity for event queue.
/// Sized to handle typical bursts while providing backpressure under extreme load.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Channel capacity for command queue.
/// Commands are typically infrequent, so a smaller buffer suffices.
const COMMAND_CHANNEL_CAPACITY: usize = 64;

impl ChannelBridge {
    /// Creates a new channel bridge with bounded channels.
    pub fn new() -> Self {
        let (event_tx, event_rx) = bounded(EVENT_CHANNEL_CAPACITY);
        let (command_tx, command_rx) = bounded(COMMAND_CHANNEL_CAPACITY);

        Self {
            event_tx,
            event_rx,
            command_tx,
            command_rx,
        }
    }
}

impl Default for ChannelBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// PipeWire connection handler.
pub struct PwConnection {
    /// Thread handle for the PipeWire thread
    thread_handle: Option<JoinHandle<()>>,
    /// Channel for receiving events
    pub event_rx: Receiver<PwEvent>,
    /// Channel for sending commands
    pub command_tx: Sender<AppCommand>,
    /// Channel for receiving real meter updates
    pub meter_rx: Receiver<Vec<MeterUpdate>>,
    /// Whether the connection should be running
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl PwConnection {
    /// Creates a new PipeWire connection and spawns the worker thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the PipeWire thread cannot be spawned (e.g., resource exhaustion).
    pub fn new() -> Result<Self, std::io::Error> {
        let bridge = ChannelBridge::new();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Create a bounded channel for meter updates (moderate buffer to handle bursts)
        let (meter_tx, meter_rx) = bounded::<Vec<MeterUpdate>>(16);

        let event_tx = bridge.event_tx.clone();
        let command_rx = bridge.command_rx.clone();
        let running_clone = running.clone();

        let thread_handle = std::thread::Builder::new()
            .name("pipewire".to_string())
            .spawn(move || {
                run_pipewire_thread(event_tx, command_rx, meter_tx, running_clone);
            })?;

        Ok(Self {
            thread_handle: Some(thread_handle),
            event_rx: bridge.event_rx,
            command_tx: bridge.command_tx,
            meter_rx,
            running,
        })
    }

    /// Stops the connection.
    pub fn stop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Send disconnect command to wake up the thread
        let _ = self.command_tx.send(AppCommand::Disconnect);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Drains all pending events (non-blocking).
    pub fn drain_events(&self) -> Vec<PwEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Drains all pending real meter updates (non-blocking).
    pub fn drain_meter_updates(&self) -> Vec<Vec<MeterUpdate>> {
        let mut updates = Vec::new();
        while let Ok(batch) = self.meter_rx.try_recv() {
            updates.push(batch);
        }
        updates
    }
}

impl Drop for PwConnection {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Handle for a bound node proxy and its listeners.
/// Both the proxy and listeners must be kept alive for the connection to work.
struct NodeProxyHandle {
    /// The bound Node proxy (kept alive for volume/mute control)
    #[allow(dead_code)]
    node: Node,
    /// Node event listener (for info/param callbacks)
    _node_listener: NodeListener,
    /// Proxy lifecycle listener (for removed callback)
    _proxy_listener: ProxyListener,
}

/// Runtime state for the PipeWire thread.
/// Tracks created objects so they can be managed later.
struct PwRuntimeState {
    /// Links we've created (so we can destroy them)
    created_links: HashMap<LinkId, pipewire::link::Link>,
    /// Next link ID for links we create
    next_link_id: u32,
    /// Bound node proxies for volume/mute control
    node_proxies: HashMap<NodeId, NodeProxyHandle>,
}

impl PwRuntimeState {
    fn new() -> Self {
        Self {
            created_links: HashMap::new(),
            // Start at a high ID to avoid conflicts with PipeWire's IDs
            next_link_id: 1_000_000,
            node_proxies: HashMap::new(),
        }
    }

    /// Checks if a node proxy is bound.
    fn has_node_proxy(&self, node_id: &NodeId) -> bool {
        self.node_proxies.contains_key(node_id)
    }

    /// Removes a node proxy.
    fn remove_node_proxy(&mut self, node_id: &NodeId) {
        self.node_proxies.remove(node_id);
    }
}

/// Initial delay between reconnection attempts (milliseconds).
const INITIAL_RECONNECT_DELAY_MS: u64 = 1000;

/// Maximum delay between reconnection attempts (milliseconds).
const MAX_RECONNECT_DELAY_MS: u64 = 30000;

/// Runs the PipeWire thread with auto-reconnect support.
/// Never gives up -- keeps retrying with exponential backoff until stopped.
fn run_pipewire_thread(
    event_tx: Sender<PwEvent>,
    command_rx: Receiver<AppCommand>,
    meter_tx: Sender<Vec<MeterUpdate>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // Initialize PipeWire once
    pipewire::init();
    tracing::info!("PipeWire initialized");

    let mut attempt = 0u32;
    let mut delay_ms = INITIAL_RECONNECT_DELAY_MS;

    // Outer reconnection loop -- never gives up
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        // Try to connect
        match try_connect_and_run(&event_tx, &command_rx, &meter_tx, &running) {
            Ok(()) => {
                // Connection ended normally (disconnect command)
                if running.load(std::sync::atomic::Ordering::SeqCst) {
                    // We're still supposed to be running, so this was an unexpected disconnect
                    let _ = event_tx.send(PwEvent::Disconnected);
                    attempt = 0;
                    delay_ms = INITIAL_RECONNECT_DELAY_MS;
                } else {
                    // We were told to stop, exit the loop
                    break;
                }
            }
            Err(e) => {
                tracing::error!("PipeWire connection failed: {}", e);
            }
        }

        // Check if we should try to reconnect
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        attempt += 1;

        // Notify about reconnection attempt
        tracing::info!("Reconnecting to PipeWire (attempt {})", attempt);
        let _ = event_tx.send(PwEvent::Reconnecting {
            attempt,
            max_attempts: 0, // 0 = unlimited
        });

        // Wait before retry with exponential backoff (caps at MAX_RECONNECT_DELAY_MS)
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        delay_ms = (delay_ms * 2).min(MAX_RECONNECT_DELAY_MS);

        // Drain any pending commands during reconnect wait
        while command_rx.try_recv().is_ok() {}
    }

    tracing::info!("PipeWire thread shutting down");
}

/// Attempts to connect to PipeWire and run the main loop.
/// Returns Ok(()) when the main loop exits normally, Err on connection failure.
fn try_connect_and_run(
    event_tx: &Sender<PwEvent>,
    command_rx: &Receiver<AppCommand>,
    meter_tx: &Sender<Vec<MeterUpdate>>,
    running: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    // Create main loop
    let mainloop = pipewire::main_loop::MainLoop::new(None)
        .map_err(|e| format!("Failed to create main loop: {}", e))?;

    // Create context
    let context = pipewire::context::Context::new(&mainloop)
        .map_err(|e| format!("Failed to create context: {}", e))?;

    // Connect to the PipeWire daemon
    let core = context.connect(None)
        .map_err(|e| format!("Failed to connect to PipeWire: {}", e))?;

    tracing::info!("Connected to PipeWire");
    let _ = event_tx.send(PwEvent::Connected);

    // Get the registry (wrapped in Rc for sharing with callbacks)
    let registry = Rc::new(core.get_registry()
        .map_err(|e| format!("Failed to get registry: {}", e))?);

    // Runtime state for tracking created objects
    let state = Rc::new(RefCell::new(PwRuntimeState::new()));

    // Meter stream manager for real-time audio level monitoring
    let meter_manager = Rc::new(RefCell::new(MeterStreamManager::new(meter_tx.clone())));

    // Set up registry listener
    let event_tx_for_global = event_tx.clone();
    let event_tx_for_remove = event_tx.clone();
    let state_for_global = state.clone();
    let state_for_remove = state.clone();
    let meter_manager_for_global = meter_manager.clone();
    let meter_manager_for_remove = meter_manager.clone();
    let core_for_global = core.clone();
    let registry_weak = Rc::downgrade(&registry);
    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            // Register node serials for metering (only for audio nodes, not our own meter streams)
            if global.type_ == pipewire::types::ObjectType::Node {
                if let Some(props) = global.props {
                    // Get node name to filter out our own meter streams
                    let node_name = props.get("node.name").unwrap_or("");
                    let media_class = props.get("media.class").unwrap_or("");

                    // Skip our own meter streams
                    if node_name.starts_with("pipeflow-meter") {
                        tracing::trace!("Skipping our own meter node: {}", node_name);
                    } else {
                        // Only process audio-related nodes
                        let is_audio = media_class.contains("Audio")
                            || media_class.contains("Stream");

                        if is_audio {
                            let node_id = NodeId::new(global.id);

                            // Use object.serial if available, otherwise fallback to node ID
                            // This ensures all audio nodes can be metered, even those without a serial
                            let target_id = props.get("object.serial")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| global.id.to_string());

                            // Determine if this is a sink node (receives audio) vs source (outputs audio)
                            // Sinks: Audio/Sink, Stream/Input/Audio
                            // Sources: Audio/Source, Stream/Output/Audio (like SuperCollider)
                            let is_sink = media_class.contains("Sink")
                                || media_class.contains("Input");

                            // Register for metering
                            meter_manager_for_global
                                .borrow_mut()
                                .register_and_auto_meter(&core_for_global, node_id, target_id, is_sink);
                            tracing::trace!("Registered audio node for metering: {} ({}, is_sink={})", node_name, media_class, is_sink);

                            // Bind node proxy for volume/mute control
                            if let Some(registry) = registry_weak.upgrade() {
                                bind_node_proxy(
                                    &registry,
                                    &state_for_global,
                                    &event_tx_for_global,
                                    global,
                                    node_id,
                                );
                            }
                        }
                    }
                }
            }
            handle_global_added(&event_tx_for_global, global);
        })
        .global_remove(move |id| {
            // Clean up node proxy when removed
            let node_id = NodeId::new(id);
            {
                let mut state = state_for_remove.borrow_mut();
                state.remove_node_proxy(&node_id);
            }
            // Clean up meter manager
            meter_manager_for_remove.borrow_mut().unregister_node(&node_id);
            handle_global_removed(&event_tx_for_remove, id);
        })
        .register();

    // Main loop - run until stopped
    let loop_ = mainloop.loop_();
    let mut meter_collect_counter = 0u32;
    let mut stale_check_counter = 0u32;
    let stale_threshold = std::time::Duration::from_secs(5);
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        // Process one iteration of the main loop (non-blocking)
        loop_.iterate(std::time::Duration::from_millis(16)); // ~60 Hz

        // Check for commands
        while let Ok(command) = command_rx.try_recv() {
            handle_command(
                &core,
                &registry,
                &mut state.borrow_mut(),
                &mut meter_manager.borrow_mut(),
                event_tx,
                command,
            );
        }

        // Collect meter updates every ~2 iterations (~30 Hz)
        meter_collect_counter += 1;
        if meter_collect_counter >= 2 {
            meter_collect_counter = 0;
            meter_manager.borrow_mut().collect_and_send_updates();
        }

        // Check for stale meter streams every ~5 seconds (~300 iterations at 60Hz)
        stale_check_counter += 1;
        if stale_check_counter >= 300 {
            stale_check_counter = 0;
            meter_manager.borrow_mut().restart_stale_streams(&core, stale_threshold);
        }
    }

    Ok(())
}

/// Handles a global object being added.
fn handle_global_added(event_tx: &Sender<PwEvent>, global: &pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef>) {
    let props: std::collections::HashMap<String, String> = global
        .props
        .map(|p| {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let event = match global.type_ {
        pipewire::types::ObjectType::Node => {
            let info = crate::pipewire::events::NodeInfo::from_properties(global.id, &props);
            PwEvent::NodeAdded(info)
        }
        pipewire::types::ObjectType::Port => {
            let info = crate::pipewire::events::PortInfo::from_properties(global.id, &props);
            PwEvent::PortAdded(info)
        }
        pipewire::types::ObjectType::Link => {
            let info = crate::pipewire::events::LinkInfo::from_properties(global.id, &props);
            PwEvent::LinkAdded(info)
        }
        pipewire::types::ObjectType::Client => {
            let info = crate::pipewire::events::ClientInfo::from_properties(global.id, &props);
            PwEvent::ClientAdded(info)
        }
        pipewire::types::ObjectType::Device => {
            let info = crate::pipewire::events::DeviceInfo::from_properties(global.id, &props);
            PwEvent::DeviceAdded(info)
        }
        _ => return, // Ignore other types
    };

    let _ = event_tx.send(event);
}

/// Binds a node proxy for volume/mute control.
/// This subscribes to Props parameter changes and receives volume/mute updates.
fn bind_node_proxy(
    registry: &pipewire::registry::Registry,
    state: &Rc<RefCell<PwRuntimeState>>,
    event_tx: &Sender<PwEvent>,
    global: &pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef>,
    node_id: NodeId,
) {
    // Check if already bound
    if state.borrow().has_node_proxy(&node_id) {
        return;
    }

    // Bind the node proxy
    let node: Node = match registry.bind(global) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("Failed to bind node proxy for {:?}: {:?}", node_id, e);
            return;
        }
    };

    // Set up node listener for param events
    let event_tx_for_param = event_tx.clone();
    let node_id_for_param = node_id;
    let node_listener = node
        .add_listener_local()
        .param(move |_seq, param_type, _index, _next, param| {
            // Only process Props parameters
            if param_type != libspa::param::ParamType::Props {
                return;
            }

            if let Some(pod) = param {
                parse_props_pod(pod, node_id_for_param, &event_tx_for_param);
            }
        })
        .register();

    // Subscribe to Props parameter changes to receive volume/mute updates
    node.subscribe_params(&[libspa::param::ParamType::Props]);

    // Enumerate current Props to get initial values
    node.enum_params(0, Some(libspa::param::ParamType::Props), 0, u32::MAX);

    // Set up proxy listener for removal
    let state_weak = Rc::downgrade(state);
    let node_id_for_remove = node_id;
    let proxy_listener = node
        .upcast_ref()
        .add_listener_local()
        .removed(move || {
            if let Some(state) = state_weak.upgrade() {
                state.borrow_mut().remove_node_proxy(&node_id_for_remove);
                tracing::debug!("Node proxy removed for {:?}", node_id_for_remove);
            }
        })
        .register();

    // Store the proxy and listeners
    let handle = NodeProxyHandle {
        node,
        _node_listener: node_listener,
        _proxy_listener: proxy_listener,
    };
    state.borrow_mut().node_proxies.insert(node_id, handle);
    tracing::debug!("Bound node proxy for {:?}", node_id);
}

/// Parses a Props pod to extract volume and mute values.
fn parse_props_pod(pod: &Pod, node_id: NodeId, event_tx: &Sender<PwEvent>) {
    use libspa::pod::deserialize::PodDeserializer;
    use libspa::pod::{Property, Value, ValueArray};

    // Parse the pod as an object
    let value: Value = match PodDeserializer::deserialize_from(pod.as_bytes()) {
        Ok((_, v)) => v,
        Err(e) => {
            tracing::trace!("Failed to deserialize Props pod: {:?}", e);
            return;
        }
    };

    let properties = match value {
        Value::Object(obj) => obj.properties,
        _ => return,
    };

    let mut volume: Option<f32> = None;
    let mut muted: Option<bool> = None;
    let mut channel_volumes: Option<Vec<f32>> = None;

    for prop in properties {
        match prop {
            Property { key, value: Value::Float(v), .. } if key == SPA_PROP_VOLUME => {
                volume = Some(v);
            }
            Property { key, value: Value::Bool(m), .. } if key == SPA_PROP_MUTE => {
                muted = Some(m);
            }
            Property { key, value: Value::ValueArray(ValueArray::Float(arr)), .. }
                if key == SPA_PROP_CHANNEL_VOLUMES =>
            {
                channel_volumes = Some(arr);
            }
            _ => {}
        }
    }

    // Emit volume/mute events if we got values
    if volume.is_some() || channel_volumes.is_some() || muted.is_some() {
        let mut vol_control = VolumeControl::default();

        if let Some(v) = volume {
            vol_control.master = v;
        }
        if let Some(ch) = channel_volumes {
            // Use channel volumes as master (average or first channel)
            if !ch.is_empty() {
                // Use .max(1) as defensive guard against empty slice (should never happen due to check above)
                vol_control.master = ch.iter().sum::<f32>() / ch.len().max(1) as f32;
                vol_control.channels = ch;
            }
        }
        if let Some(m) = muted {
            vol_control.muted = m;
        }

        tracing::trace!(
            "Props update for {:?}: volume={:.3}, muted={}, channels={:?}",
            node_id,
            vol_control.master,
            vol_control.muted,
            vol_control.channels
        );

        let _ = event_tx.send(PwEvent::VolumeChanged(node_id, vol_control));
    }
}

/// Sets volume on a node via wpctl (system-integrated).
///
/// This uses wpctl to set the volume, which integrates with WirePlumber's
/// metadata system. This ensures that volume changes are visible in system
/// settings (pavucontrol, KDE Plasma, GNOME Settings, etc.) and are properly
/// synchronized rather than multiplicative.
/// Attempts to set volume using wpctl (WirePlumber).
/// Returns true if successful.
fn try_wpctl_volume(id: u32, vol: f32) -> Result<(), String> {
    match std::process::Command::new("wpctl")
        .args(["set-volume", &id.to_string(), &format!("{:.4}", vol)])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("wpctl failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("wpctl not available: {}", e)),
    }
}

/// Attempts to set volume using pw-cli (direct PipeWire).
/// Returns true if successful.
fn try_pwcli_volume(id: u32, vol: f32) -> Result<(), String> {
    // pw-cli set-param <node_id> Props '{ volume: <value> }'
    let props = format!("{{ volume: {} }}", vol);
    match std::process::Command::new("pw-cli")
        .args(["set-param", &id.to_string(), "Props", &props])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("pw-cli failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("pw-cli not available: {}", e)),
    }
}

/// Attempts to set volume using pactl (PulseAudio compatibility).
/// Returns true if successful.
fn try_pactl_volume(id: u32, vol: f32) -> Result<(), String> {
    // pactl uses percentage (0-100) or absolute values
    // Convert linear volume to percentage
    let percent = (vol * 100.0).round() as u32;
    match std::process::Command::new("pactl")
        .args(["set-sink-input-volume", &id.to_string(), &format!("{}%", percent)])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                // pactl might fail for non-sink-inputs, that's expected
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("pactl failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("pactl not available: {}", e)),
    }
}

fn set_node_volume(node_id: &NodeId, volume: &VolumeControl, event_tx: &Sender<PwEvent>) {
    // Use the first channel volume, or master if channels is empty
    let vol = if volume.channels.is_empty() {
        volume.master
    } else {
        // wpctl set-volume sets all channels uniformly
        // For per-channel control, we'd need multiple calls or different approach
        volume.channels[0]
    };

    tracing::info!(
        "set_node_volume for {}: volume={:.3}",
        node_id.raw(),
        vol
    );

    VolumeWorker::get_or_init().send(
        VolumeCommand::SetVolume { node_id: *node_id, volume: vol },
        event_tx.clone(),
    );
}

/// Attempts to set mute state using wpctl (WirePlumber).
fn try_wpctl_mute(id: u32, muted: bool) -> Result<(), String> {
    let mute_arg = if muted { "1" } else { "0" };
    match std::process::Command::new("wpctl")
        .args(["set-mute", &id.to_string(), mute_arg])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("wpctl failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("wpctl not available: {}", e)),
    }
}

/// Attempts to set mute state using pw-cli (direct PipeWire).
fn try_pwcli_mute(id: u32, muted: bool) -> Result<(), String> {
    // pw-cli set-param <node_id> Props '{ mute: true/false }'
    let props = format!("{{ mute: {} }}", muted);
    match std::process::Command::new("pw-cli")
        .args(["set-param", &id.to_string(), "Props", &props])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("pw-cli failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("pw-cli not available: {}", e)),
    }
}

/// Attempts to set mute state using pactl (PulseAudio compatibility).
fn try_pactl_mute(id: u32, muted: bool) -> Result<(), String> {
    let mute_arg = if muted { "1" } else { "0" };
    match std::process::Command::new("pactl")
        .args(["set-sink-input-mute", &id.to_string(), mute_arg])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("pactl failed: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("pactl not available: {}", e)),
    }
}

/// Command for the volume control worker thread.
enum VolumeCommand {
    SetVolume { node_id: NodeId, volume: f32 },
    SetMute { node_id: NodeId, muted: bool },
}

/// Capacity for the volume command channel.
/// Small buffer provides backpressure during rapid UI interactions.
const VOLUME_COMMAND_CAPACITY: usize = 8;

/// Global volume worker handle (lazy initialized).
static VOLUME_WORKER: std::sync::OnceLock<VolumeWorker> = std::sync::OnceLock::new();

/// Handle to the volume control worker thread.
struct VolumeWorker {
    tx: Sender<(VolumeCommand, Sender<PwEvent>)>,
}

impl VolumeWorker {
    fn get_or_init() -> &'static VolumeWorker {
        VOLUME_WORKER.get_or_init(|| {
            let (tx, rx) = bounded::<(VolumeCommand, Sender<PwEvent>)>(VOLUME_COMMAND_CAPACITY);

            // Spawn worker thread
            if let Err(e) = std::thread::Builder::new()
                .name("volume-control".to_string())
                .spawn(move || {
                    volume_worker_thread(rx);
                })
            {
                tracing::error!("Failed to spawn volume worker thread: {}", e);
            }

            VolumeWorker { tx }
        })
    }

    fn send(&self, cmd: VolumeCommand, event_tx: Sender<PwEvent>) {
        // Use try_send to avoid blocking; drop command if queue is full
        if let Err(e) = self.tx.try_send((cmd, event_tx)) {
            tracing::debug!("Volume command dropped (queue full or disconnected): {:?}", e);
        }
    }
}

/// Worker thread that processes volume/mute commands sequentially.
fn volume_worker_thread(rx: Receiver<(VolumeCommand, Sender<PwEvent>)>) {
    tracing::debug!("Volume worker thread started");

    while let Ok((cmd, event_tx)) = rx.recv() {
        match cmd {
            VolumeCommand::SetVolume { node_id, volume } => {
                execute_set_volume(node_id, volume, &event_tx);
            }
            VolumeCommand::SetMute { node_id, muted } => {
                execute_set_mute(node_id, muted, &event_tx);
            }
        }
    }

    tracing::debug!("Volume worker thread exited");
}

/// Executes volume change with fallback chain.
fn execute_set_volume(node_id: NodeId, vol: f32, event_tx: &Sender<PwEvent>) {
    let id = node_id.raw();

    if let Err(e1) = try_wpctl_volume(id, vol) {
        tracing::debug!("wpctl volume failed: {}", e1);

        if let Err(e2) = try_pwcli_volume(id, vol) {
            tracing::debug!("pw-cli volume failed: {}", e2);

            if let Err(e3) = try_pactl_volume(id, vol) {
                tracing::warn!("All volume control methods failed for node {}: {}", id, e3);
                let _ = event_tx.send(PwEvent::VolumeControlFailed(
                    node_id,
                    format!("Volume control not supported: {}", e3),
                ));
            } else {
                tracing::info!("Volume set via pactl for node {}", id);
            }
        } else {
            tracing::info!("Volume set via pw-cli for node {}", id);
        }
    } else {
        tracing::debug!("Volume set via wpctl for node {}", id);
    }
}

/// Executes mute change with fallback chain.
fn execute_set_mute(node_id: NodeId, muted: bool, event_tx: &Sender<PwEvent>) {
    let id = node_id.raw();

    if let Err(e1) = try_wpctl_mute(id, muted) {
        tracing::debug!("wpctl mute failed: {}", e1);

        if let Err(e2) = try_pwcli_mute(id, muted) {
            tracing::debug!("pw-cli mute failed: {}", e2);

            if let Err(e3) = try_pactl_mute(id, muted) {
                tracing::warn!("All mute control methods failed for node {}: {}", id, e3);
                let _ = event_tx.send(PwEvent::VolumeControlFailed(
                    node_id,
                    format!("Mute control not supported: {}", e3),
                ));
            } else {
                tracing::info!("Mute set via pactl for node {}", id);
            }
        } else {
            tracing::info!("Mute set via pw-cli for node {}", id);
        }
    } else {
        tracing::debug!("Mute set via wpctl for node {}", id);
    }
}

/// Sets mute state on a node with fallback chain (wpctl -> pw-cli -> pactl).
fn set_node_mute(node_id: &NodeId, muted: bool, event_tx: &Sender<PwEvent>) {
    tracing::info!("set_node_mute for {}: muted={}", node_id.raw(), muted);

    VolumeWorker::get_or_init().send(
        VolumeCommand::SetMute { node_id: *node_id, muted },
        event_tx.clone(),
    );
}

/// Handles a global object being removed.
fn handle_global_removed(event_tx: &Sender<PwEvent>, id: u32) {
    // We don't know what type was removed, so we send removal events for all types
    // The application will ignore removals for IDs it doesn't have
    let _ = event_tx.send(PwEvent::NodeRemoved(crate::util::id::NodeId::new(id)));
    let _ = event_tx.send(PwEvent::PortRemoved(crate::util::id::PortId::new(id)));
    let _ = event_tx.send(PwEvent::LinkRemoved(crate::util::id::LinkId::new(id)));
}

/// Handles a command from the main thread.
fn handle_command(
    core: &pipewire::core::Core,
    registry: &pipewire::registry::Registry,
    state: &mut PwRuntimeState,
    meter_manager: &mut MeterStreamManager,
    event_tx: &Sender<PwEvent>,
    command: AppCommand,
) {
    match command {
        AppCommand::CreateLink { output_port, input_port } => {
            tracing::info!("Creating link: {:?} -> {:?}", output_port, input_port);

            // Create the link using the link factory
            let props = properties! {
                "link.output.port" => output_port.raw().to_string(),
                "link.input.port" => input_port.raw().to_string(),
                "object.linger" => "true",
            };

            match core.create_object::<pipewire::link::Link>("link-factory", &props) {
                Ok(link) => {
                    let link_id = LinkId::new(state.next_link_id);
                    state.next_link_id += 1;
                    tracing::info!("Link created successfully with internal ID {:?}", link_id);
                    state.created_links.insert(link_id, link);
                }
                Err(e) => {
                    tracing::error!("Failed to create link: {}", e);
                    let _ = event_tx.send(PwEvent::Error(crate::pipewire::events::PwError {
                        code: -1,
                        message: format!("Failed to create link: {}", e),
                    }));
                }
            }
        }
        AppCommand::RemoveLink(link_id) => {
            tracing::info!("Removing link: {:?}", link_id);

            // Check if this is a link we created
            if let Some(_link) = state.created_links.remove(&link_id) {
                // The link will be destroyed when dropped
                tracing::info!("Link {:?} destroyed (self-created)", link_id);
            } else {
                // For external links, use registry.destroy_global
                // The link_id.raw() is the PipeWire global ID
                let global_id = link_id.raw();
                tracing::info!("Destroying external link with global ID {}", global_id);
                registry.destroy_global(global_id);
                tracing::info!("Destroy command sent for link {:?}", link_id);
            }
        }
        AppCommand::ToggleLink { link_id, active } => {
            tracing::info!("Toggle link {:?}: active={}", link_id, active);

            // PipeWire doesn't directly support enabling/disabling links.
            // The visual toggle is UI-only for now.
            // A full implementation would:
            // 1. Remove the link when disabling (store the endpoints)
            // 2. Recreate the link when re-enabling
            //
            // The UI state is already updated locally in app.rs before this command is sent.
        }
        AppCommand::SetVolume { node_id, volume } => {
            tracing::debug!("Setting volume for {:?}: {:.2}", node_id, volume.master);

            // Apply volume change via wpctl (system-integrated)
            // wpctl will update WirePlumber metadata, which syncs with system settings
            set_node_volume(&node_id, &volume, event_tx);

            // Also emit event for immediate UI response
            // (PipeWire callback will follow with confirmed value from WirePlumber)
            let _ = event_tx.send(PwEvent::VolumeChanged(node_id, volume));
        }
        AppCommand::SetMute { node_id, muted } => {
            tracing::debug!("Setting mute for {:?}: {}", node_id, muted);

            // Apply mute change via wpctl (system-integrated)
            // wpctl will update WirePlumber metadata, which syncs with system settings
            set_node_mute(&node_id, muted, event_tx);

            // Also emit event for immediate UI response
            // (PipeWire callback will follow with confirmed value from WirePlumber)
            let _ = event_tx.send(PwEvent::MuteChanged(node_id, muted));
        }
        AppCommand::SetChannelVolume { node_id, channel, volume } => {
            tracing::debug!("Setting channel {} volume for {:?}: {:.2}", channel, node_id, volume);

            // For per-channel volume, we create a VolumeControl with the channel set
            // If we need to set a specific channel, we'd need to get current channels first
            // For now, just set all channels to the same value
            let vol_control = VolumeControl {
                master: volume,
                channels: vec![volume],
                ..VolumeControl::default()
            };
            set_node_volume(&node_id, &vol_control, event_tx);

            let _ = event_tx.send(PwEvent::VolumeChanged(node_id, vol_control));
        }
        AppCommand::Disconnect => {
            tracing::info!("Disconnect requested");
        }
        AppCommand::StartAllMeters => {
            tracing::info!("Starting meters for all registered nodes");
            // Clean up any stale meters first (handles incomplete cleanup from node removal)
            meter_manager.cleanup_stale_meters();
            // Enable auto-metering for new nodes
            meter_manager.set_auto_meter_all(true);
            // Get all registered node IDs and start metering them
            let node_ids = meter_manager.registered_node_ids();
            for node_id in node_ids {
                meter_manager.start_metering(core, node_id);
            }
            tracing::info!("Started {} meter streams", meter_manager.active_count());
        }
        AppCommand::StopAllMeters => {
            tracing::info!("Stopping all meter streams");
            // Disable auto-metering
            meter_manager.set_auto_meter_all(false);
            meter_manager.stop_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_bridge() {
        let bridge = ChannelBridge::new();

        // Test event channel
        bridge.event_tx.send(PwEvent::Connected).unwrap();
        let event = bridge.event_rx.recv().unwrap();
        assert!(matches!(event, PwEvent::Connected));

        // Test command channel
        bridge.command_tx.send(AppCommand::Disconnect).unwrap();
        let cmd = bridge.command_rx.recv().unwrap();
        assert!(matches!(cmd, AppCommand::Disconnect));
    }
}
