//! Pipeflow - A next-generation PipeWire graph and control application.
//!
//! This application provides full read/write control over PipeWire graphs
//! with visual routing, live control, and reproducibility features.
//!
//! # Run Modes
//!
//! - **Local mode** (default): GUI with local PipeWire connection
//! - **Headless mode** (`--headless`): gRPC server without GUI
//! - **Remote mode** (`--remote`): GUI connecting to remote server via SSH

#![warn(missing_docs)]
#![warn(clippy::all)]

mod app;
mod cli;
mod core;
mod domain;
#[cfg(feature = "network")]
mod headless;
mod icon;
#[cfg(feature = "network")]
mod network;
mod pipewire;
#[cfg(feature = "network")]
mod ssh;
mod ui;
mod util;

use clap::Parser;
use cli::{Cli, RunMode};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose);

    // Run based on mode
    let result = match cli.mode() {
        RunMode::Local => run_local_gui(),
        RunMode::Headless => run_headless(&cli),
        RunMode::Remote => run_remote(&cli),
    };

    if let Err(e) = result {
        tracing::error!("Fatal error: {}", e);
        std::process::exit(1);
    }
}

/// Runs the application in local GUI mode.
fn run_local_gui() -> Result<(), String> {
    tracing::info!("Starting Pipeflow in local GUI mode");

    // Create application icon
    let app_icon = icon::create_app_icon();

    // Configure eframe options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Pipeflow")
            .with_icon(std::sync::Arc::new(app_icon))
            .with_app_id("pipeflow"),
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Pipeflow",
        options,
        Box::new(|cc| Ok(Box::new(app::PipeflowApp::new(cc)))),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Runs the application in headless server mode.
#[cfg(feature = "network")]
fn run_headless(cli: &Cli) -> Result<(), String> {
    tracing::info!("Starting Pipeflow in headless mode on {}", cli.bind);

    // Create tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    runtime
        .block_on(headless::run_headless(cli.bind, cli.token.clone()))
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(feature = "network"))]
fn run_headless(_cli: &Cli) -> Result<(), String> {
    Err("Headless mode requires the 'network' feature. Rebuild with: cargo build --features network".to_string())
}

/// Runs the application in remote client mode.
#[cfg(feature = "network")]
fn run_remote(cli: &Cli) -> Result<(), String> {
    let remote_target = cli.parse_remote().ok_or("Invalid remote target")?;

    tracing::info!(
        "Starting Pipeflow in remote mode, connecting to {}",
        remote_target.ssh_target()
    );

    // Create tokio runtime for SSH and gRPC
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    // Set up SSH tunnel and connect
    let local_addr = format!("127.0.0.1:{}", cli.local_port);

    runtime
        .block_on(async {
            // Start SSH tunnel
            let mut tunnel = ssh::SshTunnel::new(
                &remote_target.host,
                remote_target.port,
                &remote_target.user,
                cli.identity.as_deref(),
                cli.local_port,
                cli.remote_port,
            );

            tracing::info!("Establishing SSH tunnel...");
            tunnel.start().await?;
            tracing::info!("SSH tunnel established");

            // Start headless pipeflow on remote if needed
            let remote_cmd = format!("pipeflow --headless --bind 127.0.0.1:{}", cli.remote_port);
            tunnel.run_remote_command(&remote_cmd).await?;

            // Give the remote server time to start
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .map_err(|e| e.to_string())?;

    // Now run the GUI connecting to the local forwarded port
    run_remote_gui(&local_addr, cli.token.clone())
}

#[cfg(not(feature = "network"))]
fn run_remote(_cli: &Cli) -> Result<(), String> {
    Err(
        "Remote mode requires the 'network' feature. Rebuild with: cargo build --features network"
            .to_string(),
    )
}

/// Runs the GUI in remote client mode.
#[cfg(feature = "network")]
fn run_remote_gui(addr: &str, token: Option<String>) -> Result<(), String> {
    tracing::info!("Starting remote GUI, connecting to {}", addr);

    // Create application icon
    let app_icon = icon::create_app_icon();

    let title = format!("Pipeflow (Remote: {})", addr);

    // Configure eframe options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title(&title)
            .with_icon(std::sync::Arc::new(app_icon))
            .with_app_id("pipeflow-remote"),
        ..Default::default()
    };

    let addr_owned = addr.to_string();

    // Run the application with remote connection
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::PipeflowApp::new_remote(
                cc,
                &addr_owned,
                token.clone(),
            )))
        }),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Initializes the logging system.
fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("pipeflow=debug,info"))
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("pipeflow=info,warn"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .with(filter)
        .init();
}
