//! Snapshot/preset management.
//!
//! Allows users to save and restore complete routing configurations
//! (connections and optionally volumes) that persist across PipeWire restarts.

use crate::core::state::GraphState;
use crate::domain::audio::VolumeControl;
use crate::util::id::NodeIdentifier;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A saved snapshot of the current routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique identifier.
    pub id: Uuid,
    /// User-assigned name.
    pub name: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// Port-to-port connections captured in this snapshot.
    pub connections: Vec<SnapshotConnection>,
    /// Volume state per node (keyed by stable identifier).
    pub volumes: Vec<SnapshotVolume>,
}

/// A single port-to-port connection within a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConnection {
    /// Stable identifier for the output (source) node.
    pub output_node: NodeIdentifier,
    /// Port name on the output node.
    pub output_port_name: String,
    /// Stable identifier for the input (sink) node.
    pub input_node: NodeIdentifier,
    /// Port name on the input node.
    pub input_port_name: String,
}

/// Volume state for a single node within a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotVolume {
    /// Stable identifier for the node.
    pub identifier: NodeIdentifier,
    /// Volume state.
    pub volume: VolumeControl,
}

/// Manages saving, loading, and deleting snapshots on disk.
pub struct SnapshotManager {
    snapshots: Vec<Snapshot>,
    data_dir: PathBuf,
}

impl SnapshotManager {
    /// Creates a new manager, loading all snapshots from `data_dir/snapshots/`.
    pub fn new(data_dir: PathBuf) -> Self {
        let snapshots_dir = data_dir.join("snapshots");
        let mut snapshots = Vec::new();

        if snapshots_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        match std::fs::read_to_string(&path) {
                            Ok(contents) => match serde_json::from_str::<Snapshot>(&contents) {
                                Ok(snap) => snapshots.push(snap),
                                Err(e) => {
                                    tracing::warn!("Failed to parse snapshot {:?}: {}", path, e)
                                }
                            },
                            Err(e) => {
                                tracing::warn!("Failed to read snapshot {:?}: {}", path, e)
                            }
                        }
                    }
                }
            }
        }

        // Sort by creation time
        snapshots.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Self {
            snapshots,
            data_dir,
        }
    }

    /// Captures a snapshot of the current graph state.
    ///
    /// `resolve_identifier` is a function that converts a node to its stable identifier.
    pub fn capture<F>(
        &mut self,
        name: String,
        graph: &GraphState,
        resolve_identifier: F,
    ) -> Result<Uuid>
    where
        F: Fn(&crate::domain::graph::Node, &GraphState) -> NodeIdentifier,
    {
        let mut connections = Vec::new();

        for link in graph.links.values() {
            let out_port = match graph.get_port(&link.output_port) {
                Some(p) => p,
                None => continue,
            };
            let in_port = match graph.get_port(&link.input_port) {
                Some(p) => p,
                None => continue,
            };
            let out_node = match graph.get_node(&link.output_node) {
                Some(n) => n,
                None => continue,
            };
            let in_node = match graph.get_node(&link.input_node) {
                Some(n) => n,
                None => continue,
            };

            connections.push(SnapshotConnection {
                output_node: resolve_identifier(out_node, graph),
                output_port_name: out_port.name.clone(),
                input_node: resolve_identifier(in_node, graph),
                input_port_name: in_port.name.clone(),
            });
        }

        // Capture volumes
        let mut volumes = Vec::new();
        for (node_id, vol) in &graph.volumes {
            if let Some(node) = graph.get_node(node_id) {
                volumes.push(SnapshotVolume {
                    identifier: resolve_identifier(node, graph),
                    volume: vol.clone(),
                });
            }
        }

        let snapshot = Snapshot {
            id: Uuid::new_v4(),
            name,
            created_at: chrono_now(),
            connections,
            volumes,
        };

        let id = snapshot.id;
        self.save_to_disk(&snapshot)?;
        self.snapshots.push(snapshot);
        Ok(id)
    }

    /// Returns all snapshots.
    pub fn list(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Gets a snapshot by ID.
    pub fn get(&self, id: Uuid) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// Deletes a snapshot by ID.
    pub fn delete(&mut self, id: Uuid) -> Result<()> {
        let path = self.snapshot_path(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to delete snapshot file {:?}", path))?;
        }
        self.snapshots.retain(|s| s.id != id);
        Ok(())
    }

    /// Renames a snapshot.
    pub fn rename(&mut self, id: Uuid, new_name: String) -> Result<()> {
        if let Some(snap) = self.snapshots.iter_mut().find(|s| s.id == id) {
            snap.name = new_name;
        }
        // Save after the mutable borrow is released
        if let Some(snap) = self.snapshots.iter().find(|s| s.id == id) {
            let snap_clone = snap.clone();
            self.save_to_disk(&snap_clone)?;
        }
        Ok(())
    }

    // --- Internal ---

    fn snapshots_dir(&self) -> PathBuf {
        self.data_dir.join("snapshots")
    }

    fn snapshot_path(&self, id: Uuid) -> PathBuf {
        self.snapshots_dir().join(format!("{}.json", id))
    }

    fn save_to_disk(&self, snapshot: &Snapshot) -> Result<()> {
        let dir = self.snapshots_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create snapshots dir {:?}", dir))?;
        let path = self.snapshot_path(snapshot.id);
        let json =
            serde_json::to_string_pretty(snapshot).context("Failed to serialize snapshot")?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write snapshot {:?}", path))?;
        Ok(())
    }
}

/// Returns the current UTC time as an ISO 8601 string.
fn chrono_now() -> String {
    // Use std::time to avoid adding chrono dependency
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple ISO 8601 without external crate
    let (year, month, day, hour, min, sec) = unix_to_datetime(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Converts unix timestamp to (year, month, day, hour, minute, second).
fn unix_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let mut days = secs / 86400;

    // Calculate year
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    // Calculate month and day
    let days_in_months: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for &dim in &days_in_months {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }

    (year, month, days + 1, hour, min, sec)
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chrono_now_format() {
        let ts = chrono_now();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
    }

    #[test]
    fn test_unix_to_datetime_epoch() {
        let (y, m, d, h, mi, s) = unix_to_datetime(0);
        assert_eq!((y, m, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_unix_to_datetime_known() {
        // 2024-01-15T11:30:00Z = 1705318200
        let (y, m, d, h, mi, _s) = unix_to_datetime(1705318200);
        assert_eq!(y, 2024);
        assert_eq!(m, 1);
        assert_eq!(d, 15);
        assert_eq!(h, 11);
        assert_eq!(mi, 30);
    }

    #[test]
    fn test_snapshot_serialization() {
        let snap = Snapshot {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            connections: vec![SnapshotConnection {
                output_node: NodeIdentifier::new("out".into(), None, None),
                output_port_name: "output_FL".into(),
                input_node: NodeIdentifier::new("in".into(), None, None),
                input_port_name: "input_FL".into(),
            }],
            volumes: vec![],
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "Test");
        assert_eq!(restored.connections.len(), 1);
    }
}
