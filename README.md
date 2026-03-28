# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>Easily expose WSL and Hyper-V services to external networks</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

English | [简体中文](README-CN.md)

---

## Download

### Microsoft Store (Support the Author)

Get it from the Microsoft Store for automatic updates and native Windows integration:

**[→ Download from Microsoft Store](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Release

Download standalone installers (administrator version with full features) from GitHub Releases:

**[→ Go to GitHub Releases](https://github.com/yangholmes/wsl-bridge/releases)**

Available in both MSI installer and NSIS portable formats.

---

## Features

WSL Bridge is a desktop network bridging tool for Windows 10/11, designed to solve network access challenges in WSL NAT mode.

### Core Capabilities

- **Port Forwarding**: TCP and UDP port forwarding to expose WSL/Hyper-V services to external networks
- **Proxy Services**: Built-in HTTP proxy and SOCKS5 proxy (supports CONNECT tunneling and UDP ASSOCIATE)
- **Dynamic Target Resolution**: Auto-detect WSL distro and Hyper-V VM IP changes with runtime auto-rebinding
- **Multi-NIC Binding**: Support for single NIC binding (auto-rebind on IP change) or all NICs listening
- **Firewall Integration**: Fine-grained firewall rule configuration by Domain/Private/Public Profile
- **Visual Rule Management**: Intuitive rule CRUD, batch operations, and status monitoring

### Network Topology Discovery

- **WSL Discovery**: Auto-identify distros, networkingMode, and real-time IP
- **Hyper-V Discovery**: Enumerate VMs, vSwitches, vNICs, and IP mappings
- **NIC Discovery**: Physical/virtual NICs, address families, status, and routing priority

### MCP Server (Optional)

Built-in [Model Context Protocol](https://modelcontextprotocol.io/) server for AI assistant remote management:

- Read virtualization topology information
- Create, update, and delete forwarding rules
- Enable/disable rules
- Support for Claude Desktop, Cursor, Windsurf, and other client integrations

### Audit & Logging

- Complete audit logs for rule changes
- Real-time log tail (supports pause/resume)
- Filter by level, module, rule ID, and time range
- CSV export support

---

## Tech Stack

### Frontend

- **[Solid.js](https://www.solidjs.com/)** - Reactive UI framework
- **[TanStack Router](https://tanstack.com/router)** - Type-safe routing
- **[TanStack Query](https://tanstack.com/query)** - Server state management
- **[TanStack Table](https://tanstack.com/table)** - High-performance tables
- **[Kobalte](https://kobalte.dev/)** - Accessibility component library

### Backend

- **[Tauri 2](https://v2.tauri.app/)** - Cross-platform desktop app framework
- **[Rust](https://www.rust-lang.org/)** - Systems programming language
- **[Tokio](https://tokio.rs/)** - Async runtime
- **[SQLite](https://sqlite.org/)** - Local persistence storage

### Build Tools

- **Vite** - Frontend build tool
- **pnpm** - Package manager
- **Cargo** - Rust build system

---

## Quick Start

### System Requirements

- Windows 10 (22H2+) or Windows 11
- WSL installed (optional, for WSL features)
- Hyper-V enabled (optional, for Hyper-V features)

### First-Time Setup

1. Install the app from Microsoft Store or GitHub Releases
2. Launch WSL Bridge
3. Go to the "Topology" page and scan your current network environment
4. Navigate to the "Rules" page and click "New Rule"
5. Configure the listen port and target address (WSL/Hyper-V/Static IP)
6. Click "Apply Rules" to start forwarding

---

## Contributing

All forms of contributions are welcome!

### Submitting Issues

- Use [GitHub Issues](https://github.com/yangholmes/wsl-bridge/issues) to report bugs or suggest features
- Please provide detailed reproduction steps and system environment info
- For feature suggestions, describe the use case and expected behavior

### Submitting Pull Requests

1. Fork this repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Commit your changes: `git commit -m 'Add amazing feature'`
4. Push to the branch: `git push origin feature/amazing-feature`
5. Create a Pull Request

### Development Environment

```powershell
# Install dependencies
pnpm install

# Development mode (hot reload)
pnpm tauri dev

# Type checking
pnpm typecheck

# Build
pnpm tauri build
```

---

## License

[MIT License](LICENSE)

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/yangholmes">yangholmes</a>
</p>
