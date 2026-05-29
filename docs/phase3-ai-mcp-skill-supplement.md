# Phase3 AI MCP 与 Skill 方案补充

## 背景

Phase3 引入 Proxy 与 Hosts 后，应用配置对象明显增多。如果继续将每个业务动作都拆成独立 MCP tool，例如 `create_proxy_listener`、`update_hosts_group`、`delete_route`，MCP API 数量会快速膨胀，导致展示、理解、选择和维护都变得困难。

本补充方案采用：

- 少量 MCP tools
- 丰富 MCP resources
- 结构化 config patch
- dry-run / validate / test 闭环
- 配套 `wsl-bridge-operator` skill

目标是让其他 LLM 能够可靠理解、诊断、规划和操作 wsl-bridge，而不是把 GUI 的所有按钮简单映射成 MCP API。

## 设计目标

- MCP tool 数量保持少量稳定，不随 Proxy / Hosts / Rules 对象数量线性增长。
- AI 可以快速了解应用能力、当前状态、权限限制和风险边界。
- 所有写操作都支持 dry-run，执行前可以预览影响、冲突和风险。
- 所有复杂变更走结构化 patch，由应用侧统一校验和执行。
- 应用侧负责 schema 校验、冲突检测、权限检测、事务执行、运行时重载和错误报告。
- Skill 负责沉淀 AI 使用本应用的流程、经验、注意事项和常见诊断路径。

## MCP Resources

Resources 用于让 AI 读取应用能力和状态，不承担写操作。

第一批建议 resources：

```text
wsl-bridge://ai-guide
wsl-bridge://capabilities
wsl-bridge://state/summary
wsl-bridge://state/proxy
wsl-bridge://state/hosts
wsl-bridge://state/rules
wsl-bridge://state/traffic
wsl-bridge://logs/recent
wsl-bridge://schemas/config-patch
wsl-bridge://schemas/state
```

### `wsl-bridge://ai-guide`

面向 LLM 的使用手册，说明：

- wsl-bridge 是什么。
- Proxy / Hosts / Rules / Traffic 的模块边界。
- Rules legacy 模式限制。
- 推荐读取顺序。
- 推荐变更流程。
- 危险操作列表。
- 常见任务示例。
- 常见错误诊断路径。

### `wsl-bridge://capabilities`

返回应用当前支持的能力，例如：

- 支持的模块。
- 支持的 Proxy 协议。
- Hosts 是否需要管理员权限。
- 当前平台能力。
- 当前 feature flags。
- MCP AI API 版本。

### `wsl-bridge://state/summary`

返回整体摘要，作为 AI 的第一读取入口。

示例：

```json
{
  "app": {
    "version": "x.y.z",
    "aiApiVersion": "phase3.ai.v1",
    "isAdmin": true
  },
  "proxy": {
    "listeners": 2,
    "routes": 4,
    "upstreams": 3,
    "enabled": true,
    "problems": 1
  },
  "hosts": {
    "groups": 3,
    "activeGroup": "default",
    "records": 18,
    "requiresAdmin": true
  },
  "rules": {
    "legacyMode": true,
    "total": 5,
    "migratable": 2
  }
}
```

### `wsl-bridge://state/proxy`

返回 Proxy listener / route / upstream / certificate / runtime 状态。

返回内容应该面向 AI 可读，而不是数据库原始行。建议包含：

- summary
- topology
- problems
- warnings
- runtime status
- route matching notes

### `wsl-bridge://state/hosts`

返回 Hosts 分组、当前生效组、记录摘要和权限状态。

建议包含：

- groups
- activeGroup
- records summary
- system hosts write status
- import/export capability
- permission warnings

### `wsl-bridge://state/rules`

返回 legacy Rules 状态。

建议包含：

- legacyMode
- allowedCreateTypes
- blockedCreateTypes
- migratable rules
- migration warnings

### `wsl-bridge://state/traffic`

返回 Rules + Proxy 聚合后的流量监控摘要。

建议包含：

- selected series
- current throughput
- upstream-level proxy traffic
- legacy rules traffic
- time range

### `wsl-bridge://logs/recent`

返回最近错误、警告和关键操作日志。

建议支持按模块筛选：

- proxy
- hosts
- rules
- runtime
- mcp

### `wsl-bridge://schemas/config-patch`

