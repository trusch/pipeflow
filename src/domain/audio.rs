//! Audio control abstractions.
//!
//! Provides types for managing volume, mute, and channel configuration.

use serde::{Deserialize, Serialize};

/// Volume control for a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeControl {
    /// Master volume (0.0 - 1.0, can exceed 1.0 for boost)
    pub master: f32,
    /// Per-channel volumes (if different from master)
    pub channels: Vec<f32>,
    /// Whether the node is muted
    pub muted: bool,
    /// Volume step for increment/decrement operations
    pub step: f32,
}

impl Default for VolumeControl {
    fn default() -> Self {
        Self {
            master: 1.0,
            channels: vec![1.0, 1.0], // Stereo default
            muted: false,
            step: 0.05, // 5% steps
        }
    }
}

impl VolumeControl {
    /// Sets a specific channel's volume.
    pub fn set_channel(&mut self, channel: usize, volume: f32) {
        if channel < self.channels.len() {
            self.channels[channel] = volume.clamp(0.0, 2.0);
        }
    }

    /// Sets all channels to the same volume.
    pub fn set_all_channels(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 2.0);
        for ch in &mut self.channels {
            *ch = clamped;
        }
        self.master = clamped;
    }
}

/// Converts linear amplitude (0.0-1.0) to decibels.
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

/// Signal level data for metering.
#[derive(Debug, Clone)]
pub struct MeterData {
    /// Peak levels per channel (0.0-1.0+)
    pub peak: Vec<f32>,
    /// RMS levels per channel (0.0-1.0+)
    pub rms: Vec<f32>,
    /// Hold peak values (for peak hold display)
    pub peak_hold: Vec<f32>,
    /// Timestamp of last update
    pub last_update: std::time::Instant,
}

impl Default for MeterData {
    fn default() -> Self {
        Self {
            peak: vec![0.0, 0.0],
            rms: vec![0.0, 0.0],
            peak_hold: vec![0.0, 0.0],
            last_update: std::time::Instant::now(),
        }
    }
}

impl MeterData {
    /// Updates meter values with new data.
    pub fn update(&mut self, peak: Vec<f32>, rms: Vec<f32>) {
        // Update peak hold (decay slowly)
        let decay = 0.99;
        for (i, &new_peak) in peak.iter().enumerate() {
            if i < self.peak_hold.len() {
                if new_peak > self.peak_hold[i] {
                    self.peak_hold[i] = new_peak;
                } else {
                    self.peak_hold[i] *= decay;
                }
            }
        }

        self.peak = peak;
        self.rms = rms;
        self.last_update = std::time::Instant::now();
    }

    /// Returns the maximum peak across all channels.
    pub fn max_peak(&self) -> f32 {
        self.peak.iter().cloned().fold(0.0, f32::max)
    }

    /// Returns the peak for a specific channel, with staleness-based decay applied.
    /// If no updates have been received for longer than `stale_threshold`, the peak
    /// value decays exponentially toward zero.
    pub fn get_decayed_peak(&self, channel: usize, stale_threshold: std::time::Duration) -> f32 {
        let raw_peak = self.peak.get(channel).copied().unwrap_or(0.0);
        self.apply_staleness_decay(raw_peak, stale_threshold)
    }

    /// Returns the maximum peak across all channels, with staleness-based decay applied.
    pub fn get_decayed_max_peak(&self, stale_threshold: std::time::Duration) -> f32 {
        let raw_peak = self.max_peak();
        self.apply_staleness_decay(raw_peak, stale_threshold)
    }

    /// Applies exponential decay to a value based on how long since last update.
    /// Returns the original value if within the threshold, decayed value otherwise.
    fn apply_staleness_decay(&self, value: f32, stale_threshold: std::time::Duration) -> f32 {
        let elapsed = self.last_update.elapsed();
        if elapsed > stale_threshold {
            let stale_secs = (elapsed - stale_threshold).as_secs_f32();
            // Decay rate of 3.0 means ~5% remaining after 1 second
            let decay_factor = (-stale_secs * 3.0).exp();
            value * decay_factor
        } else {
            value
        }
    }

}

/// Meter data for link flow visualization.
/// Tracks the activity level flowing through a link for visual effects.
#[derive(Debug, Clone)]
pub struct LinkMeterData {
    /// Current activity level (0.0-1.0+), derived from source node's meter
    pub activity: f32,
    /// Smoothed activity for animation (with attack/decay)
    pub smoothed_activity: f32,
    /// Phase for traveling pulse animation (0.0-1.0)
    pub pulse_phase: f32,
    /// Whether this link is currently clipping (activity > 1.0)
    pub is_clipping: bool,
}

impl Default for LinkMeterData {
    fn default() -> Self {
        Self {
            activity: 0.0,
            smoothed_activity: 0.0,
            pulse_phase: 0.0,
            is_clipping: false,
        }
    }
}

