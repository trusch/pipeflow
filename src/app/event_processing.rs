//! PipeWire event processing.
//!
//! Handles events from PipeWire daemon including node/port/link lifecycle,
//! meter updates, and connection state changes.

use crate::core::commands::AppCommand;
use crate::core::state::ConnectionState;
use crate::domain::graph::{Link, Node, Port, PortDirection};
use crate::domain::rules::RuleTrigger;
use crate::pipewire::events::{MeterUpdate, PwEvent};
use crate::util::id::NodeId;
use crate::util::layout::{get_metering_target_id, is_metering_node, SmartLayout};
use crate::util::spatial::Position;

use super::command_handling::create_stable_identifier;
use super::PipeflowApp;

impl PipeflowApp {
    /// Processes meter updates from real PipeWire streams or remote connection.
    pub(super) fn process_meter_updates(&mut self) {
        if !self.config.meters.enabled && !self.is_remote {
            return;
        }

        let batches: Vec<Vec<MeterUpdate>> = if let Some(ref pw) = self.pw_connection {
            pw.drain_meter_updates()
        } else {
            #[cfg(feature = "network")]
            {
                if let Some(ref remote) = self.remote_connection {
                    remote.drain_meter_updates()
                } else {
                    Vec::new()
                }
            }
            #[cfg(not(feature = "network"))]
            {
                Vec::new()
            }
        };

        if !batches.is_empty() {
            let mut state = self.state.write();
            for batch in batches {
                for update in batch {
                    if let Some(meter) = state.graph.meters.get_mut(&update.node_id) {
                        meter.update(update.peak, update.rms);
                    }
                }
            }
        }
    }

    fn meter_activity_for_node_port(
        graph: &crate::core::state::GraphState,
        node_id: crate::util::id::NodeId,
        port_id: crate::util::id::PortId,
        stale_threshold: std::time::Duration,
    ) -> Option<(f32, bool)> {
        let meter = graph.meters.get(&node_id)?;
        if let Some(channel) = graph
            .get_port(&port_id)
            .and_then(|port| port.channel.map(|channel| channel as usize))
        {
            Some((meter.get_decayed_peak(channel, stale_threshold), true))
        } else {
            Some((meter.get_decayed_max_peak(stale_threshold), false))
        }
    }

    /// Updates link meter data based on meter activity from both ends of the link.
    ///
    /// Prefer exact per-port/per-channel readings when available. This avoids a common
    /// failure mode where changing one channel on a multi-output device makes all sibling
    /// links pulse because we fell back to coarse node-wide activity. Only when we lack
    /// channel information do we fall back to node-level meter activity.
    pub(super) fn update_link_meters(&mut self, dt: f32) {
        if !self.config.meters.enabled {
            return;
        }

        let dt = dt.clamp(1.0 / 240.0, 0.25);
        let refresh_hz = self.config.meters.refresh_rate.max(1) as u64;
        let refresh_interval_ms = 1000 / refresh_hz;
        let stale_threshold = std::time::Duration::from_millis((refresh_interval_ms * 4).max(120));

        let mut state = self.state.write();

        // Collect link updates (to avoid borrow issues)
        let updates: Vec<_> = state
            .graph
            .links
            .values()
            .filter(|link| link.is_active)
            .map(|link| {
                let source = Self::meter_activity_for_node_port(
                    &state.graph,
                    link.output_node,
                    link.output_port,
                    stale_threshold,
                );

                let sink = Self::meter_activity_for_node_port(
                    &state.graph,
                    link.input_node,
                    link.input_port,
                    stale_threshold,
                );

                let activity = match (source, sink) {
                    // Best case: both ends know the exact channel. Use the overlap.
                    (Some((src, true)), Some((dst, true))) => src.min(dst),
                    // If only one side is exact, trust it over coarse node-level activity.
                    (Some((src, true)), Some((_dst, false))) => src,
                    (Some((_src, false)), Some((dst, true))) => dst,
                    (Some((src, true)), None) => src,
                    (None, Some((dst, true))) => dst,
                    // Only coarse node activity available: fall back to the stronger endpoint.
                    (Some((src, false)), Some((dst, false))) => src.max(dst),
                    (Some((src, false)), None) => src,
                    (None, Some((dst, false))) => dst,
                    (None, None) => 0.0,
                };
                (link.id, activity)
            })
            .collect();

        // Apply updates
        for (link_id, activity) in updates {
            if let Some(link_meter) = state.graph.link_meters.get_mut(&link_id) {
                link_meter.update(activity, dt);
            }
        }

        // Decay inactive links
        let inactive_links: Vec<_> = state
            .graph
            .links
            .values()
            .filter(|link| !link.is_active)
            .map(|link| link.id)
            .collect();

        for link_id in inactive_links {
            if let Some(link_meter) = state.graph.link_meters.get_mut(&link_id) {
                link_meter.update(0.0, dt);
            }
        }
    }

