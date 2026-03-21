//! Domain model for graph-native mixer nodes.
//!
//! A mixer node is a real PipeWire node created by pipeflow with N stereo input
//! strips and a stereo master output.  Each strip carries gain, mute, and label
//! state that pipeflow manages on behalf of the user.

use serde::{Deserialize, Serialize};

/// Parameters needed to create a new mixer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MixerNodeConfig {
    /// Display name for the mixer.
    pub name: String,
    /// Number of stereo input strips (2–16).
    pub input_count: usize,
    /// Graph position hint (x, y).
    pub position: (f32, f32),
}

/// Per-strip state inside a mixer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerStripState {
    /// Linear gain multiplier (0.0–2.0, 1.0 = unity).
    pub gain: f32,
    /// Whether this strip is muted.
    pub muted: bool,
    /// Human-readable label (defaults to "Strip N").
    pub label: String,
}

impl MixerStripState {
    /// Creates a new strip with unity gain and a default label.
    pub fn new(index: usize) -> Self {
        Self {
            gain: 1.0,
            muted: false,
            label: format!("Strip {}", index + 1),
        }
    }
}

/// Full state for a mixer node (strips + master).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerNodeState {
    /// Display name.
    pub name: String,
    /// Per-input-strip states.
    pub strips: Vec<MixerStripState>,
    /// Master output gain (linear, 0.0–2.0).
    pub master_gain: f32,
    /// Whether the master output is muted.
    pub master_muted: bool,
    /// PID of the spawned pw-loopback process, if any.
    #[serde(skip)]
    pub process_pid: Option<u32>,
}

impl MixerNodeState {
    /// Creates a new mixer node state with the given number of input strips.
    pub fn new(name: String, input_count: usize) -> Self {
        let strips = (0..input_count).map(MixerStripState::new).collect();
        Self {
            name,
            strips,
            master_gain: 1.0,
            master_muted: false,
            process_pid: None,
        }
    }

    /// Returns the number of input strips.
    pub fn strip_count(&self) -> usize {
        self.strips.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_correct_strip_count() {
        let state = MixerNodeState::new("Test".into(), 4);
        assert_eq!(state.strip_count(), 4);
        assert_eq!(state.strips[0].label, "Strip 1");
        assert_eq!(state.strips[3].label, "Strip 4");
        assert!((state.master_gain - 1.0).abs() < f32::EPSILON);
        assert!(!state.master_muted);
    }

    #[test]
    fn strip_defaults_are_sane() {
        let strip = MixerStripState::new(0);
        assert!((strip.gain - 1.0).abs() < f32::EPSILON);
        assert!(!strip.muted);
    }
}
