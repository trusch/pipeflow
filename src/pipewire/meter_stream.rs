//! Real-time audio meter streams.
//!
//! Creates PipeWire monitor streams to capture actual audio levels from nodes.
//! Uses the Stream API with capture mode to tap into audio without
//! affecting the normal signal flow.

use crate::pipewire::events::MeterUpdate;
use crate::util::id::NodeId;
use crossbeam::channel::Sender;
use libspa::param::format::{MediaSubtype, MediaType};
use libspa::param::format_utils;
use libspa::pod::Pod;
use pipewire::properties::properties;
use pipewire::spa;
use pipewire::stream::{Stream, StreamFlags, StreamListener};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Data passed to stream callbacks.
struct MeterStreamData {
    /// Node ID being monitored
    node_id: u32,
    /// Number of audio channels
    channels: u32,
    /// Sample rate
    sample_rate: u32,
    /// Current peak values per channel
    peaks: Vec<f32>,
    /// Current RMS values per channel
    rms: Vec<f32>,
    /// Whether we have new data
    dirty: bool,
    /// Audio format info for parsing
    format: spa::param::audio::AudioInfoRaw,
}

impl Default for MeterStreamData {
    fn default() -> Self {
        Self {
            node_id: 0,
            channels: 2,       // Default stereo; updated when stream format is negotiated
            sample_rate: 48000, // Common default; updated from actual stream format
            peaks: vec![0.0; 2],
            rms: vec![0.0; 2],
            dirty: false,
            format: Default::default(),
        }
    }
}

impl MeterStreamData {
    fn new(node_id: NodeId) -> Self {
        Self {
            node_id: node_id.raw(),
            channels: 2,       // Default stereo; updated when stream format is negotiated
            sample_rate: 48000, // Common default; updated from actual stream format
            peaks: vec![0.0; 2],
            rms: vec![0.0; 2],
            dirty: false,
            format: Default::default(),
        }
    }

    fn set_format(&mut self, channels: u32, sample_rate: u32) {
        if channels > 0 {
            self.channels = channels;
        }
        if sample_rate > 0 {
            self.sample_rate = sample_rate;
        }
        self.peaks.resize(self.channels as usize, 0.0);
        self.rms.resize(self.channels as usize, 0.0);
    }

    fn update_from_format(&mut self) {
        let channels = self.format.channels();
        let rate = self.format.rate();
        self.set_format(channels, rate);
    }

    fn update_levels(&mut self, channel: usize, peak: f32, rms_val: f32) {
        if channel < self.peaks.len() {
            self.peaks[channel] = peak;
            self.rms[channel] = rms_val;
            self.dirty = true;
        }
    }

    fn take_update(&mut self) -> Option<MeterUpdate> {
        if self.dirty {
            self.dirty = false;
            // Swap out current buffers with fresh zeroed ones to avoid cloning.
            // The old buffers become the update payload; new buffers are reused next cycle.
            let mut peak = vec![0.0; self.channels as usize];
            let mut rms = vec![0.0; self.channels as usize];
            std::mem::swap(&mut self.peaks, &mut peak);
            std::mem::swap(&mut self.rms, &mut rms);
            Some(MeterUpdate {
                node_id: NodeId::new(self.node_id),
                peak,
                rms,
            })
        } else {
            None
        }
    }
}

/// Shared meter data accessible from callbacks.
type SharedMeterData = Rc<RefCell<MeterStreamData>>;

/// Info about a registered node for metering.
#[derive(Clone)]
struct NodeMeterInfo {
    /// Object serial for targeting
    serial: String,
    /// Whether this is a sink node (vs a source)
    is_sink: bool,
}

/// A manager for meter streams within the PipeWire thread.
pub struct MeterStreamManager {
    /// Active meter streams by node ID
    streams: HashMap<NodeId, MeterStreamHandle>,
    /// Channel for sending meter updates
    update_tx: Sender<Vec<MeterUpdate>>,
    /// Node info for targeting (node_id -> NodeMeterInfo)
    node_info: HashMap<NodeId, NodeMeterInfo>,
    /// Whether to automatically meter new nodes when registered
    auto_meter_all: bool,
}

