# Phase 3 补充需求方案：流量监控

## 1. 背景

当前首页中的 `规则流量监控` 模块仅统计 Legacy Rules 数据，底层统计模型也仅支持按 `rule_id` 聚合。随着 Phase 3 中 Proxy 模块落地，首页需要将 Rules 与 Proxy 的流量统一纳入同一个监控入口，因此将首页模块更名为 `流量监控`，并扩展为多数据源统一展示。

本补充方案用于指导该需求的后续开发，不替代原有三期方案，而是作为三期收尾阶段的增量设计。

## 2. 目标

### 2.1 功能目标

1. 首页 `规则流量监控` 更名为 `流量监控`
2. `流量监控` 同时聚合：
   - Legacy Rules 流量
   - Proxy 流量
3. Proxy 以 `Upstream` 作为最小统计单位
4. 首页图表允许将 Rules 与 Proxy 混合展示在同一张时间序列图中
5. 图表汇总值保持为“当前选中对象总和”

### 2.2 统计口径目标

1. Rules 继续沿用当前流量统计口径
2. Proxy 流量以 `bytes_in / bytes_out / connections / duration_ms` 为核心指标
3. Proxy `requests` 仅对 `http / https upstream` 统计
4. `ws / wss / grpc / grpcs` 不展示 `requests`

## 3. 范围与非目标

### 3.1 本次范围

1. 首页 `流量监控` 模块改造
2. 后端流量统计模型扩展为支持多实体
3. Proxy 流量统计接入
4. Rules 与 Proxy 混合展示

### 3.2 非目标

1. 不进行 Legacy Rules 全量命名清理
2. 不重做首页其他模块布局
3. 不在本次引入更细粒度的 WebSocket message / gRPC message 解析统计
4. 不在本次改造 Proxy 运行时指标页的布局逻辑

## 4. 现状分析

### 4.1 前端现状

1. 首页 [TrafficChart.tsx](C:\Users\yangholmes.liang\work\wsl-bridge\src\features\dashboard\TrafficChart.tsx) 当前仅支持 `rules` 输入
2. 图表选择项、图例、窗口数据请求都绑定到 `rule_id`
3. 首页 [DashboardPage.tsx](C:\Users\yangholmes.liang\work\wsl-bridge\src\features\dashboard\DashboardPage.tsx) 只拉取 Legacy Rules，不拉取 Proxy Listener / Route / Upstream

### 4.2 后端现状

1. [traffic.rs](C:\Users\yangholmes.liang\work\wsl-bridge\src-tauri\crates\core\src\traffic.rs) 当前仅支持按 `rule_id` 记账
2. SQLite `traffic_stats` 当前主键语义为 `rule_id + time_bucket`
3. Proxy 当前仅有运行时命中/错误指标，不会写入 `traffic_stats`

### 4.3 结论

该需求不是简单改文案，而是将流量统计体系从“Legacy Rules 单一来源”升级为“多实体统一流量统计”。

## 5. 设计结论

### 5.1 展示结论

首页 `流量监控` 支持将 Legacy Rules 与 Proxy Upstream 混合到一张图中展示。

原因：

1. 横轴均为时间
2. 纵轴均为流量 / 连接数
3. 对用户而言，首页关注的是“谁在吃流量”，而不是必须先区分来源模块

### 5.2 Proxy 最小统计单位

Proxy 使用 `Upstream` 作为最小统计单位。

原因：

1. 每次请求最终只命中一个 Upstream，归属清晰
2. Upstream 才是真正承载目标流量的实体
3. 后续若要按 Route / Listener 聚合，可以基于 Upstream 做汇总

### 5.3 Proxy 展示名称

由于当前 Proxy Upstream 数据模型没有独立 `name` 字段，首页图表中使用：

`<Route 标识> / <Upstream 标识>`

作为展示名。

建议展示格式：

1. 若 Route 有 `server_names`：
   - `<首个 server_name 或 default> / <target_ref 或 target_host:port>`
2. 若 Route 无 `server_names` 且是默认路由：
   - `default / <target_ref 或 target_host:port>`

## 6. 统计口径

### 6.1 通用指标

所有实体统一支持：

1. `bytes_in`
2. `bytes_out`
3. `connections`
4. `total_duration_ms`
5. `avg_duration_ms`

### 6.2 Requests 指标口径

#### Legacy Rules

保持现状。

#### Proxy

1. `http / https upstream`
   - `requests` 正常统计
2. `ws / wss / grpc / grpcs`
   - 不在首页 `流量监控` 中展示 `requests`
   - 底层可以保留 `requests = 0` 或不累计，但前端不作为可选指标

原因：

