# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>让 WSL 和 Hyper-V 服务轻松暴露到外部网络</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

[English](README.md) | 简体中文

---

## 获取方式

### Microsoft Store（支持一下作者）

从 Microsoft Store 购买获取，享受自动更新和 Windows 原生集成体验：

**[→ 前往 Microsoft Store 下载](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Release

从 GitHub Releases 下载独立安装包（管理员权限完整功能版）：

**[→ 前往 GitHub Releases](https://github.com/yangholmes/wsl-bridge/releases)**

提供 MSI 安装包和 NSIS 便携版两种格式。

---

## 功能特性

WSL Bridge 是一款面向 Windows 10/11 的桌面网络桥接工具，专为解决 WSL NAT 模式下的网络访问难题而设计。

### 核心能力

- **端口转发**：支持 TCP 和 UDP 端口转发，将 WSL/Hyper-V 服务暴露到外部网络
- **代理服务**：内置 HTTP 代理和 SOCKS5 代理（支持 CONNECT 隧道和 UDP ASSOCIATE）
- **动态目标解析**：自动探测 WSL 发行版和 Hyper-V 虚拟机 IP 变化，运行时自动重绑
- **多网卡绑定**：支持单网卡绑定（IP 变化自动重绑）或全网卡监听
- **防火墙集成**：按 Domain/Private/Public Profile 精细化配置防火墙规则
- **可视化规则管理**：直观的规则 CRUD、批量操作、状态监控

### 网络拓扑探测

- **WSL 探测**：自动识别发行版、networkingMode 和实时 IP
- **Hyper-V 探测**：枚举虚拟机、vSwitch、vNIC 和 IP 映射
- **网卡探测**：物理/虚拟网卡、地址族、状态和路由优先级

### MCP 服务器（可选）

内置 [Model Context Protocol](https://modelcontextprotocol.io/) 服务器，支持 AI 助手远程管理：

- 读取虚拟化拓扑信息
- 创建、更新、删除转发规则
- 启用/禁用规则
- 支持 Claude Desktop、Cursor、Windsurf 等客户端集成

### 审计与日志

- 完整的规则变更审计日志
- 实时日志 Tail（支持暂停/继续）
- 按级别、模块、规则 ID、时间范围过滤
- CSV 导出支持

---

## 技术栈

### 前端

- **[Solid.js](https://www.solidjs.com/)** - 响应式 UI 框架
- **[TanStack Router](https://tanstack.com/router)** - 类型安全的路由
- **[TanStack Query](https://tanstack.com/query)** - 服务端状态管理
- **[TanStack Table](https://tanstack.com/table)** - 高性能表格
- **[Kobalte](https://kobalte.dev/)** - 可访问性组件库

### 后端

- **[Tauri 2](https://v2.tauri.app/)** - 跨平台桌面应用框架
- **[Rust](https://www.rust-lang.org/)** - 系统级编程语言
- **[Tokio](https://tokio.rs/)** - 异步运行时
- **[SQLite](https://sqlite.org/)** - 本地持久化存储

### 构建工具

- **Vite** - 前端构建工具
- **pnpm** - 包管理器
- **Cargo** - Rust 构建系统

---

## 快速开始

### 系统要求

- Windows 10 (22H2+) 或 Windows 11
- WSL 已安装（可选，用于 WSL 功能）
- Hyper-V 已启用（可选，用于 Hyper-V 功能）

### 首次使用

1. 从 Microsoft Store 或 GitHub Releases 安装应用
2. 启动 WSL Bridge
3. 进入"拓扑"页面，扫描当前网络环境
4. 进入"规则"页面，点击"新建规则"
5. 配置监听端口和目标地址（WSL/Hyper-V/静态 IP）
6. 点击"应用规则"启动转发

---

## 贡献指南

欢迎各种形式的贡献！

### 提交 Issue

- 使用 [GitHub Issues](https://github.com/yangholmes/wsl-bridge/issues) 报告 bug 或提出功能建议
- 请提供详细的复现步骤和系统环境信息
- 对于功能建议，请说明使用场景和预期行为

### 提交 Pull Request

1. Fork 本仓库
2. 创建功能分支：`git checkout -b feature/amazing-feature`
3. 提交代码：`git commit -m 'Add amazing feature'`
4. 推送分支：`git push origin feature/amazing-feature`
5. 创建 Pull Request

### 开发环境

```powershell
# 安装依赖
pnpm install

# 开发模式（热重载）
pnpm tauri dev

# 类型检查
pnpm typecheck

# 构建
pnpm tauri build
```

---

## 许可证

[MIT License](LICENSE)

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/yangholmes">yangholmes</a>
</p>
