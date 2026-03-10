//! Utility module for Pipeflow.
//!
//! Contains shared utilities used across the application:
//! - Type-safe ID wrappers
//! - Spatial/layout algorithms
//! - Smart layout algorithms

pub mod id;
pub mod layout;
pub mod spatial;

pub use layout::is_metering_node;
