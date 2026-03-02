# WSL Bridge 方案设计文档（Windows 10/11）

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 文档版本：`v2.0`
- 更新时间：`2026-03-02`
- 目标读者：产品、研发、测试

## 2. 背景与目标

当 WSL 运行在 NAT 网络模式时，外部网络无法直接访问 WSL 内服务。项目目标是在 Windows 侧提供统一的桥接与代理能力，覆盖 WSL 与 Hyper-V 场景，并通过桌面应用可视化管理。

本次设计调整为：**项目交付为一个独立 Tauri 桌面应用（通常为 `.exe`），用户每次只需启动该可执行程序，不再依赖前后端分离或独立 Windows Service。**

## 3. 约束与结论

### 3.1 已确认需求

1. 必须支持 HTTP/SOCKS 协议。
2. 必须支持 UDP。
3. 目标系统是 Windows 10/11。
4. 规则仅在应用启动后执行；应用未启动时无需恢复或自愈。
5. 支持绑定单网卡或所有网卡。
6. 仅需管理 WSL 与 Hyper-V。
7. 防火墙需按 Domain/Private/Public 独立配置。
8. 权限模型为整个应用统一管理员权限运行。

### 3.2 对 `netsh interface portproxy` 的结论

`netsh interface portproxy` 不能作为主方案。原因：

- 不支持 UDP。
- 不支持 HTTP/SOCKS 代理能力。
- 对动态目标（如 WSL IP 变化）和运行期可观测性支持弱。

可作为兼容性后备（仅 TCP 端口映射）但不作为核心执行器。

## 4. 总体架构（单应用）

### 4.1 形态

`wsl-bridge.exe`（Tauri 2，单进程桌面应用）

- 一个可执行文件启动应用（UI + 系统能力）。
- UI 层负责展示与交互。
- Rust Core 层负责转发、代理、防火墙、网络探测、规则编排。
- 使用 Tauri Command/Event 在同进程内通信，不使用跨进程 IPC。

### 4.2 模块划分

1. `ui`（Solid.js + TanStack）

- 规则配置、拓扑展示、状态监控、日志查看。
- 不直接操作系统 API。

2. `core-engine`（Rust）

- 管理规则生命周期与运行状态。
- 执行 TCP/UDP 转发与 HTTP/SOCKS 代理。
- 负责 WSL/Hyper-V/网卡探测。
- 负责防火墙规则创建、更新、删除。

3. `store`（SQLite）

- 保存配置态规则、运行状态快照、审计日志。
- 建议路径：`%ProgramData%\wsl-bridge\state.db`（默认）或应用数据目录。

### 4.3 生命周期策略

- 应用启动后，用户可点击“应用规则”触发 `ApplyRules`。
- 应用退出时，统一执行 `StopRules`，停止监听并按策略回收运行态防火墙规则。
- 满足“应用没启动就不执行规则”的业务约束。

## 5. 技术栈与组件

### 5.1 桌面应用

- Tauri：`>= 2.0`
- 前端：Solid.js + TanStack Router + TanStack Query + TanStack Table
- 前后端通信：Tauri `command` + `event`（同进程）

### 5.2 Rust Core

- 异步运行时：`tokio`
- 网络与套接字：`tokio::net` + `socket2`
- HTTP 代理：`hyper`（或等效 Rust HTTP 栈）
- SOCKS5：优先成熟 crate；不足时最小自实现（CONNECT + UDP ASSOCIATE）
- 持久化：`sqlx + sqlite`
- 日志与追踪：`tracing + tracing-subscriber`
- Windows API：`windows` crate（网卡、Firewall COM/WMI、系统信息）

## 6. 核心功能设计

### 6.1 网络嗅探

#### 6.1.1 WSL 嗅探

- 读取用户 `.wslconfig` 的 `networkingMode`（若存在）。
- 运行态检测：
  - `wsl.exe --status`
  - `wsl -d <distro> hostname -I`
