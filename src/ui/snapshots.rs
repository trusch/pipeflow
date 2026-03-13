//! Snapshot management UI.
//!
//! Provides controls for saving and restoring routing snapshots.

use crate::domain::snapshots::SnapshotManager;
use egui::Ui;
use uuid::Uuid;

/// Snapshot management panel.
pub struct SnapshotPanel {
    /// Text buffer for new snapshot name.
    name_input: String,
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
        }
    }

    /// Shows the snapshot panel and returns any requested actions.
    pub fn show(&mut self, ui: &mut Ui, manager: &SnapshotManager) -> SnapshotPanelResponse {
        let mut response = SnapshotPanelResponse::default();

        // Save snapshot section
        ui.horizontal(|ui| {
            let te = ui.add(
                egui::TextEdit::singleline(&mut self.name_input)
                    .hint_text("Snapshot name...")
                    .desired_width(ui.available_width() - 70.0),
            );

            let can_save = !self.name_input.trim().is_empty();
            let save_clicked = ui
                .add_enabled(
                    can_save,
                    egui::Button::new(format!("{} Save", egui_phosphor::regular::FLOPPY_DISK)),
                )
                .clicked();

            // Also save on Enter
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if (save_clicked || enter) && can_save {
                response.capture_snapshot = Some(self.name_input.trim().to_string());
                self.name_input.clear();
            }
        });

        ui.separator();

        // List of saved snapshots
        let snapshots = manager.list();
        if snapshots.is_empty() {
            ui.weak("No snapshots saved");
        } else {
            for snap in snapshots {
                ui.push_id(snap.id, |ui| {
                    ui.horizontal(|ui| {
                        // Name and info
                        ui.vertical(|ui| {
                            ui.label(&snap.name);
                            ui.horizontal(|ui| {
                                ui.weak(format_timestamp(&snap.created_at));
                                ui.weak(format!("| {} connections", snap.connections.len()));
                            });
                        });

                        // Action buttons - right aligned
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Delete
                            if ui
                                .small_button(egui_phosphor::regular::TRASH)
                                .on_hover_text("Delete snapshot")
                                .clicked()
                            {
                                response.delete_snapshot = Some(snap.id);
                            }

                            // Restore
                            if ui
                                .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .on_hover_text("Restore this snapshot")
                                .clicked()
                            {
                                response.restore_snapshot = Some(snap.id);
                            }
                        });
                    });

                    ui.separator();
                });
            }
        }

        response
    }
}

/// Response from the snapshot panel.
#[derive(Debug, Default)]
pub struct SnapshotPanelResponse {
    /// User wants to capture a new snapshot with this name.
    pub capture_snapshot: Option<String>,
    /// User wants to restore a snapshot.
    pub restore_snapshot: Option<Uuid>,
    /// User wants to delete a snapshot.
    pub delete_snapshot: Option<Uuid>,
}

/// Formats an ISO 8601 timestamp for display (just date + time, no seconds).
fn format_timestamp(ts: &str) -> String {
    // "2024-01-15T12:30:00Z" -> "2024-01-15 12:30"
    if ts.len() >= 16 {
        ts[..16].replace('T', " ")
    } else {
        ts.to_string()
    }
}
