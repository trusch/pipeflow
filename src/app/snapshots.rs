//! Snapshot capture and restore workflows.

use super::{command_handling, FeedbackLevel, PipeflowApp};
use crate::core::commands::AppCommand;
use crate::util::id::NodeIdentifier;

impl PipeflowApp {
    pub(super) fn handle_snapshot_panel_response(
        &mut self,
        response: crate::ui::snapshots::SnapshotPanelResponse,
    ) {
        // Capture new named snapshot
        if response.capture_quick_save {
            let state = self.state.read();
            let result = self
                .components
                .snapshot_manager
                .capture_quick_save(&state.graph, command_handling::create_stable_identifier);
            drop(state);
            match result {
                Ok(_id) => {
                    self.resolve_persistent_issue("saved-setup-save-failed");
                    self.set_status_message("Saved a quick fallback scene", false);
                }
                Err(e) => {
                    let msg = format!("Failed to save quick scene: {}", e);
                    tracing::error!("{}", msg);
                    self.set_status_message(&msg, true);
                    self.push_persistent_issue(
                        "saved-setup-save-failed",
                        FeedbackLevel::Error,
                        "Could not save the current setup",
                        Some(msg.clone()),
                    );
                }
            }
        }

        if let Some(name) = response.capture_snapshot {
            let state = self.state.read();
            let result = self.components.snapshot_manager.capture(
                name.clone(),
                &state.graph,
                command_handling::create_stable_identifier,
            );
            drop(state);
            match result {
                Ok(_id) => {
                    self.resolve_persistent_issue("saved-setup-save-failed");
                    self.set_status_message(format!("Saved scene '{}'", name), false);
                }
                Err(e) => {
                    let msg = format!("Failed to save snapshot: {}", e);
                    tracing::error!("{}", msg);
                    self.set_status_message(&msg, true);
                    self.push_persistent_issue(
                        "saved-setup-save-failed",
                        FeedbackLevel::Error,
                        "Could not save the current setup",
                        Some(msg.clone()),
                    );
                }
            }
        }

        if let Some(id) = response.toggle_favorite {
            match self.components.snapshot_manager.toggle_favorite(id) {
                Ok(true) => self.set_status_message("Pinned saved setup", false),
                Ok(false) => self.set_status_message("Unpinned saved setup", false),
                Err(e) => {
                    let msg = format!("Failed to update saved setup: {}", e);
                    tracing::error!("{}", msg);
                    self.set_status_message(&msg, true);
                }
            }
        }

        if let Some(id) = response.toggle_protected {
            match self.components.snapshot_manager.toggle_protected(id) {
                Ok(true) => self.set_status_message("Protected saved setup", false),
                Ok(false) => self.set_status_message("Unprotected saved setup", false),
                Err(e) => {
                    let msg = format!("Failed to update saved setup protection: {}", e);
                    tracing::error!("{}", msg);
                    self.set_status_message(&msg, true);
                }
            }
        }

        // Delete snapshot
        if let Some(id) = response.delete_snapshot {
            if let Err(e) = self.components.snapshot_manager.delete(id) {
                let msg = format!("Failed to delete snapshot: {}", e);
                tracing::error!("{}", msg);
                self.set_status_message(&msg, true);
                self.push_persistent_issue(
                    "saved-setup-delete-failed",
                    FeedbackLevel::Error,
                    "Could not delete the saved setup",
                    Some(msg.clone()),
                );
            }
        }

        // Restore snapshot
        if let Some(id) = response.restore_snapshot {
            self.restore_snapshot(id);
        }
    }

