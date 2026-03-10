//! Pipeflow - A next-generation PipeWire graph and control application.
//!
//! This library crate exposes the core domain types, state management,
//! and utilities used by the Pipeflow application. It is primarily used
//! for benchmarks and integration tests.
//!
//! # Module Organization
//!
//! - [`core`] - Application state management, commands, configuration, and typed errors
//! - [`domain`] - Domain models independent of PipeWire: graph, audio, safety, filters, groups, rules
//! - [`util`] - Shared utilities: ID types, spatial positioning, layout algorithms

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod core;
pub mod domain;
pub mod util;
