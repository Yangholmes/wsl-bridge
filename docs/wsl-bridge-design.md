# WSL Bridge 方案设计文档（Windows 10/11）

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 文档版本：`v1.0`
- 更新时间：`2026-03-01`
- 目标读者：产品、前端、后端、测试、运维

## 2. 背景与目标

当 WSL 运行在 NAT 网络模式时，外部网络无法直接访问 WSL 内服务。项目目标是在 Windows 侧提供统一的桥接与代理能力，覆盖 WSL 与 Hyper-V 场景，并可由桌面应用可视化管理。

本项目需要支持：

- L4 转发（TCP/UDP）
- L7 代理（HTTP/SOCKS）
- 多网卡绑定策略
- 防火墙按 Domain/Private/Public 分开配置
- UI 普通权限 + 后台服务提权

## 3. 约束与结论

### 3.1 已确认需求（来自需求澄清）

1. 必须支持 HTTP/SOCKS 协议。
2. 必须支持 UDP。
3. 目标系统是 Windows 10/11。
4. 规则仅在应用启动后执行；应用未启动时无需恢复/自愈。
5. 支持绑定单网卡或所有网卡。
6. 仅需管理 WSL 与 Hyper-V。
7. 防火墙需按 Domain/Private/Public 独立配置。
8. 权限模型为 UI 普通权限 + 后台服务提权。

### 3.2 对 `netsh interface portproxy` 的结论

`netsh interface portproxy` 不能作为主方案。原因：

- 不支持 UDP。
- 不支持 HTTP/SOCKS 代理能力。
- 对动态目标（如 WSL IP 变化）和运行期可观测性支持弱。

可作为兼容性后备（仅 TCP 端口映射）但不作为核心执行器。

## 4. 总体架构

### 4.1 进程与职责

1. `wsl-bridge-ui`（Tauri 2 + Solid.js + TanStack）

- 普通用户权限运行。
- 提供规则管理、网络拓扑展示、状态监控和日志查看。
- 不直接操作系统敏感配置。

2. `wsl-bridge-service`（Windows Service，Rust）

- 提权运行（建议 LocalSystem 或受限服务账号）。
- 执行所有敏感操作：监听端口、代理转发、防火墙修改、网络探测。
- 管理规则生命周期（应用会话驱动）。

3. 本地存储（SQLite）

- 路径建议：`C:\ProgramData\wsl-bridge\state.db`
- 保存配置态规则、运行状态、审计日志、拓扑快照。

### 4.2 IPC 通信

- UI 与服务通过 Named Pipe 通信（仅本机）。
- Named Pipe 设置 ACL，仅允许当前登录用户 SID + Administrators 访问。
- 所有请求均附带会话标识 `session_id` 和请求 ID，便于追踪。

### 4.3 生命周期策略（关键）

- 服务可常驻，但默认不应用规则。
- UI 启动后发起 `ActivateSession`，服务才 `ApplyRules`。
- UI 心跳超时或显式退出时，服务执行 `StopRules`，撤销运行态监听与可选防火墙规则。
- 满足“应用没启动就不执行规则”的业务约束。

## 5. 技术栈与组件

### 5.1 桌面端

- Tauri：`>= 2.0`
- 前端：Solid.js + TanStack Router + TanStack Query + TanStack Table
- 前端职责：展示/配置/触发，不承担系统权限逻辑

### 5.2 服务端（Rust）

- 异步运行时：`tokio`
- 服务管理：`windows-service`
- 网络与套接字：`tokio::net` + `socket2`
- HTTP 代理：`hyper`（或等效 Rust HTTP 栈）
- SOCKS5：优先成熟 crate；不足时最小自实现（CONNECT + UDP ASSOCIATE）
- 持久化：`sqlx + sqlite`
- 日志与追踪：`tracing + tracing-subscriber`
- Windows API：`windows` crate（网卡、服务、事件、Firewall COM/WMI）

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
- SingleNic：绑定该网卡当前 IP，网卡地址变化时自动重绑。
- AllNics：监听 `0.0.0.0`（IPv4）和 `::`（IPv6，可配置开关）。

### 6.4 防火墙策略

- 每条规则拥有独立 firewall policy。
- Profile 分离：Domain/Private/Public 三个开关独立。
- 规则命名建议：`WSLBridge-{RuleId}-{Profile}-{Proto}-{Port}`
- 创建、更新、删除与规则状态一致，避免遗留放行。

## 7. 规则模型与数据结构

## 7.1 `proxy_rule`

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

## 7.2 `firewall_policy`