    /// Enables or disables real PipeWire metering.
    pub(super) fn set_real_metering(&mut self, enabled: bool) {
        let command = if enabled {
            tracing::info!("Enabling real PipeWire metering");
            AppCommand::StartAllMeters
        } else {
            tracing::info!("Disabling real PipeWire metering");
            AppCommand::StopAllMeters
        };

        if let Some(ref handler) = self.command_handler {
            if let Err(e) = handler.execute_unchecked(command) {
                tracing::error!("Failed to change meter state: {:?}", e);
            }
        }
    }

    /// Processes events from PipeWire or remote connection.
    ///
    /// This is the main event loop that handles:
    /// - Connection state changes
    /// - Node/Port/Link lifecycle events
    /// - Volume and mute changes
    /// - Meter updates
    pub(super) fn process_pw_events(&mut self) {
        let events = self.drain_events();
        if events.is_empty() {
            return;
        }

        // Check config before taking state lock
        let meters_enabled = self.config.meters.enabled;

        let mut state = self.state.write();
        let mut should_start_meters = false;
        let mut volume_changed = false;
        let mut volume_control_issues: Vec<(crate::util::id::NodeId, String)> = Vec::new();
        let mut resolved_volume_issue_nodes: Vec<crate::util::id::NodeId> = Vec::new();
        let mut pending_mixer_nodes: Vec<(String, u32)> = Vec::new();
        let mut new_mixer_node_ids: Vec<(NodeId, String)> = Vec::new();

        for event in events {
            match event {
                PwEvent::Connected => {
                    state.connection = ConnectionState::Connected;
                    tracing::info!("Connected to PipeWire");
                    if meters_enabled {
                        should_start_meters = true;
                    }
                }
                PwEvent::Disconnected => {
                    state.connection = ConnectionState::Disconnected;
                    // Persist all current volume states before clearing graph
                    let volume_snapshot: Vec<_> = state
                        .graph
                        .volumes
                        .iter()
                        .filter_map(|(node_id, vol)| {
                            state.graph.get_node(node_id).map(|node| {
                                let id = create_stable_identifier(node, &state.graph);
                                (id, vol.clone())
                            })
                        })
                        .collect();
                    for (identifier, volume) in volume_snapshot {
                        state.ui.persist_volume(&identifier, volume);
                    }
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
                    // Track mixer node names for post-loop registration
                    if info.name.starts_with("pipeflow-mixer-") {
                        new_mixer_node_ids.push((info.id, info.name.clone()));
                    }
                    Self::handle_node_added(&mut state, info);
                }
                PwEvent::NodeRemoved(id) => {
                    // Persist volume before removing node
                    let vol_snapshot = state.graph.volumes.get(&id).cloned();
                    let identifier = state
                        .graph
                        .get_node(&id)
                        .map(|node| create_stable_identifier(node, &state.graph));
                    if let (Some(vol), Some(ident)) = (vol_snapshot, identifier) {
                        state.ui.persist_volume(&ident, vol);
                    }
                    state.graph.remove_node(&id);
                    state.ui.cleanup_removed_node(&id);
                    resolved_volume_issue_nodes.push(id);
                }
                PwEvent::PortAdded(info) => {
                    let node_id = info.node_id;
                    if state.graph.get_node(&node_id).is_none() {
                        tracing::trace!(
                            "Ignoring port for unknown/hidden node {:?}: {:?}",
                            node_id,
                            info.id
                        );
                        continue;
                    }
                    let port = Port {
                        id: info.id,
                        node_id: info.node_id,
                        name: info.name,
                        direction: info.direction,
                        channel: info.channel,
                        physical_path: info.physical_path,
                        alias: info.alias,
                        is_monitor: info.is_monitor,
                        is_control: info.is_control,
                    };
                    state.graph.add_port(port);
                    // Reconcile rules when ports are added (rules match on port names)
                    Self::reconcile_rules_for_node(&mut state, node_id);
                }
                PwEvent::PortRemoved(id) => {
                    state.graph.remove_port(&id);
                }
                PwEvent::LinkAdded(info) => {
                    if state.graph.get_node(&info.output_node).is_none()
                        || state.graph.get_node(&info.input_node).is_none()
                    {
                        tracing::trace!(
                            "Ignoring link touching unknown/hidden nodes: {:?} ({} -> {})",
                            info.id,
                            info.output_node.raw(),
                            info.input_node.raw()
                        );
                        continue;
                    }
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
                    state.graph.remove_link(&id);
                }
                PwEvent::VolumeChanged(node_id, volume) => {
                    state.graph.volumes.insert(node_id, volume.clone());
                    // Persist volume for restoration after node restart
                    let identifier = state
                        .graph
                        .get_node(&node_id)
                        .map(|node| create_stable_identifier(node, &state.graph));
                    if let Some(ident) = identifier {
                        state.ui.persist_volume(&ident, volume);
                        volume_changed = true;
                        resolved_volume_issue_nodes.push(node_id);
                    }
                }
                PwEvent::MuteChanged(node_id, muted) => {
                    // Clone what we need before mutating
                    let updated = if let Some(vol) = state.graph.volumes.get_mut(&node_id) {
                        vol.muted = muted;
                        Some(vol.clone())
                    } else {
                        None
                    };
                    // Persist updated mute state
                    if let Some(vol) = updated {
                        let identifier = state
                            .graph
                            .get_node(&node_id)
                            .map(|node| create_stable_identifier(node, &state.graph));
                        if let Some(ident) = identifier {
                            state.ui.persist_volume(&ident, vol);
                            volume_changed = true;
                            resolved_volume_issue_nodes.push(node_id);
                        }
                    }
                }
                PwEvent::VolumeControlFailed(node_id, error_msg) => {
                    tracing::warn!("Volume control failed for {:?}: {}", node_id, error_msg);
                    volume_control_issues.push((node_id, error_msg.clone()));
                    state.graph.volume_control_failed.insert(node_id, error_msg);
                }
                PwEvent::MeterUpdate(updates) => {
                    for update in updates {
                        if let Some(meter) = state.graph.meters.get_mut(&update.node_id) {
                            meter.update(update.peak, update.rms);
                        }
                    }
                }
                PwEvent::MixerNodeCreated { name, pid } => {
                    tracing::info!(
                        "Mixer node '{}' created (PID {}), will register when PW node appears",
                        name,
                        pid
                    );
                    // The actual NodeId assignment happens when the PipeWire
                    // NodeAdded event arrives for the pipeflow-mixer-* node.
                    // We store the PID in a pending map so we can associate it later.
                    pending_mixer_nodes.push((name, pid));
                }
                _ => {}
            }
        }

        drop(state);

        for node_id in resolved_volume_issue_nodes {
            self.resolve_persistent_issue(&format!("volume-control-failed-{}", node_id.raw()));
        }

        for (node_id, issue) in volume_control_issues {
            self.push_persistent_issue(
                format!("volume-control-failed-{}", node_id.raw()),
                super::FeedbackLevel::Warning,
                "Volume control is unavailable for a node",
                Some(issue),
            );
        }

        // Flag layout save if volume state changed (so it's persisted to disk)
        if volume_changed {
            self.components.needs_layout_save = true;
        }

        // Start real metering after processing all events (only in local mode)
        if should_start_meters && !self.is_remote {
            self.set_real_metering(true);
        }

        // Register pending mixer nodes — these are nodes whose pw-loopback just
        // spawned.  We store the (name, pid) so that when the PipeWire NodeAdded
        // event arrives in a future frame, we can match by node.name prefix.
        for (name, pid) in pending_mixer_nodes {
            // Scan the graph for a node whose name matches "pipeflow-mixer-<name>"
            let expected_node_name = format!("pipeflow-mixer-{}", name);
            let state = self.state.read();
            let node_id = state
                .graph
                .nodes
                .values()
                .find(|n| n.name == expected_node_name)
                .map(|n| n.id);
            drop(state);
            if let Some(node_id) = node_id {
                let input_count = 4; // default; we'll get the real count from the pending info
                let mut mixer_state =
                    crate::domain::mixer_node::MixerNodeState::new(name.clone(), input_count);
                mixer_state.process_pid = Some(pid);
                self.components
                    .mixer_node_manager
                    .insert(node_id, mixer_state);
                tracing::info!("Registered mixer node '{}' as {:?}", name, node_id);
            } else {
                tracing::debug!(
                    "Mixer node '{}' not yet in graph, will match on next NodeAdded",
                    name
                );
            }
        }

        // Auto-register any pipeflow-mixer-* nodes that appeared in this frame
        // but aren't already tracked by the mixer node manager.
        for (node_id, node_name) in new_mixer_node_ids {
            if !self.components.mixer_node_manager.is_mixer_node(&node_id) {
                let display_name = node_name
                    .strip_prefix("pipeflow-mixer-")
                    .unwrap_or(&node_name)
                    .to_string();
                let mixer_state =
                    crate::domain::mixer_node::MixerNodeState::new(display_name.clone(), 4);
                self.components
                    .mixer_node_manager
                    .insert(node_id, mixer_state);
                tracing::info!(
                    "Auto-registered mixer node '{}' as {:?}",
                    display_name,
                    node_id
                );
            }
        }
    }