返回结构化 config patch 的 JSON schema。

### `wsl-bridge://schemas/state`

返回 state resources 的结构说明，便于 AI 稳定解析。

## MCP Tools

Tools 保持少量、高层、可组合。

第一批建议 tools：

```text
inspect_app
validate_config
apply_config_patch
test_connectivity
export_config
import_config
```

后续可选 tools：

```text
plan_change
rollback_change
```

### `inspect_app`

用于按需读取应用状态。

```ts
inspect_app({
  modules: ["proxy", "hosts", "rules", "traffic"],
  detail: "summary" | "full" | "diagnostic"
})
```

说明：

- 与 resources 有一定重叠。
- Resources 适合固定路径读取。
- Tool 适合带参数组合查询。

### `validate_config`

用于验证当前配置或待执行 patch。

```ts
validate_config({
  modules: ["proxy", "hosts"],
  patch?: ConfigPatch,
  checks: ["schema", "conflict", "permission", "reachability", "runtime"]
})
```

检查范围：

- patch schema 是否正确。
- listener 端口是否冲突。
- server name / path / route 是否重复或被遮蔽。
- upstream target 是否可解析。
- Hosts 是否需要管理员权限。
- Rules legacy 限制是否违反。
- Proxy runtime 是否需要重载。

### `apply_config_patch`

核心写入口。

```ts
apply_config_patch({
  mode: "dryRun" | "apply",
  patch: ConfigPatch,
  idempotencyKey?: string
})
```

要求：

- `dryRun` 不落库、不写系统 hosts、不触发运行时重载。
- `apply` 必须事务化执行。
- 失败时不产生半成功配置。
- 支持 `clientId` 引用本次新增对象。
- 支持 `idempotencyKey` 避免重复提交。
- 返回 effects / warnings / conflicts / errors。

### `test_connectivity`

用于验证运行时链路。

```ts
test_connectivity({
  target: {
    type: "proxy-route" | "upstream" | "host-port" | "url",
    value: {}
  }
})
```

检查范围：

- listener 是否监听。
- route 是否匹配。
- upstream 是否可连接。
- HTTP / HTTPS 是否可响应。
- WS / WSS 是否能完成握手。
- gRPC / grpcs 按当前能力明确返回支持状态。

### `export_config`

用于导出配置。

```ts
export_config({
  modules: ["proxy", "hosts", "rules"],
  format: "json" | "hosts-file"
})
```

### `import_config`

用于导入配置。

```ts
import_config({
  module: "hosts" | "proxy",
  content: string,
  mode: "dryRun" | "apply"
})
```

要求：

- 导入必须支持 dry-run。
- 返回冲突、覆盖、追加、权限需求等影响。

## Config Patch 协议

Config patch 是本方案的核心，用于替代大量细粒度 CRUD tools。

顶层结构：

```ts
type ConfigPatch = {
  version: "phase3.ai-patch.v1";
  reason?: string;
  proxy?: ProxyPatch;
  hosts?: HostsPatch;
  rules?: RulesPatch;
  settings?: SettingsPatch;
};
```

### Proxy Patch 示例

```json
{
  "version": "phase3.ai-patch.v1",
  "reason": "Expose WSL websocket service to LAN",
  "proxy": {
    "listeners": {
      "create": [
        {
          "clientId": "listener-wsl-ws-8081",
          "name": "WSL WS 8081",
          "bindAddress": "0.0.0.0",
          "port": 8081,
          "protocol": "http"
        }
      ]
    },
    "upstreams": {
      "create": [
        {
          "clientId": "upstream-ubuntu-4001",
          "targetType": "wsl",
          "targetRef": "Ubuntu",
          "targetPort": 4001,
          "protocol": "ws"
        }
      ]
    },
    "routes": {
      "create": [
        {
          "listenerRef": "listener-wsl-ws-8081",
          "serverNames": ["*"],
          "pathPrefix": "/api",
          "upstreamRef": "upstream-ubuntu-4001",
          "priority": 100
        }
      ]
    }
  }
}
```

### Hosts Patch 示例

```json
{
  "version": "phase3.ai-patch.v1",
  "hosts": {
    "groups": {
      "create": [
        {
          "clientId": "dev-hosts",
          "name": "dev",
          "description": "Development hosts"
        }
      ],
      "activate": {
        "groupRef": "dev-hosts"
      }
    },
    "records": {
      "create": [
        {
          "groupRef": "dev-hosts",
          "ip": "127.0.0.1",
          "domain": "a.local",
          "enabled": true,
          "comment": "Local dev"
        }
      ]
    }
  }
}
```

