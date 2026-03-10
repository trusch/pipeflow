//! Domain layer for Pipeflow.
//!
//! This module contains core domain models independent of PipeWire:
//! - Graph model (Node, Port, Link)
//! - Audio control (Volume, Mute, Channels)
//! - Safety controller (Stage mode, Panic)
//! - Filtering and grouping logic

pub mod audio;
pub mod explain;
pub mod filters;
pub mod graph;
pub mod groups;
pub mod rules;
pub mod safety;
