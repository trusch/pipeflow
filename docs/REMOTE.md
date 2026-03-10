# Remote Control

Pipeflow supports controlling PipeWire on remote machines. This is useful for headless audio servers, studio machines without monitors, or managing audio routing on a machine from across the room (or across the network).

## Architecture

Remote control uses a gRPC protocol (feature-gated behind the `network` Cargo feature, enabled by default). The flow is:

```
┌─────────────┐    SSH Tunnel    ┌──────────────┐    PipeWire
│  Local GUI  │ ◄──────────────► │   Headless   │ ◄──────────►  Daemon
│  (--remote) │   localhost:port │   (--headless)│
└─────────────┘                  └──────────────┘
```

The local GUI establishes an SSH tunnel to the remote machine, then connects to the headless Pipeflow instance's gRPC server through that tunnel.

## Headless Mode

Run Pipeflow as a gRPC server without a GUI:

```bash
pipeflow --headless
```

This starts the server on `127.0.0.1:50051` by default. To change the bind address:

```bash
pipeflow --headless --bind 127.0.0.1:9090
```

### Token Authentication

Secure the gRPC endpoint with a token:

```bash
pipeflow --headless --token mysecrettoken
```

Or via environment variable:

```bash
export PIPEFLOW_TOKEN=mysecrettoken
pipeflow --headless
```

When a token is set, all gRPC requests must include the matching token. Connections without the correct token are rejected.

### Running as a Service

For persistent headless operation, create a systemd user service:

```ini
# ~/.config/systemd/user/pipeflow.service
[Unit]
Description=Pipeflow headless server
After=pipewire.service

[Service]
ExecStart=/usr/local/bin/pipeflow --headless --token %h/.config/pipeflow/token
Restart=on-failure

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now pipeflow
```

## Remote Mode

Connect a local GUI to a remote headless instance:

```bash
pipeflow --remote user@studio-machine
```

This automatically:
1. Establishes an SSH tunnel to the remote host
2. Forwards the remote gRPC port to localhost
3. Launches the GUI connected through the tunnel

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--remote <USER@HOST>` | — | Remote target (required for remote mode) |
| `--ssh-port <PORT>` | 22 | SSH port on remote host |
| `--remote-port <PORT>` | 50051 | gRPC port on the remote machine |
| `--local-port <PORT>` | 50051 | Local port for the tunnel endpoint |
| `-i, --identity <FILE>` | — | SSH private key file |
| `--token <TOKEN>` | — | gRPC authentication token |

### Examples

Basic remote connection:
```bash
pipeflow --remote user@192.168.1.50
```

Custom SSH port and identity file:
```bash
pipeflow --remote user@studio --ssh-port 2222 -i ~/.ssh/studio_key
```

Non-default gRPC port with token auth:
```bash
pipeflow --remote user@studio --remote-port 9090 --token mysecret
```

If the remote host only has the username (no `@`), Pipeflow uses the current `$USER`:
```bash
pipeflow --remote 192.168.1.50  # connects as $USER@192.168.1.50
```

## Security

### Token Authentication

Token auth protects against unauthorized gRPC access. It is **not encrypted** — the token is sent in plaintext over the gRPC connection. This is acceptable when:

- The connection runs through an SSH tunnel (encrypted)
- The server binds to `127.0.0.1` only (default, no network exposure)

**Do not** bind to `0.0.0.0` with token auth alone. Always use SSH tunnels for network-exposed instances.

### SSH Tunnels

SSH provides encryption and authentication for the network layer. The remote mode uses SSH tunnels by default — the gRPC traffic never touches the network unencrypted.

Recommendations:
- Use key-based SSH authentication (disable password auth)
- Keep the headless server bound to `127.0.0.1` (default)
- Use `--token` as an additional layer of defense

### When to Use What

| Scenario | Token | SSH Tunnel | Bind Address |
|----------|-------|------------|--------------|
| Local-only headless | Optional | No | `127.0.0.1` (default) |
| Remote over LAN | Yes | Yes (`--remote`) | `127.0.0.1` (default) |
| Remote over internet | Yes | Yes (`--remote`) | `127.0.0.1` (default) |
| **Never do this** | Any | No | `0.0.0.0` |

## Example Workflows

### Studio Machine from Laptop

On the studio machine (headless, always running):
```bash
pipeflow --headless --token $(cat ~/.config/pipeflow/token)
```

From your laptop:
```bash
pipeflow --remote you@studio-machine --token $(cat ~/.config/pipeflow/token)
```

You get the full Pipeflow GUI locally, controlling the studio machine's PipeWire graph.

### Raspberry Pi Audio Server

Run Pipeflow headless on a Pi handling audio routing:
```bash
# On the Pi
pipeflow --headless --bind 127.0.0.1:50051 --token pitoken

# From your workstation
pipeflow --remote pi@raspberrypi.local --token pitoken
```

### Quick Debug Session

SSH into a machine and check its audio state with verbose logging:
```bash
pipeflow --remote admin@server --token debug -v
```

## Building Without Network Support

If you don't need remote control, disable the `network` feature to reduce dependencies:

```bash
cargo build --release --no-default-features
```

This removes tonic, prost, and tokio from the build. The `--headless` and `--remote` flags will not be available.