impl LinkMeterData {
    /// Updates the link meter data with a new activity level from the source node.
    /// The `dt` parameter is the time delta in seconds for smooth animation.
    pub fn update(&mut self, source_activity: f32, dt: f32) {
        self.activity = source_activity;
        self.is_clipping = source_activity > 1.0;

        // Smooth the activity with attack/decay envelope
        let attack_rate = 8.0; // Fast attack
        let decay_rate = 2.0;  // Slower decay
        let rate = if source_activity > self.smoothed_activity {
            attack_rate
        } else {
            decay_rate
        };
        self.smoothed_activity += (source_activity - self.smoothed_activity) * rate * dt;
        self.smoothed_activity = self.smoothed_activity.max(0.0);

        // Advance pulse phase based on activity (faster pulses for higher activity)
        if self.smoothed_activity > 0.01 {
            let pulse_speed = 0.5 + self.smoothed_activity * 1.5; // 0.5 to 2.0 cycles per second
            self.pulse_phase += pulse_speed * dt;
            if self.pulse_phase > 1.0 {
                self.pulse_phase -= 1.0;
            }
        } else {
            // Reset pulse when inactive
            self.pulse_phase = 0.0;
        }
    }

    /// Returns the glow intensity (0.0-1.0) based on smoothed activity.
    pub fn glow_intensity(&self) -> f32 {
        self.smoothed_activity.min(1.0)
    }

    /// Returns a color hint based on activity level.
    /// 0 = green (normal), 1 = yellow (elevated), 2 = red (clipping)
    pub fn color_hint(&self) -> u8 {
        if self.is_clipping {
            2 // Red/clipping (signal > 0dB)
        } else if self.smoothed_activity > 0.85 {
            1 // Yellow/elevated (approaching clipping, ~-1.5dB)
        } else {
            0 // Green/normal (< -1.5dB)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_control_set_all_channels() {
        let mut vol = VolumeControl::default();

        vol.set_all_channels(0.8);
        assert_eq!(vol.master, 0.8);
        assert_eq!(vol.channels[0], 0.8);

        // Clamping should work
        vol.set_all_channels(3.0);
        assert_eq!(vol.master, 2.0);
    }

    #[test]
    fn test_db_conversion() {
        // 0 dB = 1.0 linear
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.001);

        // Silent
        assert_eq!(linear_to_db(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn test_meter_data_update() {
        let mut meter = MeterData::default();

        meter.update(vec![0.5, 0.3], vec![0.3, 0.2]);
        assert_eq!(meter.max_peak(), 0.5);
        assert_eq!(meter.peak_hold[0], 0.5);

        // Peak hold should decay if new peak is lower
        meter.update(vec![0.2, 0.1], vec![0.1, 0.05]);
        assert!(meter.peak_hold[0] < 0.5);
        assert!(meter.peak_hold[0] > 0.2);
    }

    #[test]
    fn test_meter_data_last_update_tracking() {
        let mut meter = MeterData::default();
        let before = std::time::Instant::now();

        meter.update(vec![0.5, 0.5], vec![0.3, 0.3]);

        // last_update should be recent
        assert!(meter.last_update >= before);
        assert!(meter.last_update.elapsed().as_millis() < 100);
    }

    #[test]
    fn test_meter_data_decayed_peak_fresh() {
        let mut meter = MeterData::default();
        meter.update(vec![0.8, 0.6], vec![0.5, 0.4]);

        let threshold = std::time::Duration::from_millis(100);

        // Fresh meter should return unchanged peak
        let decayed = meter.get_decayed_peak(0, threshold);
        assert!((decayed - 0.8).abs() < 0.001, "Fresh meter should return unchanged value");

        let decayed_max = meter.get_decayed_max_peak(threshold);
        assert!((decayed_max - 0.8).abs() < 0.001, "Fresh meter max should return unchanged value");
    }

    #[test]
    fn test_meter_data_decayed_peak_stale() {
        let mut meter = MeterData::default();
        meter.update(vec![0.8, 0.6], vec![0.5, 0.4]);

        let threshold = std::time::Duration::from_millis(50);

        // Wait to become stale
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Should be significantly decayed after 100ms past threshold
        let decayed = meter.get_decayed_peak(0, threshold);
        assert!(decayed < 0.8, "Stale meter should have decayed peak");
        assert!(decayed < 0.7, "Should be significantly decayed after 100ms stale");

        // After enough time, should be nearly zero
        std::thread::sleep(std::time::Duration::from_millis(400));
        let very_decayed = meter.get_decayed_peak(0, threshold);
        assert!(very_decayed < 0.2, "Should be nearly zero after ~500ms stale");
    }

    #[test]
    fn test_meter_data_decayed_peak_channel_bounds() {
        let mut meter = MeterData::default();
        meter.update(vec![0.8, 0.6], vec![0.5, 0.4]);

        let threshold = std::time::Duration::from_millis(100);

        // Invalid channel should return 0
        let invalid = meter.get_decayed_peak(99, threshold);
        assert_eq!(invalid, 0.0, "Invalid channel should return 0.0");
    }

    #[test]
    fn test_link_meter_data_attack_behavior() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0; // 60 FPS

        // Start with no activity
        assert_eq!(link_meter.smoothed_activity, 0.0);

        // Apply sudden activity - should attack quickly
        link_meter.update(0.8, dt);

        // With attack_rate = 8.0, after one frame:
        // smoothed += (0.8 - 0.0) * 8.0 * (1/60) = 0.8 * 0.133 ≈ 0.107
        assert!(link_meter.smoothed_activity > 0.1, "Attack should be fast");
        assert!(link_meter.smoothed_activity < 0.2, "But not instant");

        // After several frames, should approach target
        for _ in 0..30 {
            link_meter.update(0.8, dt);
        }
        assert!(link_meter.smoothed_activity > 0.7, "Should approach target after 30 frames");
    }

    #[test]
    fn test_link_meter_data_decay_behavior() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Set to high activity first
        link_meter.smoothed_activity = 0.8;
        link_meter.activity = 0.8;

        // Now activity drops to zero - should decay slowly
        link_meter.update(0.0, dt);

        // With decay_rate = 2.0, after one frame:
        // smoothed += (0.0 - 0.8) * 2.0 * (1/60) = -0.8 * 0.033 ≈ -0.027
        // So smoothed ≈ 0.8 - 0.027 = 0.773
        assert!(link_meter.smoothed_activity < 0.8, "Should start decaying");
        assert!(link_meter.smoothed_activity > 0.7, "Decay should be slow");

        // After many frames, should approach zero
        for _ in 0..120 {
            link_meter.update(0.0, dt);
        }
        assert!(link_meter.smoothed_activity < 0.1, "Should approach zero after 2 seconds");
    }

    #[test]
    fn test_link_meter_data_pulse_phase_advances() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Set activity high enough for pulse
        link_meter.smoothed_activity = 0.5;
        link_meter.update(0.5, dt);

        let phase1 = link_meter.pulse_phase;

        link_meter.update(0.5, dt);
        let phase2 = link_meter.pulse_phase;

        assert!(phase2 > phase1, "Pulse phase should advance with activity");
    }

