# wsl-bridge

WSL Bridge 是一个面向 Windows 10/11 的单应用桌面工具，目标是在 WSL NAT 场景下提供可视化的端口转发与代理能力。

项目采用单可执行应用形态（Tauri），不依赖独立后端服务。

## 当前状态

- 架构设计：已完成（见 `docs/wsl-bridge-design.md`）。
- UI/UX 设计：已完成（见 `docs/wsl-bridge-uiux-design.md`）。
- M1：已完成（单应用骨架 + 规则管理 + TCP/UDP 转发 + 防火墙 Profile 基础能力）。
- M2：已完成（WSL/Hyper-V 拓扑探测 + 动态目标解析 + Runtime/Topology 页面 + 网卡变化自动重绑）。

## 仓库结构

```txt
.
├─ docs/
│  ├─ wsl-bridge-design.md
│  ├─ wsl-bridge-uiux-design.md
│  └─ 开发日志.md
├─ src/                   # Solid + Kobalte + TanStack 前端源码
├─ dist/                  # 前端构建产物（pnpm build 后生成）
├─ src-tauri/
│  ├─ app/                # 单应用入口、Tauri 配置
│  └─ crates/
│     ├─ core/            # 规则引擎、转发执行器、防火墙执行器、拓扑探测
│     └─ shared/          # DTO / 数据模型
└─ Cargo.toml             # Rust workspace
```

## M1 已实现能力

### 核心引擎

- 规则 CRUD：
  - `create_rule`
  - `list_rules`
  - `update_rule`
  - `delete_rule`
  - `enable_rule`
- 运行控制：
  - `apply_rules`
  - `stop_rules`
  - `get_runtime_status`
  - `tail_logs`

### 网络能力（M1）

- TCP 转发执行器（真实监听 + 双向转发）
- UDP 转发执行器（真实监听 + datagram 转发）
- 绑定模式：
  - `all_nics`
  - `single_nic`（按网卡 ID 解析本机地址）
- 端口冲突检测（监听地址端口冲突）
- 动态目标解析（M2 第一阶段）：
  - `target_kind = wsl | hyperv` 时，应用规则阶段按 `target_ref` 实时解析目标 IP
  - `scan_topology()` 返回 WSL/Hyper-V 拓扑信息
- 运行期自动重绑（M2）：
  - 后台轮询拓扑变化；当 `single_nic` 绑定地址或动态目标地址变化时，自动重应用规则。

### 防火墙能力（M1）

- 每条规则支持 Domain/Private/Public Profile 配置
- 规则应用/停止时可执行防火墙增删（`netsh advfirewall`）
- 支持防火墙模式：
  - `disabled`
  - `best_effort`
  - `enforced`

### 持久化与状态

- SQLite 持久化：
  - `proxy_rule`
  - `firewall_policy`
  - `runtime_state`
  - `audit_log`
- 启动自动加载历史规则与状态快照
- 规则变更/应用/停止后自动落盘

### 应用与 UI

- 前端已迁移到 Solid + Kobalte + TanStack 正式结构：
  - Kobalte：Dialog / TextField / Select / Switch / Checkbox 等可访问组件
  - TanStack Router：页面路由与布局
  - TanStack Query：规则/运行态/拓扑查询管理
  - TanStack Table：规则表格渲染
- Tauri command 已接通（默认启用）
- Rules 页面能力：
  - 新建规则（含防火墙 Profile 配置）
  - 编辑规则（受后端 patch 能力约束）
  - 筛选（名称/类型/启用状态）
  - 批量操作（批量启用/禁用/删除、当前页全选）
  - 分页（10/20/50 每页、页码导航）
  - 运行态合并展示（state/last_apply_at/last_error）
  - `wsl/hyperv` 目标实时 IP 预览（基于拓扑扫描结果）
  - `wsl/hyperv` 的目标引用自动下拉填充（识别到的 distro/VM 名称）
  - 行内启停、删除、应用/停止
- 新增 M2 页面：
  - Runtime：运行态列表、错误高亮、按规则查看关联日志
  - Topology：WSL / Hyper-V / 网卡三块拓扑信息
- 网卡下拉来源于 `scan_topology()` 适配器列表
- Topology 查询采用懒加载与共享缓存策略，降低 Rules/Topology 切页重复扫描频率

## 存储与环境变量

- 默认数据库路径：`./data/state.db`
- `WSL_BRIDGE_DB_PATH`：覆盖数据库路径
- `WSL_BRIDGE_FIREWALL_MODE`：`disabled | best_effort | enforced`（默认 `best_effort`）
- `WSL_BRIDGE_TOPOLOGY_POLL_SECS`：拓扑轮询间隔秒数（默认 `8`，用于自动重绑检测）

## 本地开发与验证

```powershell
pnpm install
pnpm typecheck
cargo fmt --all
cargo test --workspace
pnpm tauri dev
pnpm tauri build
```

## UI 调试指南

### 1. Tauri 联调模式（前后端联通）

单终端执行：

```powershell
pnpm tauri dev
```

适用场景：

- 校验 Tauri command 与 Rust 引擎真实行为
- 校验 SQLite、防火墙、TCP/UDP 执行路径
- 校验单网卡/全网卡与运行态联动

### 2. 打包最终产物

```powershell
pnpm tauri build
```

该命令会先执行前端构建，再编译并打包 Tauri 桌面应用（`.exe`/安装包）。
默认产物位于 `target/release/bundle/`，例如：

- `target/release/bundle/msi/WSL Bridge_0.1.0_x64_en-US.msi`
- `target/release/bundle/nsis/WSL Bridge_0.1.0_x64-setup.exe`

## M1 测试覆盖

- 核心引擎单测：
  - 规则 CRUD
  - 冲突检测
  - stop 流程
  - SQLite roundtrip
  - TCP 转发端到端
  - UDP 转发端到端

## 后续路线图

1. M3：HTTP 代理、SOCKS5、日志与运行态联动增强。
2. M4：安装打包、签名发布、兼容性与稳定性验收。
