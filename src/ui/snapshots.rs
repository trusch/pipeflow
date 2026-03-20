//! Snapshot management UI.
//!
//! Provides controls for saving and restoring routing snapshots.

use crate::core::state::GraphState;
use crate::domain::graph::PortDirection;
use crate::domain::snapshots::{display_timestamp, Snapshot, SnapshotManager};
use crate::util::id::{NodeId, NodeIdentifier};
use egui::{Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Snapshot management panel.
pub struct SnapshotPanel {
    /// Text buffer for new snapshot name.
    name_input: String,
    /// Which scenes are expanded to show restore details.
    expanded_scenes: HashSet<Uuid>,
}

#[derive(Debug, Default, Clone)]
struct SceneRestorePreview {
    links_to_create: usize,
    links_to_remove: usize,
    volume_changes: usize,
    unresolved_connections: usize,
    missing_nodes: Vec<String>,
}

impl SceneRestorePreview {
    fn status_text(&self) -> String {
        if self.unresolved_connections == 0 {
            "Ready".to_string()
        } else {
            format!(
                "{} waiting on missing nodes",
                self.unresolved_connections
            )
        }
    }
}

impl Default for SnapshotPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotPanel {
    /// Creates a new snapshot panel.
    pub fn new() -> Self {
        Self {
            name_input: String::new(),
            expanded_scenes: HashSet::new(),
        }
    }

    /// Shows the snapshot panel and returns any requested actions.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        manager: &SnapshotManager,
        graph: &GraphState,
    ) -> SnapshotPanelResponse {
        let mut response = SnapshotPanelResponse::default();

        ui.horizontal_wrapped(|ui| {
            if ui
                .button(format!("{} Quick Save", egui_phosphor::regular::LIGHTNING))
                .on_hover_text("Quick save current state")
                .clicked()
            {
                response.capture_quick_save = true;
            }

            let te = ui.add(
                egui::TextEdit::singleline(&mut self.name_input)
                    .hint_text("Scene name...")
                    .desired_width(220.0),
            );

            let can_save = !self.name_input.trim().is_empty();
            let save_clicked = ui
                .add_enabled(
                    can_save,
                    egui::Button::new(format!(
                        "{} Save Scene",
                        egui_phosphor::regular::BOOKMARK_SIMPLE
                    )),
                )
                .clicked();

            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if (save_clicked || enter) && can_save {
                response.capture_snapshot = Some(self.name_input.trim().to_string());
                self.name_input.clear();
            }
        });

        ui.add_space(4.0);
        ui.separator();

        let mut scenes: Vec<_> = manager.list().iter().collect();
        scenes.sort_by(|a, b| {
            b.favorite
                .cmp(&a.favorite)
                .then_with(|| b.protected.cmp(&a.protected))
                .then_with(|| b.created_at.cmp(&a.created_at))
        });

        if scenes.is_empty() {
            ui.weak("No saved setups yet");
        } else {
            for snap in scenes {
                let preview = compute_restore_preview(snap, graph);
                ui.push_id(snap.id, |ui| {
                    self.show_scene_card(ui, snap, &preview, &mut response);
                });
                ui.add_space(6.0);
            }
        }

        response
    }

    fn show_scene_card(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        preview: &SceneRestorePreview,
        response: &mut SnapshotPanelResponse,
    ) {
        let is_expanded = self.expanded_scenes.contains(&snap.id);
        let accent = if snap.favorite {
            Color32::from_rgb(255, 210, 90)
        } else if snap.quick_save {
            Color32::from_rgb(120, 200, 255)
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };

        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.7)))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&snap.name).strong().size(15.0));
                            if snap.favorite {
                                badge(ui, egui_phosphor::regular::STAR, "Favorite", accent);
                            }
                            if snap.protected {
                                badge(
                                    ui,
                                    egui_phosphor::regular::SHIELD_CHECK,
                                    "Protected",
                                    Color32::from_rgb(120, 220, 150),
                                );
                            }
                            if snap.quick_save {
                                badge(
                                    ui,
                                    egui_phosphor::regular::LIGHTNING,
                                    "Quick Save",
                                    Color32::from_rgb(120, 200, 255),
                                );
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.weak(display_timestamp(&snap.created_at));
                            ui.weak(format!("• {} links", snap.connections.len()));
                            ui.weak(format!("• {} volume targets", snap.volumes.len()));
                            if let Some(last_used) = &snap.last_restored_at {
                                ui.weak(format!("• last used {}", display_timestamp(last_used)));
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(preview.status_text());
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui
                            .small_button(if is_expanded {
                                egui_phosphor::regular::CARET_UP
                            } else {
                                egui_phosphor::regular::CARET_DOWN
                            })
                            .on_hover_text("Show restore summary")
                            .clicked()
                        {
                            if is_expanded {
                                self.expanded_scenes.remove(&snap.id);
                            } else {
                                self.expanded_scenes.insert(snap.id);
                            }
                        }

                        if ui
                            .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                            .on_hover_text("Restore this saved setup")
                            .clicked()
                        {
                            response.restore_snapshot = Some(snap.id);
                        }

                        if ui
                            .small_button(egui_phosphor::regular::STAR)
                            .on_hover_text(if snap.favorite {
                                "Remove favorite pin"
                            } else {
                                "Pin near the top"
                            })
                            .clicked()
                        {
                            response.toggle_favorite = Some(snap.id);
                        }

                        if ui
                            .small_button(egui_phosphor::regular::SHIELD_CHECK)
                            .on_hover_text(if snap.protected {
                                "Unprotect so it can be deleted"
                            } else {
                                "Protect against accidental deletion"
                            })
                            .clicked()
                        {
                            response.toggle_protected = Some(snap.id);
                        }

                        if ui
                            .add_enabled(
                                !snap.protected,
                                egui::Button::new(egui_phosphor::regular::TRASH),
                            )
                            .on_hover_text(if snap.protected {
                                "Protected setups must be unprotected before deleting"
                            } else {
                                "Delete saved setup"
                            })
                            .clicked()
                        {
                            response.delete_snapshot = Some(snap.id);
                        }
                    });
                });

                if is_expanded {
                    ui.add_space(8.0);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Before restore").strong());
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(format!("Create {} link(s)", preview.links_to_create));
                                ui.weak(format!("Remove {} link(s)", preview.links_to_remove));
                                ui.weak(format!("Adjust {} volume target(s)", preview.volume_changes));
                            });
                            if preview.unresolved_connections > 0 {
                                ui.add_space(4.0);
                                ui.colored_label(
                                    Color32::from_rgb(255, 200, 100),
                                    format!(
                                        "{} unresolved",
                                        preview.unresolved_connections
                                    ),
                                );
                                if !preview.missing_nodes.is_empty() {
                                    let missing = preview.missing_nodes.join(", ");
                                    ui.weak(format!("Waiting on: {}", missing));
                                }
                            } else {
                                ui.colored_label(
                                    Color32::from_rgb(120, 220, 150),
                                    "All nodes present",
                                );
                            }
                        });
                }
            });
    }
}