    #[test]
    fn test_link_meter_data_pulse_phase_resets_when_inactive() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Set some pulse phase
        link_meter.pulse_phase = 0.5;
        link_meter.smoothed_activity = 0.005; // Below threshold

        link_meter.update(0.0, dt);

        assert_eq!(link_meter.pulse_phase, 0.0, "Pulse should reset when inactive");
    }

    #[test]
    fn test_link_meter_data_clipping_detection() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Below clipping
        link_meter.update(0.9, dt);
        assert!(!link_meter.is_clipping);

        // At clipping
        link_meter.update(1.0, dt);
        assert!(!link_meter.is_clipping);

        // Above clipping
        link_meter.update(1.1, dt);
        assert!(link_meter.is_clipping);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_link_meter_data_color_hint() {
        let mut link_meter = LinkMeterData::default();

        // Normal (green)
        link_meter.smoothed_activity = 0.5;
        link_meter.is_clipping = false;
        assert_eq!(link_meter.color_hint(), 0);

        // Elevated (yellow) - approaching clipping
        link_meter.smoothed_activity = 0.9;
        assert_eq!(link_meter.color_hint(), 1);

        // Clipping (red)
        link_meter.is_clipping = true;
        assert_eq!(link_meter.color_hint(), 2);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_link_meter_data_glow_intensity_clamped() {
        let mut link_meter = LinkMeterData::default();

        link_meter.smoothed_activity = 0.5;
        assert_eq!(link_meter.glow_intensity(), 0.5);

        // Should clamp to 1.0 max
        link_meter.smoothed_activity = 1.5;
        assert_eq!(link_meter.glow_intensity(), 1.0);

        link_meter.smoothed_activity = 0.0;
        assert_eq!(link_meter.glow_intensity(), 0.0);
    }

    #[test]
    fn test_link_meter_smoothed_activity_never_negative() {
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Even with aggressive decay, should never go negative
        link_meter.smoothed_activity = 0.01;
        for _ in 0..1000 {
            link_meter.update(0.0, dt);
        }

        assert!(link_meter.smoothed_activity >= 0.0, "Should never be negative");
    }

    #[test]
    fn test_link_meter_full_cycle() {
        // Simulate a realistic usage cycle: silence -> music -> silence
        let mut link_meter = LinkMeterData::default();
        let dt = 1.0 / 60.0;

        // Initially silent
        assert_eq!(link_meter.glow_intensity(), 0.0);

        // Music starts - activity ramps up over ~0.5 seconds
        for _ in 0..30 {
            link_meter.update(0.6, dt);
        }
        let during_music = link_meter.glow_intensity();
        assert!(during_music > 0.4, "Should show significant activity during music");

        // Music stops - activity decays over ~1 second
        for _ in 0..60 {
            link_meter.update(0.0, dt);
        }
        let after_stop = link_meter.glow_intensity();
        assert!(after_stop < during_music, "Should decay after music stops");
        assert!(after_stop < 0.2, "Should be mostly faded after 1 second of silence");

        // After 2 seconds of silence, should be essentially zero
        for _ in 0..60 {
            link_meter.update(0.0, dt);
        }
        assert!(link_meter.glow_intensity() < 0.05, "Should be nearly zero after 2 seconds");
    }
}