/// Handle to a meter stream.
struct MeterStreamHandle {
    /// The actual PipeWire stream
    _stream: Stream,
    /// Stream data for reading levels
    data: SharedMeterData,
    /// Listener to keep alive
    _listener: StreamListener<SharedMeterData>,
    /// Last time this stream produced data (for staleness detection)
    last_data_time: std::time::Instant,
}

impl MeterStreamManager {
    /// Creates a new meter stream manager.
    pub fn new(update_tx: Sender<Vec<MeterUpdate>>) -> Self {
        Self {
            streams: HashMap::new(),
            update_tx,
            node_info: HashMap::new(),
            auto_meter_all: false,
        }
    }

    /// Registers and optionally auto-starts metering for a node.
    /// Call this with core when auto-metering is needed.
    /// `is_sink` should be true for AudioSink/StreamInputAudio nodes, false for sources.
    pub fn register_and_auto_meter(
        &mut self,
        core: &pipewire::core::Core,
        node_id: NodeId,
        serial: String,
        is_sink: bool,
    ) {
        self.node_info.insert(node_id, NodeMeterInfo { serial, is_sink });
        if self.auto_meter_all {
            self.start_metering(core, node_id);
        }
    }

    /// Enables or disables auto-metering for new nodes.
    pub fn set_auto_meter_all(&mut self, enabled: bool) {
        self.auto_meter_all = enabled;
    }

    /// Removes a node's info.
    pub fn unregister_node(&mut self, node_id: &NodeId) {
        self.node_info.remove(node_id);
        self.stop_metering(node_id);
    }

    /// Starts metering a node.
    pub fn start_metering(&mut self, core: &pipewire::core::Core, node_id: NodeId) -> bool {
        // Already metering this node
        if self.streams.contains_key(&node_id) {
            return true;
        }

        // Get the node's info
        let info = match self.node_info.get(&node_id) {
            Some(i) => i.clone(),
            None => {
                tracing::warn!("No info for node {:?}, cannot create meter stream", node_id);
                return false;
            }
        };

        // Check if we already have a meter stream targeting this serial (e.g., node restarted with new ID)
        // If so, clean up the stale meter to avoid duplicates
        let stale_node_ids: Vec<NodeId> = self
            .node_info
            .iter()
            .filter(|(other_id, other_info)| {
                *other_id != &node_id
                    && other_info.serial == info.serial
                    && self.streams.contains_key(other_id)
            })
            .map(|(id, _)| *id)
            .collect();

        for stale_id in stale_node_ids {
            tracing::info!(
                "Cleaning up stale meter for node {:?} (serial {} now owned by {:?})",
                stale_id,
                info.serial,
                node_id
            );
            self.streams.remove(&stale_id);
        }

        // Create the meter stream
        match self.create_meter_stream(core, node_id, &info.serial, info.is_sink) {
            Some(handle) => {
                self.streams.insert(node_id, handle);
                tracing::debug!("Started metering node {:?} (is_sink={})", node_id, info.is_sink);
                true
            }
            None => {
                tracing::warn!("Failed to create meter stream for node {:?}", node_id);
                false
            }
        }
    }

    /// Stops metering a node.
    pub fn stop_metering(&mut self, node_id: &NodeId) {
        if self.streams.remove(node_id).is_some() {
            tracing::debug!("Stopped metering node {:?}", node_id);
        }
    }

    /// Collects all pending meter updates and sends them.
    pub fn collect_and_send_updates(&mut self) {
        let now = std::time::Instant::now();
        let mut updates = Vec::new();

        for (_, handle) in self.streams.iter_mut() {
            if let Ok(mut data) = handle.data.try_borrow_mut() {
                if let Some(update) = data.take_update() {
                    handle.last_data_time = now;
                    updates.push(update);
                }
            }
        }

        if !updates.is_empty() {
            // Silently drop if channel is full - this is expected under high load
            let _ = self.update_tx.try_send(updates);
        }
    }

