# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>面向 Windows 的 WSL / Hyper-V 网络代理、Hosts 管理与 AI 集成桌面工具</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

[English](README.md) | 简体中文 | [繁體中文](README-HK.md) | [日本語](README-JA.md)

---

## 获取方式

### Microsoft Store（支持一下作者）

可从 Microsoft Store 安装，享受自动更新与 Windows 原生集成体验：

**[→ 前往 Microsoft Store 下载](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Releases

也可从 GitHub Releases 下载独立安装包：

**[→ 前往 GitHub Releases](https://github.com/yangholmes/wsl-bridge/releases)**

提供格式：

- `MSI` 安装包
- `NSIS` 便携包

---

## 项目定位

WSL Bridge 是一款面向 Windows 10/11 的桌面应用，用于统一管理以下几类本地开发服务暴露与访问场景：

- `WSL`
- `Hyper-V`
- `Static host:port`

项目当前已经不再只是“端口转发工具”，而是扩展为包含反向代理、Hosts 管理、流量监控和 AI 集成能力的完整工作台。

---

## 当前模块

### Proxy

独立的 `Proxy` 模块用于承载现代 HTTP 系流量分发与反向代理能力。

主要能力：

- Listener / Route / Upstream 三层拓扑模型
- HTTP / HTTPS Listener
- TLS 终止
- 用户手动上传证书
- 本地 CA 生成证书并用于 Listener
- 反向代理到 `WSL`、`Hyper-V`、`Static` 上游
- URL 级上游目标
- Path Prefix 改写
- WebSocket 支持
- gRPC / gRPCS 第一版透传支持
- `server_name` 分流与通配符匹配
- 按优先级匹配单条 Route
- 基于 PixiJS 的 Proxy 拓扑画布：支持搜索、缩放、平移、上下文菜单、右侧详情面板

### Hosts

独立的 `Hosts` 模块用于管理结构化的本地域名覆盖配置。

主要能力：

- 结构化 Hosts 分组，持久化到 SQLite
- 任意时刻仅一套“当前生效分组”写入系统 `hosts`
- 表格化记录编辑，支持 IPv4 / IPv6
- 分组复制、重命名、删除、导入、导出
- 首次使用时自动从系统 `hosts` 文件导入到 `default`
- 导入导出使用系统原生文件选择器
- 实际写入系统 `hosts` 需要管理员权限，但页面不会隐藏，而是提供明确提示

### Rules

`Rules` 现已进入 legacy 模式。

当前定位：

- 继续保留历史规则的查看、修改、启停、删除
- 新建规则仅允许：
  - `udp_fwd`
  - `socks5_proxy`
- 旧的 `tcp_fwd` 与 `http_proxy` 可迁移到 `Proxy`

### Dashboard / 流量监控

首页现在已同时聚合 `Rules` 与 `Proxy` 两类运行与流量数据。

主要变化：

- `规则流量监控` 已升级为统一的 `流量监控`
- 可混合展示 Legacy Rules 与 Proxy Upstream 曲线
- 应用状态、规则状态、风险提示都已纳入 Proxy 统计

### AI 集成

独立的 `AI 集成` 模块集中管理 MCP、Skill 安装、能力暴露和诊断。

主要能力：

- 内置 MCP 服务
- 面向 AI 的只读状态资源：Proxy / Hosts / Rules / Traffic / Logs
- 基于结构化 `ConfigPatch` 的：
  - `dry-run`
  - 事务式 `apply`
  - 配置校验
  - 连通性测试
- Agent Skill 预览、安装、卸载
- Skill 安装前自动检查并补齐全局 MCP 客户端配置
- AI 相关审计日志

当前已对齐的 Agent 目标包括：

- Claude Code
- Codex
- Cursor
- Copilot
- OpenCode
- 通用 `.agents`

---

## 核心能力总览

当前 WSL Bridge 已覆盖：

- TCP / UDP 转发
- SOCKS5 代理
- 面向 HTTP 系开发服务的反向代理
- WSL / Hyper-V 动态目标发现与自动重绑
- 多网卡监听 / 绑定模式
- 防火墙配置集成
- 结构化 Hosts 管理
- WSL / Hyper-V / NIC 拓扑扫描
- 审计日志与运行日志
- AI 辅助读取、规划、预检与受控写入

---

## MCP / AI API 模型

当前 AI 接口不再沿用“每个按钮一个 MCP Tool”的膨胀式设计，而是采用以下模型：

- 少量 MCP Tools
- 丰富 MCP Resources
- 结构化 ConfigPatch
- validate / dry-run / test 闭环
- `wsl-bridge-operator` Skill

代表性 MCP Resources：

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

代表性 MCP Tools：

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

## 技术栈

### 前端

- [Solid.js](https://www.solidjs.com/)
- [TanStack Router](https://tanstack.com/router)
- [TanStack Query](https://tanstack.com/query)
- [TanStack Table](https://tanstack.com/table)
- [Kobalte](https://kobalte.dev/)
- [PixiJS](https://pixijs.com/)：用于 Proxy 拓扑画布

### 后端

- [Tauri 2](https://v2.tauri.app/)
- [Rust](https://www.rust-lang.org/)
- [Tokio](https://tokio.rs/)
- [SQLite](https://sqlite.org/)

### 工具链

- `Vite`
- `pnpm`
- `Cargo`

---

## 快速开始

### 系统要求

- Windows 10 `22H2+` 或 Windows 11
- 如需使用 WSL 目标，请先安装 WSL
- 如需使用 Hyper-V 目标，请先启用 Hyper-V
- 如需完整 Hosts / 防火墙 / 运行时能力，建议以管理员身份运行

### 首次使用

1. 从 Microsoft Store 或 GitHub Releases 安装应用。
2. 打开 `Topology`，扫描当前网络环境。
3. 如需 HTTP / HTTPS 反向代理，请进入 `Proxy`。
4. 如需 Legacy UDP 转发或 SOCKS5，请进入 `Rules`。
5. 如需本地域名覆盖，请进入 `Hosts`。
6. 如需 AI 辅助配置，请进入 `AI 集成` 查看本地 MCP 服务状态。

### 典型使用场景

#### 将 WSL Web 服务暴露到局域网

1. 打开 `Proxy`。
2. 创建一个监听 `0.0.0.0:<port>` 的 `Listener`。
3. 创建 `Route`，按需填写 `server_name` 与路径前缀。
4. 创建指向 WSL 发行版与目标端口的 `Upstream`。
5. 使用内置连通性测试，或在其他局域网设备访问该地址。

#### 管理多套本地 Hosts 预设

1. 打开 `Hosts`。
2. 创建或导入多个分组。
3. 在记录编辑弹窗中维护每组 Hosts 记录。
4. 打开目标分组的开关，将其整体写入系统 `hosts` 文件。

#### 安装 AI 集成

1. 打开 `AI 集成`。
2. 确认本地 MCP 服务状态正常。
3. 选择目标 Agent。
4. 查看 Skill 安装预览。
5. 如需要，先安装全局 MCP 客户端配置，再安装全局或项目级 Skill。

---

## 开发

```powershell
# 安装依赖
pnpm install

# 启动前端 + Tauri 开发环境
pnpm tauri dev

# 类型检查
pnpm typecheck

# 构建前端
pnpm build

# 构建桌面应用
pnpm tauri build
```

开发前建议先阅读：

- [docs/wsl-bridge-design.md](docs/wsl-bridge-design.md)
- [docs/wsl-bridge-uiux-design.md](docs/wsl-bridge-uiux-design.md)
- [docs/开发日志.md](docs/开发日志.md)

---

## 贡献

欢迎提交 Issue 和 Pull Request。

提交问题时，建议同时说明：

- Windows 版本
- WSL / Hyper-V 环境信息
- 复现步骤
- 问题发生在 `Rules`、`Proxy`、`Hosts` 还是 `AI 集成`

---

## 许可证

[MIT License](LICENSE)