- `rule_id` TEXT PK/FK
- `allow_domain` INTEGER
- `allow_private` INTEGER
- `allow_public` INTEGER
- `direction` TEXT（默认 `inbound`）
- `action` TEXT（默认 `allow`）

## 7.3 `runtime_state`

- `rule_id` TEXT PK/FK
- `state` TEXT (`running|stopped|error`)
- `pid` INTEGER NULL
- `last_error` TEXT NULL
- `last_apply_at` INTEGER

## 7.4 `session_state`

- `session_id` TEXT PK
- `client_user_sid` TEXT
- `last_heartbeat_at` INTEGER
- `active` INTEGER

## 7.5 `audit_log`

- `id` INTEGER PK AUTOINCREMENT
- `time` INTEGER
- `level` TEXT
- `module` TEXT
- `event` TEXT
- `detail` TEXT

## 8. IPC 接口（UI <-> Service）

### 8.1 会话类

1. `ActivateSession { client_version } -> { session_id, heartbeat_interval_ms }`
2. `Heartbeat { session_id } -> { ok }`
3. `DeactivateSession { session_id } -> { ok }`

### 8.2 拓扑类

1. `ScanTopology -> { adapters, wsl, hyperv, timestamp }`
2. `GetWSLDistros -> { distros[] }`
3. `GetHyperVVMs -> { vms[] }`

### 8.3 规则类

1. `ListRules -> { rules[] }`
2. `CreateRule { rule, firewall } -> { id }`
3. `UpdateRule { id, patch } -> { ok }`
4. `DeleteRule { id } -> { ok }`
5. `EnableRule { id, enabled } -> { ok }`

### 8.4 运行与状态

1. `ApplyRules { session_id } -> { applied, failed[] }`
2. `StopRules { session_id } -> { stopped }`
3. `GetRuntimeStatus -> { items[] }`
4. `TailLogs { cursor } -> { events[], next_cursor }`

## 9. 执行流程

### 9.1 启动流程

1. UI 启动并连接服务。
2. 发起 `ActivateSession`，进入心跳。
3. UI 拉取拓扑与规则，展示当前状态。
4. 用户点击“应用规则”后服务执行 `ApplyRules`。

### 9.2 应用规则流程

1. 校验端口冲突与配置合法性。
2. 解析目标地址（WSL/Hyper-V -> 实时 IP）。
3. 按 `type` 创建对应 listener/代理实例。
4. 下发 Profile 防火墙规则。
5. 写入 `runtime_state` 与审计日志。

### 9.3 退出与清理流程

1. UI 发送 `DeactivateSession`（或心跳超时）。
2. 服务停止运行态代理/转发 listener。
3. 根据策略清理运行态相关防火墙规则。
4. 保留配置态规则（下次 UI 启动可再次应用）。

## 10. 安全与权限

### 10.1 权限边界

- UI 无管理员权限。
- 所有系统改动必须走服务 IPC。
- 服务端对每个 IPC 请求做来源 SID 校验和参数校验。

### 10.2 配置安全

- 不在前端保存敏感配置。
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

### M1

- 服务骨架 + Named Pipe IPC
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

- 安装器与服务注册
- 升级/卸载清理
- 压测与稳定性回归
- Win10/11 兼容验收

## 13. 风险与应对

1. UDP 会话复杂度高
   应对：先完成单目标映射与超时回收，再逐步加入高级策略。
2. WSL/Hyper-V IP 变化导致中断
   应对：应用规则时实时解析；运行期按事件/轮询刷新。
3. 防火墙遗留规则
   应对：统一命名 + 引导式回收 + 启动时一致性检查。
4. UI/Service 版本不一致
   应对：IPC 协议版本字段与兼容检查。

## 14. 非目标（当前阶段不做）

- 不支持 VMware/VirtualBox。
- 不做内核级 WFP/驱动方案。
- 不做开机自动恢复规则（与需求约束一致）。

## 15. 验收标准（MVP）

1. 在 Windows 10/11 上可安装并启动 UI 与服务。
2. 可创建并应用 TCP/UDP 规则，外部能访问 WSL/Hyper-V 服务。
3. 可创建 HTTP/SOCKS5 代理并可用。
4. 可按单网卡/所有网卡切换绑定策略。
5. 可按 Domain/Private/Public 分别配置放行。
6. 关闭 UI 后规则停止生效。
7. 日志可定位规则应用失败原因。

## 16. 实施建议（代码仓库）

- 建议 workspace 结构：
  - `apps/ui`（Tauri + Solid）
  - `apps/service`（Windows Service）
  - `crates/shared`（DTO、错误码、协议版本）
  - `crates/core`（探测、编排、执行器）
  - `docs/`（设计与运维文档）

---