- 生成有效状态：`nat | mirrored | unknown`
- 对 NAT 模式给出“建议创建桥接规则”的引导。

#### 6.1.2 Hyper-V 嗅探

- 枚举 vSwitch、VM、VM 网卡、IP 信息。
- 维护 `vm_name -> vnic -> ip` 映射。
- 支持 UI 按 VM 选择目标。

#### 6.1.3 网卡嗅探

- 枚举物理/虚拟网卡、地址族、状态、路由优先级。
- 提供绑定候选列表（用于单网卡绑定）。

### 6.2 代理与转发能力

支持四种规则类型：

1. `tcp_fwd`：TCP 端口转发
2. `udp_fwd`：UDP 端口转发
3. `http_proxy`：HTTP 代理（含 CONNECT）
4. `socks5_proxy`：SOCKS5（CONNECT + UDP ASSOCIATE）

### 6.3 多网卡支持

- `BindMode = SingleNic | AllNics`
- `SingleNic`：绑定该网卡当前 IP，网卡地址变化时自动重绑。
- `AllNics`：监听 `0.0.0.0`（IPv4）和 `::`（IPv6，可配置开关）。

### 6.4 防火墙策略

- 每条规则拥有独立 firewall policy。
- Profile 分离：Domain/Private/Public 三个开关独立。
- 规则命名建议：`WSLBridge-{RuleId}-{Profile}-{Proto}-{Port}`
- 创建、更新、删除与规则状态一致，避免遗留放行。

## 7. 数据模型与状态

### 7.1 `proxy_rule`

- `id` TEXT PK
- `name` TEXT
- `type` TEXT (`tcp_fwd|udp_fwd|http_proxy|socks5_proxy`)
- `listen_host` TEXT
- `listen_port` INTEGER
- `target_kind` TEXT (`wsl|hyperv|static`)
- `target_ref` TEXT（如 distro 名或 vm 名，static 下可为空）
- `target_host` TEXT（解析后或静态 IP）
- `target_port` INTEGER
- `bind_mode` TEXT (`single_nic|all_nics`)
- `nic_id` TEXT NULL
- `enabled` INTEGER
- `created_at` INTEGER
- `updated_at` INTEGER

### 7.2 `firewall_policy`

- `rule_id` TEXT PK/FK
- `allow_domain` INTEGER
- `allow_private` INTEGER
- `allow_public` INTEGER
- `direction` TEXT（默认 `inbound`）
- `action` TEXT（默认 `allow`）

### 7.3 `runtime_state`

- `rule_id` TEXT PK/FK
- `state` TEXT (`running|stopped|error`)
- `last_error` TEXT NULL
- `last_apply_at` INTEGER

### 7.4 `audit_log`

- `id` INTEGER PK AUTOINCREMENT
- `time` INTEGER
- `level` TEXT
- `module` TEXT
- `event` TEXT
- `detail` TEXT

## 8. 应用内接口（UI <-> Rust Core）

通过 Tauri `command` 暴露能力，典型接口如下：

### 8.1 拓扑类

1. `scan_topology() -> { adapters, wsl, hyperv, timestamp }`
2. `get_wsl_distros() -> { distros[] }`
3. `get_hyperv_vms() -> { vms[] }`

### 8.2 规则类

1. `list_rules() -> { rules[] }`
2. `create_rule({ rule, firewall }) -> { id }`
3. `update_rule({ id, patch }) -> { ok }`
4. `delete_rule({ id }) -> { ok }`
5. `enable_rule({ id, enabled }) -> { ok }`

### 8.3 运行与状态

1. `apply_rules() -> { applied, failed[] }`
2. `stop_rules() -> { stopped }`
3. `get_runtime_status() -> { items[] }`
4. `tail_logs({ cursor }) -> { events[], next_cursor }`

## 9. 执行流程

### 9.1 启动流程