1. WebSocket / gRPC 隧道的核心价值在于长连接与字节流量
2. `requests` 在这些协议中容易退化为“建链次数”或“stream 建立次数”，含义不稳定
3. 本次不引入对消息帧的深度解析，避免复杂度失控

### 6.3 汇总口径

首页汇总值始终表示“当前选中对象总和”，不显示“全部对象总和”。

## 7. 数据模型改造

### 7.1 设计目标

将当前按 `rule_id` 聚合的流量模型扩展为通用实体模型。

### 7.2 新增实体类型

建议在 shared 中新增：

```rust
pub enum TrafficEntityType {
    LegacyRule,
    ProxyUpstream,
}
```

### 7.3 通用实体标识

建议将 `traffic_stats` 与内存窗口数据统一改为以下键：

1. `entity_type`
2. `entity_id`

实体示例：

1. Legacy Rules
   - `entity_type = legacy_rule`
   - `entity_id = <rule_id>`
2. Proxy Upstream
   - `entity_type = proxy_upstream`
   - `entity_id = <upstream_id>`

### 7.4 前端展示 DTO

新增统一展示模型：

```ts
type TrafficMonitorEntity = {
  entityType: "legacy_rule" | "proxy_upstream";
  entityId: string;
  label: string;
  enabled: boolean;
};
```

窗口数据 DTO 建议改为：

```ts
type TrafficWindowData = {
  entity_type: "legacy_rule" | "proxy_upstream";
  entity_id: string;
  samples: TrafficSample[];
};
```

统计查询 DTO 建议改为：

```ts
type QueryTrafficStatsRequest = {
  entity_type: "legacy_rule" | "proxy_upstream";
  entity_id: string;
  start_time?: string;
  end_time?: string;
  interval?: "minute";
};
```

## 8. SQLite 改造方案

### 8.1 现有问题

当前表结构只支持 `rule_id`，无法兼容 Proxy Upstream。

### 8.2 目标结构

建议将 `traffic_stats` 扩展为：

1. `entity_type TEXT NOT NULL`
2. `entity_id TEXT NOT NULL`
3. `time_bucket INTEGER NOT NULL`
4. `bytes_in INTEGER NOT NULL`
5. `bytes_out INTEGER NOT NULL`
6. `connections INTEGER NOT NULL`
7. `requests INTEGER NOT NULL`
8. `total_duration_ms INTEGER NOT NULL`
9. `avg_duration_ms INTEGER NOT NULL`
10. `created_at INTEGER NOT NULL`

唯一索引改为：

`(entity_type, entity_id, time_bucket)`

### 8.3 迁移策略

采用平滑迁移：

1. 启动时检查旧表结构
2. 若仍为 `rule_id` 模式，则执行一次 schema migration：
   - 新建临时表
   - 将旧数据迁移为 `entity_type = legacy_rule`
   - 替换旧表
3. 保持旧历史流量不丢失

## 9. Rust 后端改造方案

### 9.1 traffic 模块改造

将 [traffic.rs](C:\Users\yangholmes.liang\work\wsl-bridge\src-tauri\crates\core\src\traffic.rs) 从 `rule_id` 模型改为 `TrafficEntityKey` 模型。

建议新增：

```rust
struct TrafficEntityKey {
    entity_type: TrafficEntityType,
    entity_id: String,
}
```

内存态 `HashMap<String, RuleTrafficState>` 改为：

```rust
HashMap<TrafficEntityKey, EntityTrafficState>
```

### 9.2 TrafficRecorder 改造

当前 `TrafficRecorder` 绑定单个 `rule_id`，需要改为绑定：

1. `entity_type`
2. `entity_id`

同时保留访问日志打点能力。

### 9.3 Legacy Rules 兼容

Legacy Rules 启动 Forwarder 时，继续创建 recorder，但改为：

1. `entity_type = legacy_rule`
2. `entity_id = rule_id`

这样旧模块无需重写埋点逻辑，只需要替换 recorder 构造方式。

### 9.4 Proxy 流量接入

Proxy 在选中 Upstream 后，需要针对命中的 `upstream_id` 记账。

建议做法：

1. 在 Proxy 请求匹配出目标 Upstream 后，创建对应 recorder 上下文
2. 所有成功路径在完成响应或隧道结束后，向该 Upstream 的 recorder 写入：
   - `bytes_in`
   - `bytes_out`
   - `connections`
   - `requests`
   - `duration_ms`

### 9.5 Proxy requests 统计规则

1. `http / https upstream`
   - `requests += 1`
2. `ws / wss / grpc / grpcs`
   - `requests += 0`
   - 仅累计 `connections / bytes / duration`

### 9.6 接口改造

后端需要将以下接口从 `rule_id` 改为“多实体”：

1. `get_traffic_window_data`
2. `query_traffic_stats`

建议新增新的统一入参，而不是继续只接收 `ruleIds`：