### Patch 执行原则

- `dryRun` 不产生副作用。
- `apply` 必须事务化。
- patch 中允许使用 `clientId` 引用本次新增对象。
- 应用侧负责将 `clientId` 映射成真实 ID。
- 所有 destructive 操作必须返回影响范围。
- 涉及系统 hosts 写入、监听 `0.0.0.0`、删除级联对象等，需要返回 warning。
- 重复提交同一 `idempotencyKey` 不应重复创建对象。

## Dry-run 返回格式

Dry-run 需要返回清晰的预览，不应只返回 `ok: true`。

成功示例：

```json
{
  "ok": true,
  "mode": "dryRun",
  "summary": [
    "Create 1 listener",
    "Create 1 upstream",
    "Create 1 route"
  ],
  "warnings": [
    {
      "severity": "warning",
      "code": "LAN_EXPOSURE",
      "message": "0.0.0.0:8081 will be accessible from LAN"
    }
  ],
  "conflicts": [],
  "effects": {
    "creates": [],
    "updates": [],
    "deletes": [],
    "runtimeRestartRequired": true,
    "requiresAdmin": false
  }
}
```

失败示例：

```json
{
  "ok": false,
  "mode": "dryRun",
  "errors": [
    {
      "code": "PORT_CONFLICT",
      "target": "proxy.listeners[0]",
      "message": "Port 8081 is already used by another listener"
    }
  ]
}
```

## Validate / Test 闭环

AI 能否可靠使用应用，关键在验证闭环。

`validate_config` 负责配置层：

- schema 是否正确。
- listener 端口是否冲突。
- server name / path / route 是否重复或被遮蔽。
- upstream target 是否可解析。
- Hosts 是否需要管理员权限。
- Rules legacy 限制是否违反。
- Proxy runtime 是否需要重载。

`test_connectivity` 负责运行层：

- listener 是否真的监听。
- route 是否能匹配。
- upstream 是否可连接。
- HTTP / HTTPS 是否有响应。
- WS / WSS 是否能完成握手。
- gRPC / grpcs 如果仍是债务，需要明确返回 `unsupported` 或 `partial`。

失败结果应该包含失败阶段：

```json
{
  "ok": false,
  "stage": "upstream_connect",
  "message": "Route matched but upstream Ubuntu:4001 is not reachable",
  "suggestions": [
    "Check whether WSL distribution is running",
    "Check whether service listens on the expected port"
  ]
}
```

## wsl-bridge-operator Skill

Skill 不替代 MCP。Skill 用于告诉 AI 如何正确使用 MCP。

建议目录：

```text
wsl-bridge-operator/
  SKILL.md
  references/
    concepts.md
    proxy-recipes.md
    hosts-recipes.md
    rules-legacy.md
    troubleshooting.md
    patch-schema.md
    safety.md
```

### `SKILL.md` 应包含

- 什么时候使用这个 skill。
- 第一件事先读 `wsl-bridge://ai-guide` 和 `wsl-bridge://state/summary`。
- 所有写操作必须先 dry-run。
- 哪些场景需要用户确认。
- Proxy 配置推荐流程。
- Hosts 配置推荐流程。
- Rules legacy 限制。
- 常见诊断路径。
- 不要调用大量底层 CRUD。
- 不要猜测系统状态，优先读取 resources。

### 典型流程

用户要求暴露 WSL 服务：

```text
1. Read wsl-bridge://state/summary and wsl-bridge://state/proxy.
2. Check existing listener port conflict.
3. Build ConfigPatch.
4. Call apply_config_patch with mode=dryRun.
5. Explain warnings to user.
6. Apply only after confirmation if operation is risky.
7. Run test_connectivity.
8. If failed, inspect logs/recent and provide targeted fix.
```

## 安全策略

AI 写配置必须有明确安全边界。

以下场景应返回 `requiresConfirmation: true`：

- 写系统 hosts。
- 激活 hosts 分组。
- 删除 hosts 分组。
- 删除 proxy listener / route / upstream。
- listener 绑定 `0.0.0.0`。
- 开启 HTTPS 并导入证书。
- 修改 legacy rules。
- 迁移 rules 到 proxy。

