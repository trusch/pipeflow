//! PipeWire integration layer for Pipeflow.
//!
//! This module handles all communication with the PipeWire daemon:
//! - Connection management (MainLoop, Context, Core)
//! - Registry listener for graph updates
//! - Event types flowing from PipeWire to the app
//! - Signal metering (real and simulated)

pub mod connection;
pub mod events;
pub mod meter_stream;
pub mod meters;
