//! Signal metering.
//!
//! Collects audio signal levels from PipeWire nodes.
//! Currently uses simulated data with natural-looking movement patterns.

use crate::pipewire::events::MeterUpdate;
use crate::util::id::NodeId;
use crossbeam::channel::{bounded, Sender, TrySendError};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for meter collection.
#[derive(Debug, Clone)]
pub struct MeterConfig {
    /// Whether metering is enabled
    pub enabled: bool,
    /// Refresh rate in Hz
    pub refresh_rate: u32,
    /// Channel buffer size
    pub buffer_size: usize,
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_rate: 30,
            buffer_size: 4,
        }
    }
}

/// State for meter simulation (shared between main app and meter thread).
#[derive(Debug, Default)]
pub struct MeterSimulationState {
    /// Node IDs that have active outgoing links (should show activity)
    pub active_nodes: HashSet<NodeId>,
    /// All known node IDs
    pub all_nodes: HashSet<NodeId>,
    /// Previous peak levels for smooth interpolation
    previous_levels: HashMap<NodeId, f32>,
    /// Target levels for smooth movement
    target_levels: HashMap<NodeId, f32>,
    /// Time since last target change per node
    time_since_change: HashMap<NodeId, u32>,
}

impl MeterSimulationState {
    /// Generates simulated meter data with smooth transitions.
    fn generate_updates(&mut self) -> Vec<MeterUpdate> {
        let mut updates = Vec::new();

        for &node_id in &self.all_nodes {
            let is_active = self.active_nodes.contains(&node_id);

            // Get or create previous level
            let prev = *self.previous_levels.get(&node_id).unwrap_or(&0.0);

            // Get or create target level
            let target = self.target_levels.entry(node_id).or_insert(0.0);
            let time_since = self.time_since_change.entry(node_id).or_insert(0);

            // Periodically pick new target (every ~20 frames on average for active nodes)
            *time_since += 1;
            let should_change = if is_active {
                *time_since > 15 && fastrand::f32() < 0.15
            } else {
                *time_since > 30 && fastrand::f32() < 0.05
            };

            if should_change {
                *time_since = 0;
                *target = if is_active {
                    // Active nodes: varying levels with occasional peaks
                    let base = 0.3 + fastrand::f32() * 0.4; // 0.3 to 0.7
                    if fastrand::f32() < 0.1 {
                        // Occasional peak
                        (base + 0.2).min(1.1)
                    } else {
                        base
                    }
                } else {
                    // Inactive nodes: mostly silent with occasional small blips
                    if fastrand::f32() < 0.05 {
                        0.05 + fastrand::f32() * 0.1 // Small blip
                    } else {
                        0.0
                    }
                };
            }

            // Smooth interpolation toward target (attack/decay envelope)
            let attack_rate = 0.3; // Fast attack
            let decay_rate = 0.08; // Slower decay
            let rate = if *target > prev { attack_rate } else { decay_rate };
            let current = prev + ((*target - prev) * rate);

            // Add small noise for natural movement
            let noise = if current > 0.01 {
                (fastrand::f32() - 0.5) * 0.05 * current
            } else {
                0.0
            };
            let final_level = (current + noise).max(0.0);

            // Store for next iteration
            self.previous_levels.insert(node_id, current);

            // Only send updates for nodes with some activity
            if final_level > 0.001 || prev > 0.001 {
                // Stereo simulation with slight channel variation
                let peak_l = final_level;
                let peak_r = final_level * (0.9 + fastrand::f32() * 0.2);
                let rms_l = peak_l * 0.7;
                let rms_r = peak_r * 0.7;

                updates.push(MeterUpdate {
                    node_id,
                    peak: vec![peak_l, peak_r],
                    rms: vec![rms_l, rms_r],
                });
            }
        }

        updates
    }
}

/// Collector for signal meter data.
pub struct MeterCollector {
    /// Configuration
    config: MeterConfig,
    /// Channel for sending meter updates
    update_tx: Sender<Vec<MeterUpdate>>,
    /// Whether the collector is running
    running: Arc<AtomicBool>,
    /// Thread handle
    thread_handle: Option<std::thread::JoinHandle<()>>,
    /// Shared simulation state
    pub simulation_state: Arc<RwLock<MeterSimulationState>>,
}

impl MeterCollector {
    /// Creates a new meter collector.
    pub fn new(config: MeterConfig) -> Self {
        let (update_tx, _update_rx) = bounded(config.buffer_size);
        let running = Arc::new(AtomicBool::new(false));
        let simulation_state = Arc::new(RwLock::new(MeterSimulationState::default()));

        Self {
            config,
            update_tx,
            running,
            thread_handle: None,
            simulation_state,
        }
    }

    /// Starts the meter collection thread.
    ///
    /// Returns `true` if the thread was started successfully, `false` otherwise.
    pub fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        if !self.config.enabled {
            return true;
        }

        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let update_tx = self.update_tx.clone();
        let refresh_rate = self.config.refresh_rate;
        let simulation_state = self.simulation_state.clone();

        match std::thread::Builder::new()
            .name("meter-collector".to_string())
            .spawn(move || {
                run_meter_thread(running, update_tx, refresh_rate, simulation_state);
            }) {
            Ok(handle) => {
                self.thread_handle = Some(handle);
                true
            }
            Err(e) => {
                tracing::error!("Failed to spawn meter thread: {}", e);
                self.running.store(false, Ordering::SeqCst);
                false
            }
        }
    }

    /// Stops the meter collection thread.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Returns whether the collector is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Sets the enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && !self.is_running() {
            self.config.enabled = true;
            self.start();
        } else if !enabled && self.is_running() {
            self.stop();
            self.config.enabled = false;
        }
    }

    /// Sets the refresh rate.
    pub fn set_refresh_rate(&mut self, rate: u32) {
        self.config.refresh_rate = rate.clamp(1, 60);
    }
}

impl Drop for MeterCollector {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Runs the meter collection thread.
fn run_meter_thread(
    running: Arc<AtomicBool>,
    update_tx: Sender<Vec<MeterUpdate>>,
    refresh_rate: u32,
    simulation_state: Arc<RwLock<MeterSimulationState>>,
) {
    let interval = Duration::from_millis(1000 / refresh_rate as u64);

    tracing::info!(
        "Meter collection started at {} Hz ({:?} interval)",
        refresh_rate,
        interval
    );

    while running.load(Ordering::SeqCst) {
        // Generate simulated meter data based on node activity
        let updates = {
            let mut state = simulation_state.write();
            state.generate_updates()
        };

        if !updates.is_empty() {
            // Use try_send to avoid blocking if the receiver is slow
            match update_tx.try_send(updates) {
                Ok(_) => {}
                Err(TrySendError::Full(_)) => {
                    // Drop the update if the channel is full
                    tracing::trace!("Meter channel full, dropping update");
                }
                Err(TrySendError::Disconnected(_)) => {
                    tracing::debug!("Meter channel disconnected");
                    break;
                }
            }
        }

        std::thread::sleep(interval);
    }

    tracing::info!("Meter collection stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meter_config_default() {
        let config = MeterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.refresh_rate, 30);
    }

    #[test]
    fn test_meter_collector_lifecycle() {
        let mut collector = MeterCollector::new(MeterConfig {
            enabled: true,
            refresh_rate: 10,
            buffer_size: 2,
        });

        assert!(!collector.is_running());

        assert!(collector.start(), "Failed to start meter collector");
        assert!(collector.is_running());

        // Let it run briefly
        std::thread::sleep(Duration::from_millis(50));

        collector.stop();
        assert!(!collector.is_running());
    }
}