其他要求：

- 所有 apply 写入审计日志。
- 每次 apply 生成 change id。
- 后续可通过 `rollback_change(changeId)` 回滚。
- MCP handler 不直接操作数据库，应调用与 UI 共用的 core service。

## 与现有 UI 的关系

AI 接口不绑定 UI，但应复用业务层。

建议调用链：

```text
UI Modal / Page
  -> Frontend API
    -> Tauri command
      -> Core service
        -> Store / Runtime

MCP tool
  -> MCP handler
    -> Core service
      -> Store / Runtime
```

这样可以避免：

- UI 和 MCP 行为不一致。
- MCP 生成非法配置。
- 后续维护两套逻辑。

## 分阶段落地计划

### 第一阶段：AI 可读

任务：

- 新增 `wsl-bridge://ai-guide`。
- 新增 `wsl-bridge://capabilities`。
- 新增 `wsl-bridge://state/summary`。
- 新增 `wsl-bridge://state/proxy`。
- 新增 `wsl-bridge://state/hosts`。
- 新增 `inspect_app`。
- 新增 `validate_config` 基础版本。
- 输出 `config-patch` schema 初稿。

验收标准：

- AI 能读取应用能力、当前 Proxy / Hosts 状态。
- AI 能判断当前配置是否存在明显问题。
- 不支持写操作时也可以完成诊断。

### 第二阶段：结构化变更

任务：

- 实现 `apply_config_patch(mode: "dryRun")`。
- 支持 Proxy listener / route / upstream create / update / delete dry-run。
- 支持 Hosts group / record create / update / delete / activate dry-run。
- dry-run 返回 effects / warnings / conflicts。
- 建立 patch schema 单元测试。

验收标准：

- AI 能生成 patch 并获得准确预览。
- 冲突、权限、级联删除能被 dry-run 识别。
- dry-run 不产生副作用。

### 第三阶段：安全 Apply

任务：

- 实现 `apply_config_patch(mode: "apply")`。
- 支持事务执行。
- 支持审计日志。
- 支持 idempotencyKey。
- 支持 runtime reload。
- 支持系统 hosts 写入前权限检查。

验收标准：

- dry-run 通过的 patch 可以 apply。
- apply 失败不产生半成功状态。
- 写操作有审计日志。
- 危险操作有 warning / confirmation 标记。

### 第四阶段：验证闭环

任务：

- 实现 `test_connectivity`。
- Proxy 支持 listener、route、upstream 级测试。
- Hosts 支持当前生效组检测。
- Traffic 支持 AI 读取摘要。
- recent logs 支持按模块筛选。

验收标准：

- AI 能判断失败发生在 listener、route、upstream 还是目标服务。
- HTTP / HTTPS / WS / WSS 有明确测试结果。
- gRPC / grpcs 按当前实现能力明确返回支持状态。

### 第五阶段：Skill

任务：

- 创建 `wsl-bridge-operator` skill。
- 写入 Proxy / Hosts / Rules / troubleshooting recipes。
- 对齐 MCP resources 和 patch schema。
- 增加版本字段，例如 `requiresWslBridgeAiApi >= phase3.ai.v1`。

验收标准：

- AI 能按 skill 流程完成典型任务。
- skill 中的示例 patch 能通过 schema 校验。
- 应用 API 变更时有版本兼容说明。

## MVP 建议

第一轮建议只做 AI 可读和 dry-run。

Resources：

```text
wsl-bridge://ai-guide
wsl-bridge://capabilities
wsl-bridge://state/summary
wsl-bridge://state/proxy
wsl-bridge://state/hosts
wsl-bridge://schemas/config-patch
```

Tools：

```text
inspect_app
validate_config
apply_config_patch
```

其中 `apply_config_patch` 第一版只支持：

```text
mode: dryRun
```

这样可以先让 AI 具备“看懂 + 规划 + 预检”的能力，暂时不开放直接写配置。等 schema 和 dry-run 稳定后，再开放 `apply`。

## 结论

Phase3 后的 AI 能力不应继续走“功能按钮集合式 MCP API”。推荐将 MCP AI 接口定义为一套配置操作协议：