    /// Drains events from PipeWire or remote connection.
    fn drain_events(&self) -> Vec<PwEvent> {
        if let Some(ref pw) = self.pw_connection {
            pw.drain_events()
        } else {
            #[cfg(feature = "network")]
            {
                if let Some(ref remote) = self.remote_connection {
                    remote.drain_events()
                } else {
                    Vec::new()
                }
            }
            #[cfg(not(feature = "network"))]
            {
                Vec::new()
            }
        }
    }

    /// Handles a node being added to the graph.
    ///
    /// This is an associated function (not a method) to avoid borrow conflicts
    /// when called from within a state lock.
    fn handle_node_added(
        state: &mut crate::core::state::AppState,
        info: crate::pipewire::events::NodeInfo,
    ) {
        let media_class = info.media_class.clone();
        let app_name = info.application_name.clone();
        let node_name = info.name.clone();

        let node = Node {
            id: info.id,
            name: info.name,
            client_id: info.client_id,
            media_class: info.media_class,
            application_name: info.application_name,
            description: info.description,
            nick: info.nick,
            format: info.format,
            port_ids: Vec::new(),
            is_active: true,
            layer: info.layer,
        };
        state.graph.add_node(node);

        // Create stable identifier for position restoration
        // Uses the helper function which handles satellite/metering nodes specially
        let identifier = if let Some(node) = state.graph.get_node(&info.id) {
            create_stable_identifier(node, &state.graph)
        } else {
            // Fallback (shouldn't happen since we just added the node)
            crate::util::id::NodeIdentifier::new(
                node_name.clone(),
                app_name,
                media_class.as_ref().map(|mc| mc.display_name().to_string()),
            )
        };

        // Try to restore position from persistent storage first
        if !state.ui.restore_position_for_node(info.id, &identifier) {
            // No saved position - calculate a new one
            let layout = SmartLayout::new();
            let config = layout.config();

            // Check if this is a metering node (satellite) - position to the right of main node
            let position = if is_metering_node(&node_name) {
                if let Some(main_node_id) = get_metering_target_id(&node_name) {
                    if let Some(main_pos) = state.ui.node_positions.get(&main_node_id) {
                        // Position to the right of the main node
                        Position::new(
                            main_pos.x + config.node_width + config.satellite_gap,
                            main_pos.y + config.satellite_offset_y,
                        )
                    } else {
                        // Main node not positioned yet, use default
                        Position::zero()
                    }
                } else {
                    Position::zero()
                }
            } else {
                // Regular node - place near connected nodes to minimize line lengths
                let viewport_center = Position::zero();
                layout.calculate_new_node_position(
                    info.id,
                    &state.graph,
                    &state.ui.node_positions,
                    viewport_center,
                )
            };

            state.ui.animate_to_position(info.id, position, true);
            state
                .ui
                .persistent_positions
                .insert(identifier.clone(), position);
        }

        // Restore uninteresting status from persistent storage
        state
            .ui
            .restore_uninteresting_for_node(info.id, &identifier);

        // Restore custom display name from persistent storage
        state.ui.restore_custom_name_for_node(info.id, &identifier);

        // Restore volume/mute state from persistent storage (e.g., after app restart)
        if let Some(volume) = state.ui.restore_volume_for_node(&identifier).cloned() {
            tracing::debug!(
                "Restoring volume for node {:?}: master={:.3}, muted={}",
                info.id,
                volume.master,
                volume.muted
            );
            state.graph.volumes.insert(info.id, volume);
        }

        // Reconcile group membership
        state.ui.groups.reconcile_node(info.id, &identifier);

        // Reconcile connection rules for this node
        Self::reconcile_rules_for_node(state, info.id);
    }

