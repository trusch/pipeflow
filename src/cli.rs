//! Command-line interface parsing.
//!
//! Defines CLI arguments for different modes: GUI, headless server, and remote client.

use clap::Parser;
use std::net::SocketAddr;

/// Pipeflow - A next-generation PipeWire graph and control application.
#[derive(Parser, Debug, Clone)]
#[command(name = "pipeflow")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Run in headless mode (no GUI, gRPC server only)
    #[arg(long)]
    pub headless: bool,

    /// Bind address for gRPC server in headless mode
    #[arg(long, default_value = "127.0.0.1:50051")]
    pub bind: SocketAddr,

    /// Authentication token for gRPC connections
    #[arg(long, env = "PIPEFLOW_TOKEN")]
    pub token: Option<String>,

    /// Connect to a remote pipeflow instance via SSH tunnel
    #[arg(long, value_name = "USER@HOST")]
    pub remote: Option<String>,

    /// SSH port for remote connection
    #[arg(long, default_value = "22")]
    pub ssh_port: u16,

    /// Remote pipeflow gRPC port
    #[arg(long, default_value = "50051")]
    pub remote_port: u16,

    /// Local port for SSH tunnel forwarding
    #[arg(long, default_value = "50051")]
    pub local_port: u16,

    /// Path to SSH identity file (private key)
    #[arg(long, short = 'i')]
    pub identity: Option<String>,

    /// Enable verbose logging
    #[arg(long, short)]
    pub verbose: bool,
}

impl Cli {
    /// Returns the run mode based on CLI arguments.
    pub fn mode(&self) -> RunMode {
        if self.headless {
            RunMode::Headless
        } else if self.remote.is_some() {
            RunMode::Remote
        } else {
            RunMode::Local
        }
    }

    /// Parses the remote argument into user and host components.
    pub fn parse_remote(&self) -> Option<RemoteTarget> {
        self.remote.as_ref().map(|remote| {
            if let Some((user, host)) = remote.split_once('@') {
                RemoteTarget {
                    user: user.to_string(),
                    host: host.to_string(),
                    port: self.ssh_port,
                }
            } else {
                RemoteTarget {
                    user: std::env::var("USER").unwrap_or_else(|_| "root".to_string()),
                    host: remote.clone(),
                    port: self.ssh_port,
                }
            }
        })
    }
}

/// The mode in which pipeflow should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Normal GUI mode with local PipeWire connection
    Local,
    /// Headless server mode (gRPC server, no GUI)
    Headless,
    /// Remote client mode (GUI connecting to remote via SSH tunnel)
    Remote,
}

/// Parsed remote connection target.
#[derive(Debug, Clone)]
pub struct RemoteTarget {
    /// SSH username
    pub user: String,
    /// Remote host (hostname or IP)
    pub host: String,
    /// SSH port
    pub port: u16,
}

impl RemoteTarget {
    /// Returns the SSH connection string (user@host).
    pub fn ssh_target(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_local() {
        let cli = Cli::parse_from(["pipeflow"]);
        assert_eq!(cli.mode(), RunMode::Local);
    }

    #[test]
    fn test_headless_mode() {
        let cli = Cli::parse_from(["pipeflow", "--headless"]);
        assert_eq!(cli.mode(), RunMode::Headless);
        assert!(cli.headless);
    }

    #[test]
    fn test_remote_mode() {
        let cli = Cli::parse_from(["pipeflow", "--remote", "user@host.local"]);
        assert_eq!(cli.mode(), RunMode::Remote);

        let target = cli.parse_remote().unwrap();
        assert_eq!(target.user, "user");
        assert_eq!(target.host, "host.local");
        assert_eq!(target.port, 22);
    }

    #[test]
    fn test_remote_without_user() {
        std::env::set_var("USER", "testuser");
        let cli = Cli::parse_from(["pipeflow", "--remote", "192.168.1.100"]);
        let target = cli.parse_remote().unwrap();
        assert_eq!(target.user, "testuser");
        assert_eq!(target.host, "192.168.1.100");
    }

    #[test]
    fn test_custom_ports() {
        let cli = Cli::parse_from([
            "pipeflow",
            "--remote",
            "user@host",
            "--ssh-port",
            "2222",
            "--remote-port",
            "9090",
            "--local-port",
            "9091",
        ]);

        assert_eq!(cli.ssh_port, 2222);
        assert_eq!(cli.remote_port, 9090);
        assert_eq!(cli.local_port, 9091);
    }

    #[test]
    fn test_headless_bind_address() {
        let cli = Cli::parse_from(["pipeflow", "--headless", "--bind", "0.0.0.0:8080"]);
        assert_eq!(cli.bind.to_string(), "0.0.0.0:8080");
    }
}