```text
Resources 负责理解状态
Tools 负责少量高层动作
ConfigPatch 负责表达变更
Dry-run / validate / test 负责闭环
Skill 负责沉淀 AI 使用流程
```

这套方案可以控制 MCP API 数量，同时让 AI 能够可靠地理解、预检、执行和验证 wsl-bridge 的配置变更。

## Agent Skill 分发与安装补充

`wsl-bridge-operator` skill 需要能够安装到不同用户使用的 coding agent 中。不同 Agent 对 skill、rules、instructions 的支持形态不完全一致，因此不应只提供单一复制路径，而应提供 Agent adapter 与 fallback 机制。

### Canonical Skill Package

项目内部维护一份 canonical skill package：

```text
skills/wsl-bridge-operator/
  SKILL.md
  manifest.json
  references/
    concepts.md
    proxy-recipes.md
    hosts-recipes.md
    rules-legacy.md
    troubleshooting.md
    patch-schema.md
    safety.md
```

该目录是所有 Agent adapter 的源内容。不同 Agent 的安装内容由 adapter 从 canonical package 渲染或复制生成。

### Generic Fallback：`.agents/`

对于 OpenCode、Claude Code、Cursor、Codex 等 coding 类 Agent，很多场景会支持或约定读取项目内 `.agents/` 目录中的 skill / instructions / tools。即使具体 Agent 有自己的原生目录，`.agents/` 也适合作为项目级中立 fallback。

因此 Generic adapter 的推荐 fallback 路径为：

```text
.agents/skills/wsl-bridge-operator/
  SKILL.md
  manifest.json
  references/
```

如果目标 Agent 未被识别，或识别到了 Agent 但无法确认其原生 skill 安装目录，则安装器应优先提供 `.agents/skills/wsl-bridge-operator/` 作为项目级 fallback，而不是要求用户手动复制到未知目录。

### 安装目标优先级

安装器应按以下顺序选择安装方式：

```text
1. Agent 原生 skill 目录
2. Agent 原生 project rule / instruction 目录
3. 项目级 .agents/skills/wsl-bridge-operator/
4. Generic 手动安装包
```

说明：

- 如果用户明确选择某个 Agent，应优先使用该 Agent 的原生 adapter。
- 如果原生 adapter 不可用，但当前是项目目录，应 fallback 到 `.agents/skills/wsl-bridge-operator/`。
- 如果当前不在项目目录，或 `.agents/` 不可写，则生成 Generic 手动安装包。
- `.agents/` fallback 必须标记为 `generic-project-skill`，避免误认为某个 Agent 的原生安装。

### 安装计划示例

```json
{
  "targetAgent": "generic",
  "scope": "project",
  "installType": "generic-project-skill",
  "dryRun": true,
  "writes": [
    {
      "path": ".agents/skills/wsl-bridge-operator/SKILL.md",
      "action": "create"
    },
    {
      "path": ".agents/skills/wsl-bridge-operator/manifest.json",
      "action": "create"
    },
    {
      "path": ".agents/skills/wsl-bridge-operator/references/proxy-recipes.md",
      "action": "create"
    }
  ],
  "warnings": [
    {
      "code": "GENERIC_SKILL_FALLBACK",
      "message": "The target Agent was not matched to a native adapter. The skill will be installed to the project-level .agents directory."
    }
  ]
}
```

### 文件管理规则

安装器生成的文件必须包含管理标记：

```md
<!-- managed-by: wsl-bridge -->
<!-- skill-id: wsl-bridge-operator -->
<!-- skill-version: 0.1.0 -->
<!-- install-type: generic-project-skill -->
```

卸载和更新时只处理带有这些 marker 且安装记录匹配的文件，避免误删用户手写内容。

### MCP Tool 补充

后续可以增加两个安装相关 tool：

```text
list_agent_targets
install_agent_skill
```

`list_agent_targets` 返回可识别 Agent、原生安装能力以及 `.agents/` fallback 能力。

`install_agent_skill` 默认只执行 dry-run：

```ts
install_agent_skill({
  target: "generic" | "claude-code" | "codex" | "cursor" | "copilot" | "opencode" | "openclaw",
  scope: "project" | "user",
  mode: "dryRun" | "apply",
  fallbackToAgentsDir: true
})
```

写入用户项目或用户目录属于敏感操作，`apply` 必须经过用户确认。

## AI 模块独立入口

