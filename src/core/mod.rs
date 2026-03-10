//! Application core layer for Pipeflow.
//!
//! This module contains central application logic including:
//! - State management (AppState, GraphState, UiState)
//! - Command pattern infrastructure
//! - Configuration management

pub mod commands;
pub mod config;
pub mod errors;
pub mod state;
