//! Application initialization and setup.
//!
//! Contains constructors for different run modes and egui configuration.

use crate::core::config::Config;
use crate::core::state::{create_shared_state, ConnectionState, SharedState};
use crate::pipewire::connection::PwConnection;
use crate::pipewire::meters::{MeterCollector, MeterConfig};

use super::{AppComponents, PipeflowApp};
use crate::ui::toolbar::SessionPresence;

impl PipeflowApp {
    /// Creates a new application instance in local mode.
    ///
    /// Connects to the local PipeWire daemon and initializes all UI components.
    /// Falls back to disconnected state if PipeWire connection fails.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_config();
        Self::configure_egui(&cc.egui_ctx);

        let state = create_shared_state();
        let (needs_initial_layout, saved_zoom, saved_pan) = load_saved_layout(&state);

        // Initialize PipeWire connection
        let pw_connection = match PwConnection::new() {
            Ok(conn) => conn,
            Err(e) => {
                tracing::error!("Failed to create PipeWire connection: {}", e);
                return Self::new_disconnected(
                    cc,
                    config,
                    state,
                    needs_initial_layout,
                    saved_zoom,
                    saved_pan,
                );
            }
        };

        let command_handler =
            crate::core::commands::CommandHandler::new(pw_connection.command_tx.clone());

        // Set initial safety mode from config
        {
            let mut state = state.write();
            state.safety.set_mode(config.behavior.startup_safety_mode);
        }

        // Create and start meter collector
        let meter_config = MeterConfig {
            enabled: config.meters.enabled,
            refresh_rate: config.meters.refresh_rate,
            buffer_size: 4,
        };
        let mut meter_collector = MeterCollector::new(meter_config);
        meter_collector.start();

        let components = AppComponents::new(saved_zoom, saved_pan, config.clone());