fn badge(ui: &mut Ui, icon: &str, text: &str, color: Color32) {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            28,
        ))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(255)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(color).small());
                ui.label(RichText::new(text).color(color).small());
            });
        });
}

fn compute_restore_preview(snapshot: &Snapshot, graph: &GraphState) -> SceneRestorePreview {
    let mut identifier_to_nodes: HashMap<NodeIdentifier, Vec<NodeId>> = HashMap::new();
    for node in graph.nodes.values() {
        let identifier = NodeIdentifier::new(
            node.name.clone(),
            node.application_name.clone(),
            node.media_class
                .as_ref()
                .map(|media_class| media_class.display_name().to_string()),
        );
        identifier_to_nodes
            .entry(identifier)
            .or_default()
            .push(node.id);
    }

    let mut desired_links = HashSet::new();
    let mut unresolved_connections = 0usize;
    let mut missing_nodes = HashSet::new();

    for conn in &snapshot.connections {
        let output_node_ids = identifier_to_nodes.get(&conn.output_node);
        let input_node_ids = identifier_to_nodes.get(&conn.input_node);

        let out_port = output_node_ids.and_then(|node_ids| {
            node_ids.iter().find_map(|nid| {
                graph.ports.values().find(|port| {
                    port.node_id == *nid
                        && port.direction == PortDirection::Output
                        && port.name == conn.output_port_name
                })
            })
        });

        let in_port = input_node_ids.and_then(|node_ids| {
            node_ids.iter().find_map(|nid| {
                graph.ports.values().find(|port| {
                    port.node_id == *nid
                        && port.direction == PortDirection::Input
                        && port.name == conn.input_port_name
                })
            })
        });

        match (out_port, in_port) {
            (Some(out), Some(inp)) => {
                desired_links.insert((out.id, inp.id));
            }
            _ => {
                unresolved_connections += 1;
                if output_node_ids.is_none() {
                    missing_nodes.insert(conn.output_node.to_string());
                }
                if input_node_ids.is_none() {
                    missing_nodes.insert(conn.input_node.to_string());
                }
            }
        }
    }

    let links_to_remove = graph
        .links
        .values()
        .filter(|link| !desired_links.contains(&(link.output_port, link.input_port)))
        .count();

    let links_to_create = desired_links
        .iter()
        .filter(|(out_port, in_port)| {
            !graph
                .links
                .values()
                .any(|link| link.output_port == *out_port && link.input_port == *in_port)
        })
        .count();

    let volume_changes = snapshot
        .volumes
        .iter()
        .filter(|volume| identifier_to_nodes.contains_key(&volume.identifier))
        .count();

    let mut missing_nodes: Vec<_> = missing_nodes.into_iter().collect();
    missing_nodes.sort();

    SceneRestorePreview {
        links_to_create,
        links_to_remove,
        volume_changes,
        unresolved_connections,
        missing_nodes,
    }
}

