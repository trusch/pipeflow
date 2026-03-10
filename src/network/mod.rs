//! Network module for remote pipeflow access.
//!
//! Provides gRPC server and client implementations for controlling
//! PipeWire remotely via SSH tunnel.

#[cfg(feature = "network")]
pub mod adapter;
#[cfg(feature = "network")]
pub mod client;
#[cfg(feature = "network")]
pub mod server;

/// Protocol version for compatibility checking between client and server.
pub const PROTOCOL_VERSION: u32 = 1;

/// Generated protobuf types and gRPC service definitions.
#[cfg(feature = "network")]
pub mod proto {
    tonic::include_proto!("pipeflow");
}

#[cfg(feature = "network")]
pub use client::RemoteConnection;