1. 用户启动 `wsl-bridge.exe`（管理员权限）。
2. 应用初始化数据库与运行时。
3. 加载规则与拓扑信息并展示。
4. 用户点击“应用规则”后执行 `apply_rules()`。

### 9.2 应用规则流程

1. 校验端口冲突与配置合法性。
2. 解析目标地址（WSL/Hyper-V -> 实时 IP）。
3. 按 `type` 创建对应 listener/代理实例。
4. 下发 Profile 防火墙规则。
5. 写入 `runtime_state` 与审计日志。

### 9.3 退出与清理流程

1. 应用收到退出事件。
2. 执行 `stop_rules()` 停止运行态代理/转发 listener。
3. 根据策略清理运行态相关防火墙规则。
4. 保留配置态规则（下次启动可再次应用）。

## 10. 权限与安全

### 10.1 权限策略

- 整个应用统一管理员权限运行（启动时触发 UAC）。
- 系统改动仅在 Rust Core 中执行，避免 UI 侧越权调用。
- 所有 command 输入做参数校验与边界检查。

### 10.2 配置安全

- 不在前端本地缓存敏感配置。
- 若后续支持 SOCKS/HTTP 认证，凭据需 DPAPI 加密后落盘。

### 10.3 审计

- 每次规则变更、防火墙变更、失败重试都落审计日志。
- 提供导出日志功能用于故障分析。

## 11. 兼容性策略（Windows 10/11）

- 优先使用 Win32/PowerShell 兼容 API。
- 对缺失能力做降级提示，不静默失败。
- 测试矩阵至少覆盖：
  - Windows 10 22H2
  - Windows 11 23H2/24H2
  - WSL NAT 与 mirrored 两类场景

## 12. 里程碑计划

### M1（单应用 MVP）

- Tauri 应用骨架与 command 接口
- 规则 CRUD
- TCP/UDP 转发
- 防火墙按 Profile 配置
- 单网卡/全网卡绑定

### M2

- WSL/Hyper-V 拓扑探测
- WSL/Hyper-V 目标解析
- 运行状态页与错误提示
- 网卡变化重绑

### M3

- HTTP 代理（含 CONNECT）
- SOCKS5（CONNECT + UDP ASSOCIATE）
- 审计日志与日志查看

### M4

- 安装包与签名发布
- 升级/卸载清理
- 压测与稳定性回归
- Win10/11 兼容验收

## 13. 风险与应对

1. UDP 会话复杂度高
   应对：先完成单目标映射与超时回收，再逐步加入高级策略。
2. WSL/Hyper-V IP 变化导致中断
   应对：应用规则时实时解析；运行期按事件或轮询刷新。
3. 防火墙遗留规则
   应对：统一命名 + 引导式回收 + 启动时一致性检查。
4. 单进程承载更多职责导致稳定性压力
   应对：核心执行模块化，建立 panic 恢复与守护清理逻辑。

## 14. 非目标（当前阶段不做）

- 不支持 VMware/VirtualBox。
- 不做内核级 WFP/驱动方案。
- 不做“应用未启动时自动恢复规则”（与需求约束一致）。
- 不引入独立后端服务或前后端分离部署。

## 15. 验收标准（MVP）

1. 在 Windows 10/11 上可直接启动 `wsl-bridge.exe` 完成管理操作。
2. 可创建并应用 TCP/UDP 规则，外部能访问 WSL/Hyper-V 服务。
3. 可创建 HTTP/SOCKS5 代理并可用。
4. 可按单网卡/所有网卡切换绑定策略。
5. 可按 Domain/Private/Public 分别配置放行。
6. 关闭应用后规则停止生效。
7. 日志可定位规则应用失败原因。

## 16. 实施建议（代码仓库）

- 建议 workspace 结构：
  - `src-tauri`（Tauri + Rust Core）
  - `src`（前端页面与状态管理）
  - `src-tauri/crates/core`（探测、编排、执行器）
  - `src-tauri/crates/shared`（DTO、错误码）
  - `docs`（设计与运维文档）

---