/// Response from the snapshot panel.
#[derive(Debug, Default)]
pub struct SnapshotPanelResponse {
    /// User wants to capture a new snapshot with this name.
    pub capture_snapshot: Option<String>,
    /// User wants to create a disposable quick save.
    pub capture_quick_save: bool,
    /// User wants to restore a snapshot.
    pub restore_snapshot: Option<Uuid>,
    /// User wants to delete a snapshot.
    pub delete_snapshot: Option<Uuid>,
    /// User wants to pin or unpin a snapshot.
    pub toggle_favorite: Option<Uuid>,
    /// User wants to protect or unprotect a snapshot.
    pub toggle_protected: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::VolumeControl;
    use crate::domain::graph::{Link, LinkState, MediaClass, Node, Port};
    use crate::domain::snapshots::{SnapshotConnection, SnapshotVolume};
    use crate::util::id::{LinkId, PortId};

    fn node(id: u32, name: &str, app: Option<&str>, media_class: MediaClass) -> Node {
        Node {
            id: NodeId::new(id),
            name: name.to_string(),
            client_id: None,
            media_class: Some(media_class),
            application_name: app.map(str::to_string),
            description: None,
            nick: None,
            format: None,
            port_ids: Vec::new(),
            is_active: true,
            layer: crate::domain::graph::NodeLayer::Session,
        }
    }

    #[test]
    fn test_compute_restore_preview_counts_missing_items() {
        let mut graph = GraphState::default();
        let source = node(1, "Firefox", Some("Firefox"), MediaClass::StreamOutputAudio);
        let sink = node(2, "Speakers", None, MediaClass::AudioSink);
        graph.add_node(source.clone());
        graph.add_node(sink.clone());
        graph.add_port(Port {
            id: PortId::new(10),
            node_id: source.id,
            name: "output_FL".to_string(),
            direction: PortDirection::Output,
            channel: None,
            physical_path: None,
            alias: None,
            is_monitor: false,
            is_control: false,
        });
        graph.add_port(Port {
            id: PortId::new(11),
            node_id: sink.id,
            name: "input_FL".to_string(),
            direction: PortDirection::Input,
            channel: None,
            physical_path: None,
            alias: None,
            is_monitor: false,
            is_control: false,
        });
        graph.add_link(Link {
            id: LinkId::new(50),
            output_port: PortId::new(10),
            input_port: PortId::new(11),
            output_node: source.id,
            input_node: sink.id,
            is_active: true,
            state: LinkState::Active,
        });

        let snapshot = Snapshot {
            id: Uuid::new_v4(),
            name: "Studio".into(),
            created_at: "2026-03-13T20:00:00Z".into(),
            connections: vec![
                SnapshotConnection {
                    output_node: NodeIdentifier::new(
                        "Firefox".into(),
                        Some("Firefox".into()),
                        Some(MediaClass::StreamOutputAudio.display_name().to_string()),
                    ),
                    output_port_name: "output_FL".into(),
                    input_node: NodeIdentifier::new(
                        "Speakers".into(),
                        None,
                        Some(MediaClass::AudioSink.display_name().to_string()),
                    ),
                    input_port_name: "input_FL".into(),
                },
                SnapshotConnection {
                    output_node: NodeIdentifier::new("Missing".into(), None, None),
                    output_port_name: "out".into(),
                    input_node: NodeIdentifier::new(
                        "Speakers".into(),
                        None,
                        Some(MediaClass::AudioSink.display_name().to_string()),
                    ),
                    input_port_name: "input_FR".into(),
                },
            ],
            volumes: vec![SnapshotVolume {
                identifier: NodeIdentifier::new(
                    "Speakers".into(),
                    None,
                    Some(MediaClass::AudioSink.display_name().to_string()),
                ),
                volume: VolumeControl::default(),
            }],
            quick_save: false,
            favorite: false,
            protected: false,
            last_restored_at: None,
        };

        let preview = compute_restore_preview(&snapshot, &graph);
        assert_eq!(preview.links_to_create, 0);
        assert_eq!(preview.links_to_remove, 0);
        assert_eq!(preview.volume_changes, 1);
        assert_eq!(preview.unresolved_connections, 1);
        assert_eq!(preview.missing_nodes, vec!["Missing"]);
    }
}
