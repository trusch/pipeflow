//! PipeWire connection state.

/// PipeWire connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and receiving updates
    Connected,
    /// Connection error
    Error,
}

impl ConnectionState {
    /// Returns true if connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}