随着 MCP resources/tools、Agent skill 安装、安全策略、诊断和审计日志逐步加入，AI 相关能力已经不再适合作为 Settings tab 中的一组设置项。后续应将 AI 能力独立为一级模块，提供专门的 tab 入口。

### 模块定位

新增一级模块：

```text
AI 集成
```

该模块负责管理：

- MCP 服务状态与连接配置。
- AI 能力暴露范围。
- `wsl-bridge-operator` skill 安装、更新、卸载。
- AI 写操作权限与安全策略。
- MCP / Skill 诊断。
- AI 调用审计日志。

Settings tab 不再承载完整 MCP 设置，只保留轻量跳转入口或极少量全局开关。

### 导航建议

建议导航结构：

```text
Dashboard
Proxy
Hosts
Rules
AI 集成
Logs / Runtime
Settings
```

如果侧边栏空间有限，`AI 集成` 仍应作为一级入口，不应折叠到 Settings 内。

### 页面结构

`AI 集成` tab 建议采用工作台式结构：

```text
AI 集成
├─ 顶部状态条
├─ 快速操作
├─ MCP 服务
├─ Agent Skill
├─ 能力与权限
├─ 诊断
└─ 审计日志
```

顶部状态条展示：

```text
MCP：运行中 / 已停止 / 错误
AI API：phase3.ai.v1
写操作：只读 / 仅 dry-run / 受控 apply
Skill：已安装 Agent 数量
最近错误：数量
```

快速操作建议：

```text
复制 MCP 配置
安装 Skill
运行诊断
查看 AI Guide
```

### MCP 服务面板

从原 Settings MCP 设置迁移而来。

展示：

- 服务状态。
- 监听地址 / 端口 / transport。
- 启动 / 停止 / 重启。
- 复制连接配置。
- 查看最近 MCP 错误。

该面板不再默认展示大量工具明细，而是展示 AI API 概览：

```text
AI API：phase3.ai.v1
Resources：N
Tools：N
写操作模式：仅 dry-run
```

完整 resources/tools/schema 列表放入高级展开区。

### Agent Skill 面板

负责安装 `wsl-bridge-operator` 到不同 Agent。

展示目标：

```text
Claude Code
Codex
Cursor
Copilot
OpenCode
OpenClaw
Generic .agents
```

每个目标展示：

- 检测状态。
- 安装范围：当前项目 / 用户全局。
- 安装类型：native skill / project rule / repository instruction / generic `.agents`。
- 当前版本。
- 目标版本。
- 安装 / 更新 / 卸载 / 预览。

安装必须先 dry-run，预览将写入的文件和影响范围。

### 能力与权限面板

负责控制 AI 能力边界。

建议权限模式：

```text
只读模式：只能读取 resources。
规划模式：读取 resources + dry-run。
受控写入：apply 前需要用户确认。
完全信任：允许 apply。
```

默认模式建议为：

```text
规划模式
```

危险操作必须独立提示或要求确认：

- 写系统 hosts。
- 激活 hosts 分组。
- 删除 Proxy 对象。
- 绑定 `0.0.0.0`。
- 安装或覆盖 Agent skill。
- 导入配置并覆盖现有对象。

### 诊断面板

负责检查 AI 集成可用性。

检查项：

- MCP 服务是否可连接。
- Resources 是否可读取。
- Tools schema 是否有效。
- ConfigPatch schema 是否可加载。
- `wsl-bridge-operator` skill 是否与当前 AI API 版本兼容。
- 示例 dry-run 是否可执行。

诊断结果应支持复制为报告。

### 审计日志面板

记录 AI 相关操作：

- MCP 调用记录。
- dry-run 记录。
- apply 记录。
- Agent skill 安装 / 更新 / 卸载。
- 失败原因。
- 危险操作确认记录。

审计日志用于排查问题，也用于让用户理解 AI 对应用做过什么。

### Settings 兼容策略

原 Settings 中的 MCP 设置迁移后，Settings 只保留兼容入口：

```text
AI 集成功能已迁移到独立模块。
[打开 AI 集成]
```

如果用户通过旧入口访问 MCP 设置，应跳转到 `AI 集成` tab。

迁移要求：

- 不删除已有 MCP 配置。
- 不改变现有 MCP 服务默认状态。
- 旧设置入口只做引导，不重复维护第二套 UI。
