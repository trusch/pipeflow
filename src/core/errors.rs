//! Typed error hierarchy for the Pipeflow application.
//!
//! Provides structured errors instead of stringly-typed error messages,
//! enabling pattern matching on error kinds in callers.

#![allow(dead_code)]

use crate::util::id::{LinkId, NodeId, PortId};
use thiserror::Error;

/// Top-level application error.
#[derive(Debug, Error)]
pub enum PipeflowError {
    /// A graph operation failed.
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),

    /// Configuration loading or saving failed.
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),

    /// PipeWire connection or communication failed.
    #[error("PipeWire error: {0}")]
    PipeWire(#[from] PipeWireError),
}

/// Errors related to graph operations.
#[derive(Debug, Error)]
pub enum GraphError {
    /// Attempted to operate on a node that doesn't exist.
    #[error("Node {0} not found")]
    NodeNotFound(NodeId),

    /// Attempted to operate on a port that doesn't exist.
    #[error("Port {0} not found")]
    PortNotFound(PortId),

    /// Attempted to operate on a link that doesn't exist.
    #[error("Link {0} not found")]
    LinkNotFound(LinkId),

    /// Attempted to create a link between incompatible ports.
    #[error("Cannot link port {output} to port {input}: {reason}")]
    IncompatiblePorts {
        /// The output port ID.
        output: PortId,
        /// The input port ID.
        input: PortId,
        /// Reason for incompatibility.
        reason: String,
    },
}

/// Errors related to configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to determine the config file path.
    #[error("Cannot determine config path: {0}")]
    PathResolution(String),

    /// Failed to read or write config file.
    #[error("Config I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse config file.
    #[error("Config parse error: {0}")]
    Parse(String),
}

/// Errors related to PipeWire connection.
#[derive(Debug, Error)]
pub enum PipeWireError {
    /// Connection to PipeWire daemon failed.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// PipeWire daemon disconnected unexpectedly.
    #[error("Disconnected from PipeWire")]
    Disconnected,

    /// A PipeWire command timed out.
    #[error("Command timed out")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = GraphError::NodeNotFound(NodeId::new(42));
        assert!(err.to_string().contains("42"));

        let err = GraphError::IncompatiblePorts {
            output: PortId::new(1),
            input: PortId::new(2),
            reason: "same direction".into(),
        };
        assert!(err.to_string().contains("same direction"));
    }

    #[test]
    fn test_error_conversion() {
        let graph_err = GraphError::NodeNotFound(NodeId::new(1));
        let app_err: PipeflowError = graph_err.into();
        assert!(matches!(app_err, PipeflowError::Graph(_)));
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::Parse("unexpected field".into());
        assert!(err.to_string().contains("unexpected field"));
    }

    #[test]
    fn test_pipewire_error_display() {
        let err = PipeWireError::Disconnected;
        assert!(err.to_string().contains("Disconnected"));
    }
}