    /// Restores a snapshot by diffing current connections and applying changes.
    pub(super) fn restore_snapshot(&mut self, id: uuid::Uuid) {
        use crate::domain::graph::PortDirection;

        let snapshot = match self.components.snapshot_manager.get(id) {
            Some(s) => s.clone(),
            None => return,
        };

        let state = self.state.read();

        // Build a lookup: NodeIdentifier -> Vec<NodeId> for current graph
        let mut identifier_to_nodes: std::collections::HashMap<
            NodeIdentifier,
            Vec<crate::util::id::NodeId>,
        > = std::collections::HashMap::new();
        for node in state.graph.nodes.values() {
            let ident = command_handling::create_stable_identifier(node, &state.graph);
            identifier_to_nodes.entry(ident).or_default().push(node.id);
        }

        // Resolve snapshot connections to port IDs
        let mut desired_links: std::collections::HashSet<(
            crate::util::id::PortId,
            crate::util::id::PortId,
        )> = std::collections::HashSet::new();
        let mut unresolved = 0usize;

        for conn in &snapshot.connections {
            // Find output port
            let out_port = identifier_to_nodes
                .get(&conn.output_node)
                .and_then(|node_ids| {
                    node_ids.iter().find_map(|nid| {
                        state.graph.ports.values().find(|p| {
                            p.node_id == *nid
                                && p.name == conn.output_port_name
                                && p.direction == PortDirection::Output
                        })
                    })
                });

            // Find input port
            let in_port = identifier_to_nodes
                .get(&conn.input_node)
                .and_then(|node_ids| {
                    node_ids.iter().find_map(|nid| {
                        state.graph.ports.values().find(|p| {
                            p.node_id == *nid
                                && p.name == conn.input_port_name
                                && p.direction == PortDirection::Input
                        })
                    })
                });

            match (out_port, in_port) {
                (Some(op), Some(ip)) => {
                    desired_links.insert((op.id, ip.id));
                }
                _ => {
                    unresolved += 1;
                }
            }
        }

        // Diff: find links to remove (exist now but not in snapshot)
        let mut links_to_remove = Vec::new();
        for link in state.graph.links.values() {
            let key = (link.output_port, link.input_port);
            if !desired_links.contains(&key) {
                links_to_remove.push(link.id);
            }
        }

        // Diff: find links to create (in snapshot but not in current graph)
        let mut links_to_create = Vec::new();
        for &(out_port, in_port) in &desired_links {
            let exists = state
                .graph
                .links
                .values()
                .any(|l| l.output_port == out_port && l.input_port == in_port);
            if !exists {
                links_to_create.push((out_port, in_port));
            }
        }

        // Resolve volume changes
        let mut volume_changes: Vec<(
            crate::util::id::NodeId,
            crate::domain::audio::VolumeControl,
        )> = Vec::new();
        for sv in &snapshot.volumes {
            if let Some(node_ids) = identifier_to_nodes.get(&sv.identifier) {
                for &nid in node_ids {
                    volume_changes.push((nid, sv.volume.clone()));
                }
            }
        }

        drop(state);

        // Apply removals
        for link_id in &links_to_remove {
            {
                let mut state = self.state.write();
                state.graph.remove_link(link_id);
            }
            self.handle_app_command(AppCommand::RemoveLink(*link_id));
        }

        // Apply creations
        for (output_port, input_port) in &links_to_create {
            self.handle_app_command(AppCommand::CreateLink {
                output_port: *output_port,
                input_port: *input_port,
            });
        }

        // Apply volume changes
        for (node_id, volume) in &volume_changes {
            {
                let mut state = self.state.write();
                state.graph.volumes.insert(*node_id, volume.clone());
            }
            self.handle_app_command(AppCommand::SetVolume {
                node_id: *node_id,
                volume: volume.clone(),
            });
        }

        let _ = self.components.snapshot_manager.mark_restored(id);

        let msg = format!(
            "Restored scene '{}': -{} +{} links{}",
            snapshot.name,
            links_to_remove.len(),
            links_to_create.len(),
            if unresolved > 0 {
                format!(" ({} unresolved)", unresolved)
            } else {
                String::new()
            }
        );
        tracing::info!("{}", msg);
        if unresolved > 0 {
            self.push_persistent_issue(
                "saved-setup-restore-partial",
                FeedbackLevel::Warning,
                format!("'{}' restored with missing items", snapshot.name),
                Some(format!(
                    "{} connections could not be matched to the current patch. You can reconnect the missing apps or devices, then restore again.",
                    unresolved
                )),
            );
        } else {
            self.resolve_persistent_issue("saved-setup-restore-partial");
        }
        self.set_status_message(&msg, false);
    }
}