    /// Restarts meter streams that haven't produced data recently.
    /// PipeWire streams can silently stop producing data when the audio graph
    /// is reconfigured (e.g., sample rate changes, device switches).
    pub fn restart_stale_streams(&mut self, core: &pipewire::core::Core, stale_after: std::time::Duration) {
        let stale_ids: Vec<NodeId> = self
            .streams
            .iter()
            .filter(|(_, handle)| handle.last_data_time.elapsed() > stale_after)
            .map(|(id, _)| *id)
            .collect();

        for node_id in stale_ids {
            tracing::info!("Restarting stale meter stream for node {:?}", node_id);
            self.streams.remove(&node_id);
            self.start_metering(core, node_id);
        }
    }

    /// Creates a meter stream for a node.
    fn create_meter_stream(
        &self,
        core: &pipewire::core::Core,
        node_id: NodeId,
        serial: &str,
        is_sink: bool,
    ) -> Option<MeterStreamHandle> {
        // Stream properties for monitoring a specific node
        // Use TARGET_OBJECT with the serial number for targeting
        //
        // For SINK nodes (e.g., speakers, apps receiving audio):
        //   Use STREAM_CAPTURE_SINK=true to capture the monitor output
        //
        // For SOURCE nodes (e.g., mics, apps outputting audio like SuperCollider):
        //   Don't use STREAM_CAPTURE_SINK - capture directly from the output
        let props = if is_sink {
            properties! {
                *pipewire::keys::TARGET_OBJECT => serial,
                *pipewire::keys::STREAM_CAPTURE_SINK => "true",
                *pipewire::keys::NODE_NAME => format!("pipeflow-meter-{}", node_id.raw()),
                *pipewire::keys::MEDIA_TYPE => "Audio",
                *pipewire::keys::MEDIA_CATEGORY => "Capture",
                *pipewire::keys::MEDIA_ROLE => "Music",
            }
        } else {
            properties! {
                *pipewire::keys::TARGET_OBJECT => serial,
                *pipewire::keys::NODE_NAME => format!("pipeflow-meter-{}", node_id.raw()),
                *pipewire::keys::MEDIA_TYPE => "Audio",
                *pipewire::keys::MEDIA_CATEGORY => "Capture",
                *pipewire::keys::MEDIA_ROLE => "Music",
            }
        };
        tracing::debug!(
            "Creating meter stream for node {:?} (id: {}, serial: {}, is_sink: {})",
            node_id,
            node_id.raw(),
            serial,
            is_sink
        );

        // Create stream
        let stream = match Stream::new(core, "pipeflow-meter", props) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to create meter stream: {}", e);
                return None;
            }
        };

        // Shared data
        let data: SharedMeterData = Rc::new(RefCell::new(MeterStreamData::new(node_id)));
        let data_for_listener = data.clone();

        // Set up the listener with the shared data
        let listener = stream
            .add_local_listener_with_user_data(data_for_listener)
            .state_changed(|_stream, user_data, old_state, new_state| {
                // Use try_borrow to avoid conflicts with other callbacks
                if let Ok(data) = user_data.try_borrow() {
                    tracing::debug!(
                        "Meter stream for node {} state: {:?} -> {:?}",
                        data.node_id,
                        old_state,
                        new_state
                    );
                }
            })
            .param_changed(|_stream, user_data, id, pod| {
                // NULL means to clear the format
                let Some(param) = pod else {
                    return;
                };
                if id != libspa::param::ParamType::Format.as_raw() {
                    return;
                }

                let (media_type, media_subtype) = match format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(_) => return,
                };

                // Only accept raw audio
                if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                    return;
                }

                // Use try_borrow_mut to avoid conflicts with other callbacks
                let Ok(mut data) = user_data.try_borrow_mut() else {
                    return;
                };
                // Parse the format parameters
                if let Err(e) = data.format.parse(param) {
                    tracing::warn!("Failed to parse audio format: {:?}", e);
                    return;
                }

                data.update_from_format();
                tracing::debug!(
                    "Meter stream for node {} format: rate={} channels={}",
                    data.node_id,
                    data.format.rate(),
                    data.format.channels()
                );
            })
            .process(|stream, user_data| {
                // Process audio buffers
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    // No buffer available - this is normal when no audio is flowing
                    return;
                };

                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                // Use try_borrow_mut to avoid conflicts
                let Ok(mut data) = user_data.try_borrow_mut() else {
                    return;
                };
                let channels = data.channels as usize;
                let num_data_planes = datas.len();

                // Detect format: planar (one buffer per channel) vs interleaved (all channels in one buffer)
                if num_data_planes >= channels {
                    // Planar format: each data plane is a separate channel
                    for (ch, d) in datas.iter_mut().enumerate() {
                        if ch >= channels {
                            break;
                        }

                        let chunk = d.chunk();
                        let size = chunk.size() as usize;

                        if size == 0 {
                            continue;
                        }

                        if let Some(slice) = d.data() {
                            let actual_data = &slice[..size.min(slice.len())];

                            if actual_data.len() >= 4 {
                                let samples: &[f32] = bytemuck::cast_slice(actual_data);
                                if !samples.is_empty() {
                                    let (peak, rms) = calculate_levels(samples);
                                    data.update_levels(ch, peak, rms);
                                }
                            }
                        }
                    }
                } else if num_data_planes == 1 && channels > 1 {
                    // Interleaved format: all channels in one buffer, samples alternate
                    // Layout: [ch0_s0, ch1_s0, ch2_s0, ch3_s0, ch0_s1, ch1_s1, ...]
                    let d = &mut datas[0];
                    let chunk = d.chunk();
                    let size = chunk.size() as usize;

                    if size > 0 {
                        if let Some(slice) = d.data() {
                            let actual_data = &slice[..size.min(slice.len())];

                            if actual_data.len() >= 4 {
                                let samples: &[f32] = bytemuck::cast_slice(actual_data);
                                let num_frames = samples.len() / channels;

                                if num_frames > 0 {
                                    // Calculate levels for each channel by deinterleaving
                                    for ch in 0..channels {
                                        let (peak, rms) = calculate_levels_interleaved(samples, ch, channels);
                                        data.update_levels(ch, peak, rms);
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .register();

        let listener = match listener {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to register meter stream listener: {}", e);
                return None;
            }
        };

        // Build audio format parameters - request F32LE format, accept any rate/channels
        let mut audio_info = spa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(spa::param::audio::AudioFormat::F32LE);

        let obj = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        };

        let values: Vec<u8> = match spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(obj),
        ) {
            Ok(serialized) => serialized.0.into_inner(),
            Err(e) => {
                tracing::error!("Failed to serialize audio format: {:?}", e);
                return None;
            }
        };

        let format_pod = match Pod::from_bytes(&values) {
            Some(pod) => pod,
            None => {
                tracing::error!("Failed to create Pod from serialized audio format");
                return None;
            }
        };

        let mut params = [format_pod];

        // Use AUTOCONNECT to connect to the target and MAP_BUFFERS to access audio data
        match stream.connect(
            spa::utils::Direction::Input,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        ) {
            Ok(_) => {
                tracing::info!(
                    "Meter stream connected for node {:?} (serial: {})",
                    node_id,
                    serial
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to connect meter stream for node {:?}: {}",
                    node_id,
                    e
                );
                return None;
            }
        }

        Some(MeterStreamHandle {
            _stream: stream,
            data,
            _listener: listener,
            last_data_time: std::time::Instant::now(),
        })
    }

    /// Returns the number of active meter streams.
    pub fn active_count(&self) -> usize {
        self.streams.len()
    }

    /// Returns all registered node IDs.
    pub fn registered_node_ids(&self) -> Vec<NodeId> {
        self.node_info.keys().copied().collect()
    }

    /// Stops metering all nodes.
    pub fn stop_all(&mut self) {
        self.streams.clear();
        tracing::debug!("Stopped all meter streams");
    }

    /// Cleans up stale meters for nodes that are no longer registered.
    /// This handles cases where nodes were removed but cleanup didn't complete.
    pub fn cleanup_stale_meters(&mut self) {
        let stale_ids: Vec<NodeId> = self
            .streams
            .keys()
            .filter(|id| !self.node_info.contains_key(id))
            .copied()
            .collect();

        if !stale_ids.is_empty() {
            tracing::info!("Cleaning up {} stale meter streams", stale_ids.len());
            for id in stale_ids {
                self.streams.remove(&id);
            }
        }
    }
}

/// Calculates peak and RMS levels from audio samples (planar format).
fn calculate_levels(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }

    let mut peak: f32 = 0.0;
    let mut sum_squares: f32 = 0.0;

    for &sample in samples {
        let abs = sample.abs();
        if abs > peak {
            peak = abs;
        }
        sum_squares += sample * sample;
    }

    let rms = (sum_squares / samples.len() as f32).sqrt();

    (peak, rms)
}

/// Calculates peak and RMS levels for a specific channel from interleaved audio samples.
/// Interleaved format: [ch0_s0, ch1_s0, ch2_s0, ..., ch0_s1, ch1_s1, ch2_s1, ...]
fn calculate_levels_interleaved(samples: &[f32], channel: usize, num_channels: usize) -> (f32, f32) {
    if samples.is_empty() || num_channels == 0 || channel >= num_channels {
        return (0.0, 0.0);
    }

    let mut peak: f32 = 0.0;
    let mut sum_squares: f32 = 0.0;
    let mut count: usize = 0;

    // Step through samples, extracting only this channel's values
    let mut i = channel;
    while i < samples.len() {
        let sample = samples[i];
        let abs = sample.abs();
        if abs > peak {
            peak = abs;
        }
        sum_squares += sample * sample;
        count += 1;
        i += num_channels;
    }

    let rms = if count > 0 {
        (sum_squares / count as f32).sqrt()
    } else {
        0.0
    };

    (peak, rms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_levels() {
        // Silence
        let (peak, rms) = calculate_levels(&[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(peak, 0.0);
        assert_eq!(rms, 0.0);

        // Simple sine-like pattern
        let samples = vec![0.5, -0.5, 0.5, -0.5];
        let (peak, rms) = calculate_levels(&samples);
        assert!((peak - 0.5).abs() < 0.001);
        assert!((rms - 0.5).abs() < 0.001); // RMS of +/-0.5 is 0.5

        // Peak detection
        let samples = vec![0.1, 0.2, 0.8, 0.3, -0.9];
        let (peak, _) = calculate_levels(&samples);
        assert!((peak - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_calculate_levels_interleaved() {
        // 4-channel interleaved audio: [ch0, ch1, ch2, ch3, ch0, ch1, ch2, ch3, ...]
        // Channel 0: 0.1, 0.2 (peak 0.2)
        // Channel 1: 0.5, 0.5 (peak 0.5)
        // Channel 2: 0.0, 0.0 (silence)
        // Channel 3: 0.8, 0.3 (peak 0.8)
        let samples = vec![
            0.1, 0.5, 0.0, 0.8,  // frame 0
            0.2, 0.5, 0.0, 0.3,  // frame 1
        ];

        let (peak0, _) = calculate_levels_interleaved(&samples, 0, 4);
        let (peak1, _) = calculate_levels_interleaved(&samples, 1, 4);
        let (peak2, _) = calculate_levels_interleaved(&samples, 2, 4);
        let (peak3, _) = calculate_levels_interleaved(&samples, 3, 4);

        assert!((peak0 - 0.2).abs() < 0.001, "Channel 0 peak: {}", peak0);
        assert!((peak1 - 0.5).abs() < 0.001, "Channel 1 peak: {}", peak1);
        assert!((peak2 - 0.0).abs() < 0.001, "Channel 2 peak: {}", peak2);
        assert!((peak3 - 0.8).abs() < 0.001, "Channel 3 peak: {}", peak3);

        // Test edge cases
        let (peak, rms) = calculate_levels_interleaved(&[], 0, 4);
        assert_eq!(peak, 0.0);
        assert_eq!(rms, 0.0);

        let (peak, rms) = calculate_levels_interleaved(&samples, 5, 4); // invalid channel
        assert_eq!(peak, 0.0);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn test_meter_stream_data() {
        let mut data = MeterStreamData::new(NodeId::new(42));
        assert_eq!(data.node_id, 42);
        assert!(!data.dirty);

        data.update_levels(0, 0.5, 0.3);
        assert!(data.dirty);

        let update = data.take_update();
        assert!(update.is_some());
        assert!(!data.dirty);

        let update = update.unwrap();
        assert_eq!(update.node_id.raw(), 42);
        assert_eq!(update.peak[0], 0.5);
        assert_eq!(update.rms[0], 0.3);
    }
}