```ts
type TrafficWindowQueryEntity = {
  entityType: "legacy_rule" | "proxy_upstream";
  entityId: string;
};
```

同时保留一个过渡期兼容层，避免一次性改动过大。

## 10. 前端首页改造方案

### 10.1 模块更名

首页模块标题：

`规则流量监控` -> `流量监控`

### 10.2 数据源整合

首页加载两个来源的可选实体：

1. Legacy Rules
2. Proxy Upstreams

统一映射为 `TrafficMonitorEntity[]`。

### 10.3 混合展示

同一张图中允许同时选中：

1. 若干 Legacy Rules
2. 若干 Proxy Upstreams

所有序列按统一颜色池渲染。

### 10.4 选择面板

当前配置面板里的 `规则` 区块扩展为 `监控对象`。

建议分组展示：

1. `Rules`
2. `Proxy`

但最终选中项进入同一张图。

### 10.5 默认选中策略

取消“最多 3 个”的上限。

默认行为建议：

1. 初次进入时，默认选中所有 `enabled` 的 Legacy Rules 和 enabled Proxy Upstreams
2. 若数量过多影响性能，则降级为：
   - 默认选中所有最近 120 秒内有流量的实体
   - 若仍为空，再回退为所有 enabled 实体

说明：

由于用户已明确“取消上限”，前端不再强制截断选择数量。

### 10.6 指标选项

首页 `流量监控` 指标建议保留：

1. `total`
2. `in`
3. `out`
4. `connections`

本次不在首页加入 `requests` 指标切换。

原因：

1. 当前首页语义已经是“流量监控”
2. 混合展示后，为避免不同协议下 `requests` 语义不一致，不建议在首页作为主图指标

如后续确有需要，可在详情页单独提供。

## 11. Proxy 展示名生成策略

由于 Upstream 无独立名称字段，建议前端或后端生成展示名：

### 11.1 Route 部分

优先级：

1. 首个 `server_name`
2. `default`
3. `route_id` 短标识

### 11.2 Upstream 部分

优先级：

1. `target_ref`
2. `target_host:target_port`
3. `upstream_id` 短标识

### 11.3 最终格式

`<routeLabel> / <upstreamLabel>`

## 12. 性能与风险

### 12.1 性能风险

1. 用户取消选择上限后，首页可能一次性绘制大量序列
2. uPlot 对几十条序列仍可接受，但上百条会明显影响可读性与渲染性能

### 12.2 应对策略

1. 默认按 `enabled` 或“最近有流量”自动选中，不做硬上限
2. 当选中数量过大时，在 UI 上给出轻提示，不阻止用户操作
3. 窗口数据查询接口应批量请求，避免每个实体单独请求

### 12.3 统计精度风险

1. WebSocket / gRPC 不统计 requests，属于设计结论，不是缺陷
2. 长连接协议的流量会持续累计到连接结束或周期刷新时，应确保 recorder 在 relay 过程中持续 flush

## 13. 实施步骤

### Step 1：数据模型与存储改造

1. shared 新增 `TrafficEntityType`
2. traffic DTO 从 `rule_id` 改为 `entity_type + entity_id`
3. SQLite schema migration

### Step 2：后端统计器改造

1. `TrafficTracker` 改为多实体
2. Legacy Rules recorder 迁移
3. Proxy Upstream recorder 接入

### Step 3：首页接口改造

1. 新增统一的窗口数据查询接口
2. 新增首页监控对象列表接口或由前端组合现有数据

### Step 4：首页 UI 改造

1. 模块标题改为 `流量监控`
2. 配置面板从 `规则` 扩为 `监控对象`
3. 混合绘图与汇总

### Step 5：验证与验收

1. Rust 单元测试覆盖 Legacy Rule 与 Proxy Upstream 两类实体
2. 前端验证混合选择、混合绘图、汇总一致性
3. 历史统计迁移验证

## 14. 验收标准

1. 首页模块标题显示为 `流量监控`
2. 可同时选择 Rules 与 Proxy Upstream
3. 选中对象可混合绘制在同一图表
4. 汇总值为当前选中对象总和
5. Proxy HTTP/HTTPS upstream 能正确统计流量与 requests
6. Proxy WS/WSS/gRPC/gRPCS 能正确统计流量与连接数，且首页不展示 requests
7. 旧 Legacy Rules 历史流量数据迁移后可继续查询
8. 大于 3 个对象时不再被前端强制限制

## 15. 开发结论

本需求建议按“统一流量实体模型 + 首页混合展示”落地，而不是为 Proxy 单独拼接一套首页图表逻辑。这样可以：

1. 复用现有流量监控交互
2. 避免 Rules / Proxy 两套统计系统长期分裂
3. 为后续增加 Route / Listener 聚合视图预留空间