    /// Evaluates connection rules when a node appears.
    /// Queues pending connections to be created by the main loop.
    fn reconcile_rules_for_node(state: &mut crate::core::state::AppState, trigger_node_id: NodeId) {
        let trigger_node = match state.graph.get_node(&trigger_node_id) {
            Some(n) => n,
            None => return,
        };
        let trigger_app_name = trigger_node.application_name.clone();
        let trigger_node_name = trigger_node.name.clone();

        // Collect all port IDs for the trigger node
        let trigger_port_ids: Vec<_> = state
            .graph
            .ports
            .values()
            .filter(|p| p.node_id == trigger_node_id)
            .map(|p| p.id)
            .collect();

        // If node has no ports yet, skip (we'll reconcile when ports are added)
        if trigger_port_ids.is_empty() {
            return;
        }

        // Evaluate each enabled rule
        let rules: Vec<_> = state
            .ui
            .rules
            .enabled_rules()
            .map(|r| (r.id, r.trigger, r.exclusive, r.connections.clone()))
            .collect();

        for (rule_id, trigger, exclusive, connections) in rules {
            for spec in &connections {
                // Find all output ports matching the output pattern
                let output_ports: Vec<_> = state
                    .graph
                    .ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Output)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.output_pattern.matches_runtime(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        })
                        .unwrap_or(false)
                    })
                    .map(|p| (p.id, p.node_id))
                    .collect();

                // Find all input ports matching the input pattern
                let input_ports: Vec<_> = state
                    .graph
                    .ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Input)
                    .filter(|p| {
                        let node = state.graph.get_node(&p.node_id);
                        node.map(|n| {
                            spec.input_pattern.matches_runtime(
                                n.application_name.as_deref(),
                                &n.name,
                                &p.name,
                            )
                        })
                        .unwrap_or(false)
                    })
                    .map(|p| (p.id, p.node_id))
                    .collect();

                // Check trigger conditions and queue connections
                for (output_port_id, output_node_id) in &output_ports {
                    for (input_port_id, input_node_id) in &input_ports {
                        let should_connect = match trigger {
                            RuleTrigger::OnSourceAppear => *output_node_id == trigger_node_id,
                            RuleTrigger::OnTargetAppear => *input_node_id == trigger_node_id,
                            RuleTrigger::OnBothPresent => {
                                *output_node_id == trigger_node_id
                                    || *input_node_id == trigger_node_id
                            }
                            RuleTrigger::ManualOnly => false,
                        };

                        if !should_connect {
                            continue;
                        }

                        // Check if link already exists
                        let link_exists = state.graph.links.values().any(|l| {
                            l.output_port == *output_port_id && l.input_port == *input_port_id
                        });

                        if !link_exists {
                            state.ui.rules.queue_connection(
                                *output_port_id,
                                *input_port_id,
                                rule_id,
                            );
                        }
                    }
                }

                // Handle exclusive mode: queue disconnections for other links
                if exclusive {
                    // Find all links that connect to our matched ports but aren't in our rule
                    for (output_port_id, _) in &output_ports {
                        for link in state.graph.links.values() {
                            if link.output_port == *output_port_id {
                                // Check if this link's input is NOT one of our matched inputs
                                let is_rule_link =
                                    input_ports.iter().any(|(ip, _)| link.input_port == *ip);
                                if !is_rule_link {
                                    state.ui.rules.queue_disconnection(link.id);
                                }
                            }
                        }
                    }
                    for (input_port_id, _) in &input_ports {
                        for link in state.graph.links.values() {
                            if link.input_port == *input_port_id {
                                // Check if this link's output is NOT one of our matched outputs
                                let is_rule_link =
                                    output_ports.iter().any(|(op, _)| link.output_port == *op);
                                if !is_rule_link {
                                    state.ui.rules.queue_disconnection(link.id);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Suppress unused variable warnings
        let _ = trigger_app_name;
        let _ = trigger_node_name;
    }
}