        Self {
            state,
            pw_connection: Some(pw_connection),
            #[cfg(feature = "network")]
            remote_connection: None,
            command_handler: Some(command_handler),
            is_remote: false,
            session_presence: SessionPresence {
                is_remote: false,
                target_label: Some("This machine".to_string()),
                transport_label: Some("Local PipeWire session".to_string()),
            },
            meter_collector,
            config,
            needs_initial_layout,
            components,
        }
    }

    /// Creates a new application instance without a PipeWire connection.
    ///
    /// Used when connection fails at startup or in disconnected mode.
    pub(super) fn new_disconnected(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        state: SharedState,
        needs_initial_layout: bool,
        saved_zoom: f32,
        saved_pan: egui::Vec2,
    ) -> Self {
        Self::configure_egui(&cc.egui_ctx);

        // Create meter collector (disabled since no connection)
        let meter_config = MeterConfig {
            enabled: false,
            refresh_rate: config.meters.refresh_rate,
            buffer_size: 4,
        };
        let meter_collector = MeterCollector::new(meter_config);

        // Mark connection as disconnected
        {
            let mut state = state.write();
            state.connection = ConnectionState::Disconnected;
        }

        let components = AppComponents::new(saved_zoom, saved_pan, config.clone());

        Self {
            state,
            pw_connection: None,
            #[cfg(feature = "network")]
            remote_connection: None,
            command_handler: None,
            is_remote: false,
            session_presence: SessionPresence {
                is_remote: false,
                target_label: Some("This machine".to_string()),
                transport_label: Some("Local PipeWire session".to_string()),
            },
            meter_collector,
            config,
            needs_initial_layout,
            components,
        }
    }

    /// Creates a new application instance as a client to a local headless instance.
    ///
    /// Connects to a local Pipeflow headless server via gRPC, but presents
    /// as a local (non-remote) session.
    #[cfg(feature = "network")]
    pub fn new_local_client(
        cc: &eframe::CreationContext<'_>,
        addr: &str,
        token: Option<String>,
    ) -> Self {
        use crate::network::RemoteConnection;

        let config = load_config();
        Self::configure_egui(&cc.egui_ctx);

        let state = create_shared_state();
        let (needs_initial_layout, saved_zoom, saved_pan) = load_saved_layout(&state);

        // Set initial safety mode from config
        {
            let mut state = state.write();
            state.safety.set_mode(config.behavior.startup_safety_mode);
        }

        // Connect to local headless server
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create tokio runtime: {}", e);
                return Self::new_disconnected(
                    cc,
                    config,
                    state,
                    needs_initial_layout,
                    saved_zoom,
                    saved_pan,
                );
            }
        };

        let remote_connection = runtime.block_on(async {
            match RemoteConnection::connect(addr, token).await {
                Ok(conn) => {
                    tracing::info!("Connected to local headless pipeflow server");
                    Some(conn)
                }
                Err(e) => {
                    tracing::error!("Failed to connect to local headless server: {}", e);
                    None
                }
            }
        });

        let command_handler = remote_connection
            .as_ref()
            .map(|conn| crate::core::commands::CommandHandler::new(conn.command_tx.clone()));

        // Create meter collector (disabled - meters come from headless server)
        let meter_config = MeterConfig {
            enabled: false,
            refresh_rate: 30,
            buffer_size: 4,
        };
        let meter_collector = MeterCollector::new(meter_config);

        let components = AppComponents::new(saved_zoom, saved_pan, config.clone());

        Self {
            state,
            pw_connection: None,
            remote_connection,
            command_handler,
            is_remote: false,
            session_presence: SessionPresence {
                is_remote: false,
                target_label: Some("localhost".to_string()),
                transport_label: Some("Connected to local headless service".to_string()),
            },
            meter_collector,
            config,
            needs_initial_layout,
            components,
        }
    }

    /// Creates a new application instance in remote mode.
    ///
    /// Connects to a remote Pipeflow server via gRPC.
    #[cfg(feature = "network")]
    pub fn new_remote(
        cc: &eframe::CreationContext<'_>,
        addr: &str,
        remote_target: &str,
        token: Option<String>,
    ) -> Self {
        use crate::network::RemoteConnection;

        let config = load_config();
        Self::configure_egui(&cc.egui_ctx);

        let state = create_shared_state();
        let (needs_initial_layout, saved_zoom, saved_pan) = load_saved_layout(&state);

        // Set initial safety mode from config
        {
            let mut state = state.write();
            state.safety.set_mode(config.behavior.startup_safety_mode);
        }

        // Connect to remote server
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create tokio runtime: {}", e);
                return Self::new_disconnected(
                    cc,
                    config,
                    state,
                    needs_initial_layout,
                    saved_zoom,
                    saved_pan,
                );
            }
        };

        let remote_connection = runtime.block_on(async {
            match RemoteConnection::connect(addr, token).await {
                Ok(conn) => {
                    tracing::info!("Connected to remote pipeflow server");
                    Some(conn)
                }
                Err(e) => {
                    tracing::error!("Failed to connect to remote server: {}", e);
                    None
                }
            }
        });

        let command_handler = remote_connection
            .as_ref()
            .map(|conn| crate::core::commands::CommandHandler::new(conn.command_tx.clone()));

        // Create meter collector (disabled for remote - meters come from server)
        let meter_config = MeterConfig {
            enabled: false,
            refresh_rate: 30,
            buffer_size: 4,
        };
        let meter_collector = MeterCollector::new(meter_config);

        let components = AppComponents::new(saved_zoom, saved_pan, config.clone());

        Self {
            state,
            pw_connection: None,
            remote_connection,
            command_handler,
            is_remote: true,
            session_presence: SessionPresence {
                is_remote: true,
                target_label: Some(remote_target.to_string()),
                transport_label: Some(format!("SSH tunnel via {}", addr)),
            },
            meter_collector,
            config,
            needs_initial_layout,
            components,
        }
    }

    /// Configures egui styling.
    pub(super) fn configure_egui(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        // Window and menu rounding is now handled via widget styling
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        ctx.set_style(style);

        // Register Phosphor icon font
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        ctx.set_fonts(fonts);
    }
}

/// Loads configuration from disk, falling back to defaults on error.
fn load_config() -> Config {
    Config::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config, using defaults: {}", e);
        Config::default()
    })
}

/// Loads saved layout from disk.
///
/// Returns (needs_initial_layout, saved_zoom, saved_pan).
fn load_saved_layout(state: &SharedState) -> (bool, f32, egui::Vec2) {
    let mut needs_initial_layout = true;
    let mut saved_zoom = 1.0_f32;
    let mut saved_pan = egui::Vec2::ZERO;

    if let Ok(manager) = crate::core::config::LayoutManager::new() {
        if let Ok(saved_ui) = manager.load() {
            let mut state = state.write();
            needs_initial_layout = !saved_ui.initial_layout_done;
            saved_zoom = saved_ui.zoom;
            saved_pan = egui::Vec2::new(saved_ui.pan.x, saved_ui.pan.y);
            tracing::info!(
                "Loaded saved layout (initial_layout_done: {}, zoom: {:.2})",
                saved_ui.initial_layout_done,
                saved_zoom
            );
            state.ui = saved_ui;
        }
    }

    (needs_initial_layout, saved_zoom, saved_pan)
}
