# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>Windows desktop toolkit for WSL / Hyper-V networking, reverse proxying, Hosts management, and AI-assisted operations</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

English | [简体中文](README-CN.md) | [繁體中文](README-HK.md) | [日本語](README-JA.md)

---

## Download

### Microsoft Store (Support the Author)

Install from Microsoft Store for automatic updates and native Windows integration:

**[→ Download from Microsoft Store](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Releases

Standalone installers are available from GitHub Releases:

**[→ Go to GitHub Releases](https://github.com/yangholmes/wsl-bridge/releases)**

Available formats:

- `MSI` installer
- `NSIS` portable package

---

## What It Does

WSL Bridge is a Windows 10/11 desktop application for exposing and operating local development services across:

- `WSL`
- `Hyper-V`
- `Static host:port` targets

It now combines forwarding, reverse proxying, structured Hosts management, traffic monitoring, and an AI integration workspace in one desktop application.

---

## Current Modules

### Proxy

The dedicated `Proxy` module handles modern HTTP-family traffic routing and reverse proxy workflows.

Key capabilities:

- Listener / Route / Upstream topology model
- HTTP and HTTPS listeners
- TLS termination with user-uploaded certificates or locally generated CA certificates
- Reverse proxy to `WSL`, `Hyper-V`, or `Static` upstreams
- URL-level upstream targeting
- Path-prefix rewrite
- WebSocket support
- gRPC / gRPCS first-version passthrough support
- Server-name-based routing with wildcard matching
- Priority-based route selection
- Topology canvas UI with search, pan, zoom, context menu, and side detail panel

### Hosts

The dedicated `Hosts` module manages structured local domain overrides.

Key capabilities:

- Structured Hosts groups stored in SQLite
- One active group written to the system `hosts` file at a time
- Table-based record editor with IPv4 / IPv6 support
- Group copy, import, export, rename, and delete
- First-run bootstrap from the current system `hosts` file into `default`
- Native file picker for import / export
- Admin-permission gating for actual system write, while keeping the tab visible with guidance

### Rules

`Rules` is now a legacy module.

Current positioning:

- Existing rules remain manageable
- New rule creation is limited to:
  - `udp_fwd`
  - `socks5_proxy`
- Legacy `tcp_fwd` and `http_proxy` rules can be migrated into `Proxy`

### Dashboard / Monitoring

The dashboard now aggregates runtime and traffic data across both legacy Rules and Proxy.

Highlights:

- Unified traffic monitoring
- Mixed charting for `Rules + Proxy Upstream`
- Runtime status, risk hints, and config summaries aware of both modules

### AI Integration

The dedicated `AI` workspace centralizes MCP, Skill installation, capability exposure, and diagnostics.

Key capabilities:

- Built-in MCP server
- AI-readable state resources for Proxy / Hosts / Rules / Traffic / Logs
- Structured `ConfigPatch` workflow with:
  - `dry-run`
  - transactional `apply`
  - validation
  - connectivity testing
- Agent Skill installation and preview
- Global MCP client installation checks before Skill installation
- Audit trail for AI-related operations

Supported Agent integration targets currently include:

- Claude Code
- Codex
- Cursor
- Copilot
- OpenCode
- Generic `.agents`

---

## Core Capabilities

The product currently covers:

- TCP / UDP forwarding
- SOCKS5 proxy
- Reverse proxy for HTTP-family development services
- Dynamic WSL / Hyper-V target discovery and rebinding
- Multi-NIC listening / bind-mode support
- Firewall profile integration
- Structured Hosts orchestration
- Topology discovery for WSL, Hyper-V, and NICs
- Audit logs and runtime logs
- AI-assisted inspection, planning, validation, and controlled apply

---

## MCP / AI API Overview

WSL Bridge does not expose only one-tool-per-button CRUD MCP APIs anymore. The current AI model is built around:

- Small set of MCP tools
- Rich MCP resources
- Structured config patching
- Validation / dry-run / connectivity-test loop
- `wsl-bridge-operator` Agent Skill

Representative MCP resources:

- `wsl-bridge://ai-guide`
- `wsl-bridge://capabilities`
- `wsl-bridge://state/summary`
- `wsl-bridge://state/proxy`
- `wsl-bridge://state/hosts`
- `wsl-bridge://state/rules`
- `wsl-bridge://state/traffic`
- `wsl-bridge://logs/recent`
- `wsl-bridge://schemas/config-patch`
- `wsl-bridge://schemas/state`

Representative MCP tools:

- `inspect_app`
- `validate_config`
- `apply_config_patch`
- `test_connectivity`
- `export_config`
- `import_config`
- `list_agent_targets`
- `install_agent_skill`
- `uninstall_agent_skill`

---

## Tech Stack

### Frontend

- [Solid.js](https://www.solidjs.com/)
- [TanStack Router](https://tanstack.com/router)
- [TanStack Query](https://tanstack.com/query)
- [TanStack Table](https://tanstack.com/table)
- [Kobalte](https://kobalte.dev/)
- [PixiJS](https://pixijs.com/) for the Proxy topology canvas

### Backend

- [Tauri 2](https://v2.tauri.app/)
- [Rust](https://www.rust-lang.org/)
- [Tokio](https://tokio.rs/)
- [SQLite](https://sqlite.org/)

### Tooling

- `Vite`
- `pnpm`
- `Cargo`

---

## Quick Start

### Requirements

- Windows 10 `22H2+` or Windows 11
- WSL installed if you want WSL targets
- Hyper-V enabled if you want Hyper-V targets
- Administrator privileges recommended for:
  - system Hosts activation
  - full runtime / firewall behavior

### First Run

1. Install the app from Microsoft Store or GitHub Releases.
2. Open `Topology` and scan your current network environment.
3. If you need HTTP / HTTPS reverse proxying, go to `Proxy`.
4. If you need legacy UDP forwarding or SOCKS5, go to `Rules`.
5. If you need local domain overrides, go to `Hosts`.
6. If you want AI-assisted operation, open `AI` and enable / inspect the local MCP service.

### Typical Workflows

#### Expose a web app from WSL to LAN

1. Open `Proxy`.
2. Create a `Listener` on `0.0.0.0:<port>`.
3. Create a `Route` with optional `server_name` and path-prefix matching.
4. Create an `Upstream` that targets your WSL distro and target port.
5. Use the built-in connectivity test or open the endpoint from another device.

#### Manage multiple local Hosts presets

1. Open `Hosts`.
2. Create or import one or more groups.
3. Edit records in the records modal.
4. Turn on the target group to write it into the system `hosts` file.

#### Install AI integration

1. Open `AI`.
2. Confirm local MCP service status.
3. Choose a target Agent.
4. Preview Skill installation.
5. Install global MCP config if required, then install the Skill globally or into a project.

---

## Development

```powershell
# Install dependencies
pnpm install

# Run frontend + Tauri app in dev mode
pnpm tauri dev

# Type check
pnpm typecheck

# Frontend build
pnpm build

# Desktop build
pnpm tauri build
```

Useful references before making changes:

- [docs/wsl-bridge-design.md](docs/wsl-bridge-design.md)
- [docs/wsl-bridge-uiux-design.md](docs/wsl-bridge-uiux-design.md)
- [docs/开发日志.md](docs/开发日志.md)

---

## Contributing

Issues and pull requests are welcome.

When reporting issues, include:

- Windows version
- WSL / Hyper-V environment details
- Reproduction steps
- Whether the problem is in `Rules`, `Proxy`, `Hosts`, or `AI`

---

## License

[MIT License](LICENSE)
