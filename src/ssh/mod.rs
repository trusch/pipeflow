//! SSH tunnel management for remote pipeflow connections.
//!
//! This module provides functionality to establish SSH tunnels
//! for secure communication between local and remote pipeflow instances.

#[cfg(feature = "network")]
use std::process::Stdio;
#[cfg(feature = "network")]
use tokio::process::{Child, Command};
#[cfg(feature = "network")]
use tokio::time::{sleep, Duration};

/// SSH tunnel for port forwarding.
#[cfg(feature = "network")]
pub struct SshTunnel {
    /// Remote host
    host: String,
    /// SSH port
    ssh_port: u16,
    /// SSH username
    user: String,
    /// Path to identity file (optional)
    identity: Option<String>,
    /// Local port to forward
    local_port: u16,
    /// Remote port to connect to
    remote_port: u16,
    /// SSH process handle
    process: Option<Child>,
}

#[cfg(feature = "network")]
impl SshTunnel {
    /// Creates a new SSH tunnel configuration.
    pub fn new(
        host: &str,
        ssh_port: u16,
        user: &str,
        identity: Option<&str>,
        local_port: u16,
        remote_port: u16,
    ) -> Self {
        Self {
            host: host.to_string(),
            ssh_port,
            user: user.to_string(),
            identity: identity.map(|s| s.to_string()),
            local_port,
            remote_port,
            process: None,
        }
    }

    /// Starts the SSH tunnel.
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = Command::new("ssh");

        // Basic options
        cmd.arg("-N") // Don't execute remote command
            .arg("-T") // Disable pseudo-terminal allocation
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes");

        // Port forwarding: local_port -> remote:remote_port
        cmd.arg("-L").arg(format!(
            "{}:127.0.0.1:{}",
            self.local_port, self.remote_port
        ));

        // SSH port if non-standard
        if self.ssh_port != 22 {
            cmd.arg("-p").arg(self.ssh_port.to_string());
        }

        // Identity file
        if let Some(ref identity) = self.identity {
            cmd.arg("-i").arg(identity);
        }

        // Target
        cmd.arg(format!("{}@{}", self.user, self.host));

        // Configure stdio
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        tracing::debug!("Starting SSH tunnel: {:?}", cmd);

        let child = cmd.spawn()?;
        self.process = Some(child);

        // Wait a moment for the tunnel to establish
        sleep(Duration::from_millis(500)).await;

        // Check if process is still running
        if let Some(ref mut process) = self.process {
            match process.try_wait()? {
                Some(status) => {
                    return Err(format!(
                        "SSH tunnel failed to start: exit code {:?}",
                        status.code()
                    )
                    .into());
                }
                None => {
                    tracing::info!(
                        "SSH tunnel established: localhost:{} -> {}:{}",
                        self.local_port,
                        self.host,
                        self.remote_port
                    );
                }
            }
        }

        Ok(())
    }

    /// Runs a command on the remote host via SSH.
    pub async fn run_remote_command(
        &self,
        command: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = Command::new("ssh");

        // Basic options
        cmd.arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("BatchMode=yes");

        // SSH port if non-standard
        if self.ssh_port != 22 {
            cmd.arg("-p").arg(self.ssh_port.to_string());
        }

        // Identity file
        if let Some(ref identity) = self.identity {
            cmd.arg("-i").arg(identity);
        }

        // Target and command
        cmd.arg(format!("{}@{}", self.user, self.host));

        // Run the command in background with nohup
        // Check if pipeflow is already running first
        let check_and_run = format!(
            "pgrep -f 'pipeflow --headless' > /dev/null || nohup {} > /dev/null 2>&1 &",
            command
        );
        cmd.arg("sh").arg("-c").arg(&check_and_run);

        tracing::debug!("Running remote command: {:?}", cmd);

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Remote command returned non-zero: {}", stderr);
            // Don't fail - the command might have started successfully in background
        }

        Ok(())
    }
}

#[cfg(feature = "network")]
impl Drop for SshTunnel {
    fn drop(&mut self) {
        if let Some(ref mut process) = self.process {
            // Try to kill the process synchronously
            let _ = process.start_kill();
        }
    }
}

/// Stub implementation when network feature is disabled
#[cfg(not(feature = "network"))]
pub struct SshTunnel;

#[cfg(not(feature = "network"))]
impl SshTunnel {
    pub fn new(
        _host: &str,
        _ssh_port: u16,
        _user: &str,
        _identity: Option<&str>,
        _local_port: u16,
        _remote_port: u16,
    ) -> Self {
        Self
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Network feature not enabled".into())
    }

    pub async fn run_remote_command(
        &self,
        _command: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Network feature not enabled".into())
    }

    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}
